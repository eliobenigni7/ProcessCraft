use std::fs::File;
use std::io;
use std::path::{Component, Path, PathBuf, Prefix};

/// Authority supplied by the trusted entry point, never by document content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    ReadOnly,
    Write,
}

/// Capability for exactly one local file. Creating a capability writes nothing.
/// The caller is responsible for obtaining user/automation authorization first.
/// M0 supports ordinary NTFS files under trusted directories only. Ancestor
/// handles prevent their rename, but this is not a sandbox against another
/// process running as the same user and deliberately swapping final entries.
#[derive(Debug)]
pub struct FileGrant {
    pub(crate) canonical_path: PathBuf,
    display_path: PathBuf,
    pub(crate) access: Access,
    pub(crate) _directories: Vec<File>,
}

impl FileGrant {
    /// Read only the granted file, subject to the native archive size limit.
    pub fn read(&self) -> io::Result<Vec<u8>> {
        crate::save::read_target(&self.canonical_path)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "granted file missing"))
    }

    /// Read at most `max` bytes from the opened file, rejecting larger streams.
    pub fn read_limited(&self, max: usize) -> io::Result<Vec<u8>> {
        use std::io::Read;
        let mut file = crate::save::windows::open_target(&self.canonical_path)?;
        let mut bytes = Vec::new();
        (&mut file)
            .take((max as u64).saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() > max {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "file exceeds limit",
            ));
        }
        Ok(bytes)
    }

    /// Compare pinned targets, including case, short-name and hard-link aliases.
    pub fn same_target(&self, other: &Self) -> io::Result<bool> {
        if self
            .canonical_path
            .to_string_lossy()
            .eq_ignore_ascii_case(&other.canonical_path.to_string_lossy())
        {
            return Ok(true);
        }
        crate::save::windows::same_file(&self.canonical_path, &other.canonical_path)
    }
    pub fn path(&self) -> &Path {
        &self.display_path
    }

    pub fn new(path: impl AsRef<Path>, access: Access) -> io::Result<Self> {
        let path = path.as_ref();
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "absolute file path required",
            ));
        }
        // Reject UNC/device paths, ADS, dot traversal, and all link-bearing paths.
        for (index, component) in path.components().enumerate() {
            match component {
                Component::Prefix(prefix)
                    if index == 0 && matches!(prefix.kind(), Prefix::Disk(_)) => {}
                Component::RootDir => {}
                Component::Normal(name) if valid_name(name) => {}
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "unsupported grant path",
                    ));
                }
            }
        }
        let directories = crate::save::windows::pin_directories(path)?;
        // Validate before canonicalizing, so canonicalization cannot hide a
        // followed reparse point. Unify NTFS case/8.3 aliases for sidecar naming.
        let canonical = match crate::save::windows::open_target(path) {
            Ok(file) => crate::save::windows::canonical_file(&file)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => std::fs::canonicalize(
                path.parent()
                    .ok_or_else(|| io::Error::other("missing parent"))?,
            )?
            .join(
                path.file_name()
                    .ok_or_else(|| io::Error::other("missing name"))?,
            ),
            Err(error) => return Err(error),
        };
        Ok(Self {
            canonical_path: canonical,
            display_path: path.to_path_buf(),
            access,
            _directories: directories,
        })
    }
}

fn valid_name(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    if name.ends_with([' ', '.']) || name.chars().any(|c| c < ' ' || "<>:\"/\\|?*".contains(c)) {
        return false;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "COM¹" | "COM²" | "COM³" | "LPT¹" | "LPT²" | "LPT³"
    ) && !(stem.len() == 4
        && (stem.starts_with("COM") || stem.starts_with("LPT"))
        && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}
