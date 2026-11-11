use crate::{Access, FileGrant};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const MAX_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug)]
pub enum OpenSession {
    Writable(WriterSession),
    ReadOnly,
}

/// Never blindly retry Indeterminate: reopen/compare the target and retained
/// artifacts first. A post-commit warning is still a committed save.
#[derive(Debug)]
pub enum SaveOutcome {
    NotCommitted {
        error: io::Error,
    },
    Committed {
        warning: Option<io::Error>,
    },
    Indeterminate {
        error: io::Error,
        stage: PathBuf,
        backup: PathBuf,
    },
}

/// Synchronous observation/cancellation points; also deterministic fault hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavePoint {
    StageCreated,
    Written,
    Flushed,
    BeforeReplace,
    Replaced,
}

#[derive(Debug)]
pub struct WriterSession {
    path: PathBuf,
    _lock: File,
    _directories: Vec<File>,
    expected: Option<Vec<u8>>,
    uncertain: bool,
}

impl WriterSession {
    pub fn open(grant: &FileGrant) -> io::Result<OpenSession> {
        if grant.access != Access::Write {
            return Ok(OpenSession::ReadOnly);
        }
        let directories = windows::pin_directories(&grant.canonical_path)?;
        let mut lock_path = grant.canonical_path.as_os_str().to_os_string();
        lock_path.push(".lock");
        let lock = windows::open_lock(Path::new(&lock_path))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Ok(OpenSession::ReadOnly),
            Err(std::fs::TryLockError::Error(error)) => return Err(error),
        }
        let expected = read_target(&grant.canonical_path)?;
        Ok(OpenSession::Writable(Self {
            path: grant.canonical_path.clone(),
            _lock: lock,
            _directories: directories,
            expected,
            uncertain: false,
        }))
    }

    pub(crate) fn snapshot(&self) -> Option<&[u8]> {
        self.expected.as_deref()
    }

    /// Resolve an uncertain raw-file write against its retained intended bytes.
    /// An error retains uncertainty; callers must not blindly retry replacement.
    pub fn reconcile(&mut self, expected: &[u8]) -> io::Result<bool> {
        let current = read_target(&self.path)?;
        if current.as_deref() == Some(expected) && current == self.expected {
            return Err(io::Error::other(
                "target matches both original and intended bytes; commit remains indeterminate",
            ));
        }
        if current.as_deref() == Some(expected) {
            self.expected = current;
            self.uncertain = false;
            Ok(true)
        } else if current == self.expected {
            self.uncertain = false;
            Ok(false)
        } else {
            Err(io::Error::other(
                "target differs from both saved and original bytes",
            ))
        }
    }

    pub fn save(&mut self, bytes: &[u8]) -> SaveOutcome {
        self.save_with_hook(bytes, |_, _| Ok(()))
    }

    /// The callback may inspect stage bytes; do not mutate files from it in
    /// production. Panics can leave an orphan stage, as can process termination.
    pub fn save_with_hook(
        &mut self,
        bytes: &[u8],
        mut hook: impl FnMut(SavePoint, &Path) -> io::Result<()>,
    ) -> SaveOutcome {
        self.save_impl(bytes, &mut hook, windows::replace)
    }

    pub(crate) fn save_impl(
        &mut self,
        bytes: &[u8],
        hook: &mut impl FnMut(SavePoint, &Path) -> io::Result<()>,
        replace: impl FnOnce(&Path, &Path, &Path, bool) -> io::Result<()>,
    ) -> SaveOutcome {
        let fail = |error| SaveOutcome::NotCommitted { error };
        if self.uncertain {
            return fail(io::Error::other(
                "reopen and compare after indeterminate save",
            ));
        }
        if bytes.len() > MAX_BYTES {
            return fail(io::Error::new(
                io::ErrorKind::InvalidInput,
                "128 MiB output limit",
            ));
        }
        let parent = match self.path.parent() {
            Some(p) => p,
            None => return fail(io::Error::other("missing parent")),
        };
        let stage = parent.join(format!(".processcraft-{}.stage", uuid::Uuid::new_v4()));
        let backup = parent.join(format!(".processcraft-{}.backup", uuid::Uuid::new_v4()));
        let mut owns_stage = false;
        let mut prepare = || -> io::Result<()> {
            if read_target(&self.path)? != self.expected {
                return Err(io::Error::other("target changed outside writer session"));
            }
            let mut file = windows::create_stage(&stage)?;
            owns_stage = true;
            windows::copy_dacl(&file, &self.path, self.expected.is_some())?;
            windows::copy_attributes(&stage, &self.path, self.expected.is_some())?;
            hook(SavePoint::StageCreated, &stage)?;
            file.write_all(bytes)?;
            hook(SavePoint::Written, &stage)?;
            file.sync_all()?;
            hook(SavePoint::Flushed, &stage)?;
            drop(file);
            hook(SavePoint::BeforeReplace, &stage)?;
            // Recheck after potentially long staging; the sidecar excludes
            // cooperating writers, not arbitrary external programs.
            let _pins = windows::pin_directories(&self.path)?;
            if read_target(&self.path)? != self.expected {
                return Err(io::Error::other("target changed before replacement"));
            }
            if read_target(&stage)?.as_deref() != Some(bytes) {
                return Err(io::Error::other("stage verification failed"));
            }
            Ok(())
        };
        if let Err(error) = prepare() {
            if owns_stage {
                let _ = fs::remove_file(&stage);
            }
            return fail(error);
        }
        if let Err(error) = replace(&stage, &self.path, &backup, self.expected.is_some()) {
            // ReplaceFileW can fail after changing names. Only classify a
            // failure as untouched when all three observable facts agree.
            if read_target(&self.path).ok().as_ref() == Some(&self.expected)
                && fs::read(&stage).ok().as_deref() == Some(bytes)
                && !backup.exists()
            {
                let _ = fs::remove_file(&stage);
                return fail(error);
            }
            self.uncertain = true;
            return SaveOutcome::Indeterminate {
                error,
                stage,
                backup,
            };
        }
        self.expected = Some(bytes.to_vec());
        let mut warning = hook(SavePoint::Replaced, &self.path).err();
        if backup.exists()
            && let Err(error) = fs::remove_file(&backup)
        {
            warning = Some(error);
        }
        SaveOutcome::Committed { warning }
    }
}

