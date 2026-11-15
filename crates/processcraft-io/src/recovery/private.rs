use std::{
    fs, io,
    os::windows::{
        ffi::OsStrExt,
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::Path,
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::Threading::*,
};

// This adapter creates a protected DACL containing only the process user's SID,
// then independently checks the owner, control bits and sole inheritable ACE.
pub fn create_or_verify(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing recovery parent"))?;
    fs::create_dir_all(parent)?;
    let _pins = crate::save::windows::pin_directories(path)?;
    let mut token = std::ptr::null_mut();
    // SAFETY: `token` is a valid out-pointer; the pseudo-handle from GetCurrentProcess needs no close.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: OpenProcessToken succeeded, so `token` is an owned handle that nothing else closes.
    let token = unsafe { std::fs::File::from_raw_handle(token) };
    let mut size = 0;
    // SAFETY: size query only: a null buffer with length 0 is allowed and `size` is a valid out-pointer.
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut size,
        );
    }
    if size == 0 {
        return Err(io::Error::last_os_error());
    }
    // usize elements keep the buffer pointer-aligned for the TOKEN_USER read below.
    let mut buf = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
    // SAFETY: `buf` holds at least `size` writable bytes, as just reported by the size query.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buf.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `buf` is aligned and was filled with a TOKEN_USER by the successful call above;
    // the SID it points to lives inside `buf`, which outlives every use of `sid`.
    let sid = unsafe { (*(buf.as_ptr() as *const TOKEN_USER)).User.Sid };
    let mut sid_text = std::ptr::null_mut();
    // SAFETY: `sid` is a valid SID and `sid_text` a valid out-pointer.
    if unsafe { ConvertSidToStringSidW(sid, &mut sid_text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut len = 0;
    // SAFETY: on success `sid_text` is a NUL-terminated UTF-16 string allocated with LocalAlloc;
    // the scan stops at the terminator, the slice covers exactly the scanned `len` units, and
    // the string is freed once, after the copy into `sid_string`.
    let sid_string = unsafe {
        while *sid_text.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(sid_text, len));
        LocalFree(sid_text.cast());
        text
    };
    let sddl: Vec<u16> = format!("O:{sid_string}D:P(A;OICI;FA;;;{sid_string})\0")
        .encode_utf16()
        .collect();
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: `sddl` is NUL-terminated; `descriptor` is a valid out-pointer and the size out-pointer may be null.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let attrs = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: `wide` is NUL-terminated and `attrs` points at the live descriptor created above.
    let created = unsafe { CreateDirectoryW(wide.as_ptr(), &attrs) };
    let error = if created == 0 {
        Some(io::Error::last_os_error())
    } else {
        None
    };
    // SAFETY: `descriptor` was allocated by the conversion call, is no longer referenced, and is freed once.
    unsafe { LocalFree(descriptor.cast()) };
    if let Some(error) = error
        && error.kind() != io::ErrorKind::AlreadyExists
    {
        return Err(error);
    }
    let pins = crate::save::windows::pin_directories(&path.join("pin"))?;
    let _ = pins
        .first()
        .ok_or_else(|| io::Error::other("missing directory handle"))?;
    let dir = fs::OpenOptions::new()
        .read(true)
        .access_mode(0x00020000)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let mut needed = 0;
    // SAFETY: size query only: a null descriptor with length 0 is allowed; `dir` is a live handle.
    unsafe {
        GetKernelObjectSecurity(
            dir.as_raw_handle(),
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    if needed == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut sec = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
    let ptr = sec.as_mut_ptr().cast();
    // SAFETY: `sec` holds at least `needed` writable, pointer-aligned bytes; `dir` is a live handle.
    if unsafe {
        GetKernelObjectSecurity(
            dir.as_raw_handle(),
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            ptr,
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut owner = std::ptr::null_mut();
    let mut defaulted = 0;
    let mut control = 0;
    let mut revision = 0;
    let mut present = 0;
    let mut acl = std::ptr::null_mut();
    // SAFETY: `ptr` is the self-relative descriptor filled above (kept alive by `sec`) and every
    // out-pointer is valid; `owner` and `acl` end up pointing into that buffer.
    let read = unsafe {
        GetSecurityDescriptorOwner(ptr, &mut owner, &mut defaulted) != 0
            && GetSecurityDescriptorControl(ptr, &mut control, &mut revision) != 0
            && GetSecurityDescriptorDacl(ptr, &mut present, &mut acl, &mut defaulted) != 0
    };
    if !read {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `owner` is non-null (short-circuit) and `sid` is valid; `acl` is dereferenced only
    // after the `present`/non-null checks; both point into `sec`, which is still alive.
    let untrusted = owner.is_null()
        || unsafe { EqualSid(owner, sid) } == 0
        || control & SE_DACL_PROTECTED == 0
        || present == 0
        || acl.is_null()
        || unsafe { (*acl).AceCount } != 1;
    if untrusted {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "recovery directory owner or DACL untrusted",
        ));
    }
    let mut ace = std::ptr::null_mut();
    // SAFETY: `acl` is a valid ACL with exactly one ACE (checked above); `ace` is a valid out-pointer.
    if unsafe { GetAce(acl, 0, &mut ace) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: GetAce returned a pointer to the ACE inside `sec`; its header is read first and the
    // type is verified (`AceType == 0`, access-allowed) before the SID is used.
    let ace = unsafe { &*(ace as *const ACCESS_ALLOWED_ACE) };
    // SAFETY: the SID begins at `SidStart` inside the ACE in `sec`; `sid` lives in `buf`.
    let ace_untrusted = ace.Header.AceType != 0
        || ace.Header.AceFlags != (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE) as u8
        || ace.Mask != FILE_ALL_ACCESS
        || unsafe { EqualSid((&ace.SidStart as *const u32).cast_mut().cast(), sid) } == 0;
    if ace_untrusted {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "recovery directory DACL untrusted",
        ));
    }
    Ok(())
}
