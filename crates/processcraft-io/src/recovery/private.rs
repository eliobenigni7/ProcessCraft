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
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = std::fs::File::from_raw_handle(token);
        let mut size = 0;
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut size,
        );
        if size == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buf = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
        if GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buf.as_mut_ptr().cast(),
            size,
            &mut size,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let sid = (*(buf.as_ptr() as *const TOKEN_USER)).User.Sid;
        let mut sid_text = std::ptr::null_mut();
        if ConvertSidToStringSidW(sid, &mut sid_text) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0;
        while *sid_text.add(len) != 0 {
            len += 1;
        }
        let sid_string = String::from_utf16_lossy(std::slice::from_raw_parts(sid_text, len));
        LocalFree(sid_text.cast());
        let sddl: Vec<u16> = format!("O:{sid_string}D:P(A;OICI;FA;;;{sid_string})\0")
            .encode_utf16()
            .collect();
        let mut descriptor = std::ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let attrs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let created = CreateDirectoryW(wide.as_ptr(), &attrs);
        let error = if created == 0 {
            Some(io::Error::last_os_error())
        } else {
            None
        };
        LocalFree(descriptor.cast());
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
        GetKernelObjectSecurity(
            dir.as_raw_handle(),
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
        if needed == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut sec = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        let ptr = sec.as_mut_ptr().cast();
        if GetKernelObjectSecurity(
            dir.as_raw_handle(),
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            ptr,
            needed,
            &mut needed,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut owner = std::ptr::null_mut();
        let mut defaulted = 0;
        let mut control = 0;
        let mut revision = 0;
        let mut present = 0;
        let mut acl = std::ptr::null_mut();
        if GetSecurityDescriptorOwner(ptr, &mut owner, &mut defaulted) == 0
            || GetSecurityDescriptorControl(ptr, &mut control, &mut revision) == 0
            || GetSecurityDescriptorDacl(ptr, &mut present, &mut acl, &mut defaulted) == 0
        {
            return Err(io::Error::last_os_error());
        }
        if owner.is_null()
            || EqualSid(owner, sid) == 0
            || control & SE_DACL_PROTECTED == 0
            || present == 0
            || acl.is_null()
            || (*acl).AceCount != 1
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "recovery directory owner or DACL untrusted",
            ));
        }
        let mut ace = std::ptr::null_mut();
        if GetAce(acl, 0, &mut ace) == 0 {
            return Err(io::Error::last_os_error());
        }
        let ace = &*(ace as *const ACCESS_ALLOWED_ACE);
        if ace.Header.AceType != 0
            || ace.Header.AceFlags != (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE) as u8
            || ace.Mask != FILE_ALL_ACCESS
            || EqualSid((&ace.SidStart as *const u32).cast_mut().cast(), sid) == 0
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "recovery directory DACL untrusted",
            ));
        }
        Ok(())
    }
}