#[cfg(not(windows))]
pub(crate) mod windows {
    use super::*;
    fn unsupported<T>() -> io::Result<T> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "M0 replacement is qualified only on Windows NTFS",
        ))
    }
    pub(crate) fn pin_directories(_: &Path) -> io::Result<Vec<File>> {
        unsupported()
    }
    pub(crate) fn open_target(_: &Path) -> io::Result<File> {
        unsupported()
    }
    pub(crate) fn canonical_file(_: &File) -> io::Result<PathBuf> {
        unsupported()
    }
    pub(crate) fn same_file(_: &Path, _: &Path) -> io::Result<bool> {
        unsupported()
    }
    pub(crate) fn open_lock(_: &Path) -> io::Result<File> {
        unsupported()
    }
    pub(crate) fn create_stage(_: &Path) -> io::Result<File> {
        unsupported()
    }
    pub(crate) fn copy_attributes(_: &Path, _: &Path, _: bool) -> io::Result<()> {
        unsupported()
    }
    pub(crate) fn copy_dacl(_: &File, _: &Path, _: bool) -> io::Result<()> {
        unsupported()
    }
    pub(crate) fn replace(_: &Path, _: &Path, _: &Path, _: bool) -> io::Result<()> {
        unsupported()
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn injected_ambiguous_replacement_retains_artifacts_and_blocks_retry() {
        let directory = std::env::temp_dir().join(format!("processcraft-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let target = directory.join("test.processcraft");
        fs::write(&target, b"old").unwrap();
        let grant = FileGrant::new(&target, Access::Write).unwrap();
        let OpenSession::Writable(mut writer) = WriterSession::open(&grant).unwrap() else {
            panic!("writer")
        };
        // Inject a Win32-style late failure after a name was changed. This is
        // not evidence that the host physically reproduced ERROR 1176/1177.
        let outcome = writer.save_impl(b"new", &mut |_, _| Ok(()), |_, target, backup, _| {
            fs::rename(target, backup)?;
            Err(io::Error::from_raw_os_error(1176))
        });
        let SaveOutcome::Indeterminate { stage, backup, .. } = outcome else {
            panic!("expected indeterminate")
        };
        assert_eq!(fs::read(stage).unwrap(), b"new");
        assert_eq!(fs::read(&backup).unwrap(), b"old");
        assert!(!target.exists());
        assert!(matches!(
            writer.save(b"blind retry"),
            SaveOutcome::NotCommitted { .. }
        ));
        assert!(writer.reconcile(b"new").is_err());
        fs::rename(&backup, &target).unwrap();
        assert!(!writer.reconcile(b"new").unwrap());
        assert!(matches!(writer.save(b"new"), SaveOutcome::Committed { .. }));
        drop(writer);
        drop(grant);
        fs::remove_dir_all(directory).unwrap();
    }
}

pub(crate) fn read_target(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let mut file = match windows::open_target(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "128 MiB target limit",
        ));
    }
    Ok(Some(bytes))
}

/// Reviewed exception to the crate's unsafe-code denial. Every pointer is
/// derived from a live Rust buffer or file handle and no ownership is transferred.
#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) mod windows {
    use super::*;
    use std::os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    };
    use windows_sys::Win32::{Security::*, Storage::FileSystem::*};
    const SHARE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE;
    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        let mut wide: Vec<_> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"));
        }
        wide.push(0);
        Ok(wide)
    }
    fn checked(file: File, directory: bool) -> io::Result<File> {
        let metadata = file.metadata()?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || metadata.is_dir() != directory
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "reparse/link or unexpected file kind",
            ));
        }
        if !directory {
            // Refuse read-only, encrypted, compressed, sparse and other special
            // attributes instead of claiming preservation without qualification.
            if metadata.file_attributes()
                & !(FILE_ATTRIBUTE_ARCHIVE
                    | FILE_ATTRIBUTE_NORMAL
                    | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED)
                != 0
            {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "unsupported file attributes",
                ));
            }
            let mut info = BY_HANDLE_FILE_INFORMATION::default();
            // SAFETY: valid open handle and aligned live output structure.
            if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
                return Err(io::Error::last_os_error());
            }
            if info.nNumberOfLinks != 1 {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "hard-linked files are not supported",
                ));
            }
        }
        Ok(file)
    }
    pub(crate) fn pin_directories(path: &Path) -> io::Result<Vec<File>> {
        let root = path
            .ancestors()
            .last()
            .ok_or_else(|| io::Error::other("missing root"))?;
        let root_w = wide(root)?;
        // SAFETY: live terminated root string. Remote, removable, and unknown
        // drive kinds are deliberately outside the qualified local boundary.
        if unsafe { GetDriveTypeW(root_w.as_ptr()) } != 3 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "fixed local drive required",
            ));
        }
        let mut filesystem = [0u16; 32];
        // SAFETY: terminated root string; writable fixed buffer and length match;
        // optional output pointers are documented nullable.
        if unsafe {
            GetVolumeInformationW(
                root_w.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                filesystem.as_mut_ptr(),
                filesystem.len() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if String::from_utf16_lossy(&filesystem[..4]) != "NTFS" {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "only local NTFS qualified",
            ));
        }
        path.parent()
            .ok_or_else(|| io::Error::other("missing parent"))?
            .ancestors()
            .map(|directory| {
                let file = OpenOptions::new()
                    .read(true)
                    .access_mode(FILE_LIST_DIRECTORY)
                    .share_mode(SHARE)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                    .open(directory)?;
                checked(file, true)
            })
            .collect()
    }
    pub(crate) fn open_target(path: &Path) -> io::Result<File> {
        checked(
            OpenOptions::new()
                .read(true)
                .share_mode(SHARE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(path)?,
            false,
        )
    }
    pub(crate) fn same_file(a: &Path, b: &Path) -> io::Result<bool> {
        let first = open_target(a)?;
        let second = match open_target(b) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        let mut one: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        let mut two: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(first.as_raw_handle(), &mut one) } == 0
            || unsafe { GetFileInformationByHandle(second.as_raw_handle(), &mut two) } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(one.dwVolumeSerialNumber == two.dwVolumeSerialNumber
            && one.nFileIndexHigh == two.nFileIndexHigh
            && one.nFileIndexLow == two.nFileIndexLow)
    }
    pub(crate) fn canonical_file(file: &File) -> io::Result<PathBuf> {
        use std::os::windows::ffi::OsStringExt;
        // SAFETY: live handle; null buffer with zero length asks required size.
        let required =
            unsafe { GetFinalPathNameByHandleW(file.as_raw_handle(), std::ptr::null_mut(), 0, 0) };
        if required == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buffer = vec![0u16; required as usize + 1];
        // SAFETY: buffer length matches the supplied writable capacity. The
        // name comes from the already validated handle, avoiding a path-based
        // canonicalization that could follow a swapped final symlink.
        let length = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                0,
            )
        };
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        if length as usize >= buffer.len() {
            return Err(io::Error::other("file name changed during resolution"));
        }
        Ok(PathBuf::from(std::ffi::OsString::from_wide(
            &buffer[..length as usize],
        )))
    }
    pub(crate) fn open_lock(path: &Path) -> io::Result<File> {
        checked(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .share_mode(SHARE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(path)?,
            false,
        )
    }
    pub(crate) fn create_stage(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .access_mode(0x80000000 | 0x40000000 | 0x00040000)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    }
    pub(crate) fn copy_attributes(stage: &Path, target: &Path, exists: bool) -> io::Result<()> {
        if exists {
            let attributes = open_target(target)?.metadata()?.file_attributes();
            let stage_w = wide(stage)?;
            // SAFETY: stage_w is a live, NUL-terminated path.
            if unsafe {
                SetFileAttributesW(
                    stage_w.as_ptr(),
                    FILE_ATTRIBUTE_ARCHIVE | (attributes & FILE_ATTRIBUTE_NOT_CONTENT_INDEXED),
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
    pub(crate) fn copy_dacl(file: &File, target: &Path, exists: bool) -> io::Result<()> {
        if exists {
            let original = open_target(target)?;
            let mut descriptor = security_descriptor(
                &original,
                DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION,
            )?;
            let mut stage_descriptor = security_descriptor(
                file,
                OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION,
            )?;
            let mut original_owner = std::ptr::null_mut();
            let mut stage_owner = std::ptr::null_mut();
            let mut original_group = std::ptr::null_mut();
            let mut stage_group = std::ptr::null_mut();
            let mut defaulted = 0;
            // SAFETY: Windows-produced descriptors remain live and aligned;
            // outputs point inside them. All output pointers are valid.
            let valid = unsafe {
                GetSecurityDescriptorOwner(
                    descriptor.as_mut_ptr().cast(),
                    &mut original_owner,
                    &mut defaulted,
                ) != 0
                    && GetSecurityDescriptorOwner(
                        stage_descriptor.as_mut_ptr().cast(),
                        &mut stage_owner,
                        &mut defaulted,
                    ) != 0
                    && GetSecurityDescriptorGroup(
                        descriptor.as_mut_ptr().cast(),
                        &mut original_group,
                        &mut defaulted,
                    ) != 0
                    && GetSecurityDescriptorGroup(
                        stage_descriptor.as_mut_ptr().cast(),
                        &mut stage_group,
                        &mut defaulted,
                    ) != 0
            };
            if !valid {
                return Err(io::Error::last_os_error());
            }
            if original_owner.is_null()
                || stage_owner.is_null()
                || original_group.is_null()
                || stage_group.is_null()
            {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "missing owner/group",
                ));
            }
            // SAFETY: non-null SID pointers produced by the successful getter
            // calls above; backing descriptors have not moved or been freed.
            if unsafe {
                EqualSid(original_owner, stage_owner) == 0
                    || EqualSid(original_group, stage_group) == 0
            } {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "owner/group preservation not qualified",
                ));
            }
            // SAFETY: descriptor was produced by Windows above. Protecting the
            // stage ACL avoids inheriting broader directory permissions.
            if unsafe {
                SetKernelObjectSecurity(
                    file.as_raw_handle(),
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    descriptor.as_mut_ptr().cast(),
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
    fn security_descriptor(file: &File, information: u32) -> io::Result<Vec<u32>> {
        let mut needed = 0;
        // SAFETY: first call only asks required size; handle is live.
        unsafe {
            GetKernelObjectSecurity(
                file.as_raw_handle(),
                information,
                std::ptr::null_mut(),
                0,
                &mut needed,
            );
        }
        if needed == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut descriptor = vec![0u32; (needed as usize).div_ceil(4)];
        // SAFETY: aligned buffer is at least needed bytes; handles and size
        // output remain live. Copy the DACL before any payload is written.
        if unsafe {
            GetKernelObjectSecurity(
                file.as_raw_handle(),
                information,
                descriptor.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(descriptor)
    }
    pub(crate) fn replace(
        stage: &Path,
        target: &Path,
        backup: &Path,
        exists: bool,
    ) -> io::Result<()> {
        let (stage, target, backup) = (wide(stage)?, wide(target)?, wide(backup)?);
        // SAFETY: live terminated UTF-16 buffers. No merge/ACL errors are
        // ignored. A unique backup preserves evidence of partial Win32 failure.
        let result = unsafe {
            if exists {
                ReplaceFileW(
                    target.as_ptr(),
                    stage.as_ptr(),
                    backup.as_ptr(),
                    0,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            } else {
                MoveFileExW(stage.as_ptr(), target.as_ptr(), 0)
            }
        };
        if result == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}
