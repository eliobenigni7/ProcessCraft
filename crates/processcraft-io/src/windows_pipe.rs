//! Explicitly enabled, owner-only Windows named-pipe transport.
#![allow(unsafe_code)]
use crate::local_ipc::{AttachmentRequest, MAX_FRAME_BYTES, QueueSender};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io,
    os::windows::io::{AsRawHandle, FromRawHandle},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::{Pipes::*, Threading::*},
};

const TIMEOUT: Duration = Duration::from_secs(15);
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRequest {
    token: String,
    request: AttachmentRequest,
}

pub struct Server {
    endpoint: String,
    token: String,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    sender: QueueSender,
}
impl Server {
    pub fn start(sender: QueueSender) -> io::Result<Self> {
        let endpoint = format!(r"\\.\pipe\processcraft-{}", uuid::Uuid::new_v4());
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let pipe = create_pipe(&endpoint)?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_sender = sender.clone();
        let worker_token = token.clone();
        let worker_endpoint = endpoint.clone();
        let worker = thread::spawn(move || {
            serve(
                pipe,
                &worker_endpoint,
                &worker_token,
                worker_sender,
                worker_stop,
            )
        });
        Ok(Self {
            endpoint,
            token,
            stop,
            worker: Some(worker),
            sender,
        })
    }
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn token(&self) -> &str {
        &self.token
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.sender.revoke();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn create_pipe(name: &str) -> io::Result<std::fs::File> {
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
        let mut text = std::ptr::null_mut();
        if ConvertSidToStringSidW(sid, &mut text) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0;
        while *text.add(len) != 0 {
            len += 1;
        }
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        let sddl = wide(&format!("O:{sid}D:P(A;;GA;;;{sid})"));
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
        let handle = CreateNamedPipeW(
            wide(name).as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            65536,
            65536,
            0,
            &attrs,
        );
        let err = if handle == INVALID_HANDLE_VALUE {
            Some(io::Error::last_os_error())
        } else {
            None
        };
        LocalFree(descriptor.cast());
        if let Some(err) = err {
            return Err(err);
        }
        Ok(std::fs::File::from_raw_handle(handle))
    }
}
fn paused(stop: &AtomicBool, deadline: Instant) -> io::Result<()> {
    if stop.load(Ordering::Acquire) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "attachment stopped",
        ));
    }
    if Instant::now() >= deadline {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "attachment timed out",
        ));
    }
    thread::sleep(Duration::from_millis(5));
    Ok(())
}
fn transfer(
    file: &std::fs::File,
    data: &mut [u8],
    writing: bool,
    stop: &AtomicBool,
    deadline: Instant,
) -> io::Result<()> {
    let mut offset = 0;
    while offset < data.len() {
        if stop.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "attachment stopped",
            ));
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "attachment timed out",
            ));
        }
        let mut count = 0;
        let ok = unsafe {
            if writing {
                WriteFile(
                    file.as_raw_handle(),
                    data[offset..].as_ptr(),
                    (data.len() - offset) as u32,
                    &mut count,
                    std::ptr::null_mut(),
                )
            } else {
                ReadFile(
                    file.as_raw_handle(),
                    data[offset..].as_mut_ptr(),
                    (data.len() - offset) as u32,
                    &mut count,
                    std::ptr::null_mut(),
                )
            }
        };
        if ok != 0 && count > 0 {
            offset += count as usize;
            continue;
        }
        if ok == 0 {
            let e = io::Error::last_os_error();
            match e.raw_os_error().map(|v| v as u32) {
                Some(ERROR_NO_DATA | ERROR_PIPE_LISTENING | ERROR_PIPE_BUSY) => {}
                _ => return Err(e),
            }
        }
        paused(stop, deadline)?;
    }
    Ok(())
}
fn read_frame(file: &std::fs::File, stop: &AtomicBool, deadline: Instant) -> io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    transfer(file, &mut len, false, stop, deadline)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut data = vec![0; len];
    transfer(file, &mut data, false, stop, deadline)?;
    Ok(data)
}
fn write_frame(
    file: &std::fs::File,
    data: &[u8],
    stop: &AtomicBool,
    deadline: Instant,
) -> io::Result<()> {
    if data.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    transfer(
        file,
        &mut (data.len() as u32).to_le_bytes(),
        true,
        stop,
        deadline,
    )?;
    transfer(file, &mut data.to_vec(), true, stop, deadline)
}
fn serve(
    pipe: std::fs::File,
    _endpoint: &str,
    token: &str,
    sender: QueueSender,
    stop: Arc<AtomicBool>,
) {
    // ponytail: One client at a time bounds resources; expand only if needed.
    while !stop.load(Ordering::Acquire) {
        let connected = unsafe { ConnectNamedPipe(pipe.as_raw_handle(), std::ptr::null_mut()) };
        if connected == 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error().map(|v| v as u32) != Some(ERROR_PIPE_CONNECTED) {
                thread::sleep(Duration::from_millis(5));
                continue;
            }
        }
        let deadline = Instant::now() + TIMEOUT;
        let result = (|| -> io::Result<()> {
            let data = read_frame(&pipe, &stop, deadline)?;
            if crate::strict_json::has_duplicate_keys(&data) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "duplicate JSON key",
                ));
            }
            let request: WireRequest = serde_json::from_slice(&data).map_err(io::Error::other)?;
            if request.token != token {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "authentication failed",
                ));
            }
            let rx = sender
                .submit(request.request)
                .map_err(|e| io::Error::other(format!("queue: {e:?}")))?;
            let response = loop {
                match rx.try_recv() {
                    Ok(v) => break v,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        return Err(io::Error::other("owner disconnected"));
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => paused(&stop, deadline)?,
                }
            };
            write_frame(
                &pipe,
                &serde_json::to_vec(&response).map_err(io::Error::other)?,
                &stop,
                deadline,
            )?;
            transfer(&pipe, &mut [0u8; 1], false, &stop, deadline)
        })();
        let _ = result;
        unsafe {
            DisconnectNamedPipe(pipe.as_raw_handle());
        }
        if stop.load(Ordering::Acquire) {
            break;
        }
    }
}
pub fn call(endpoint: &str, token: &str, request: &AttachmentRequest) -> io::Result<Value> {
    let id = endpoint
        .strip_prefix(r"\\.\pipe\processcraft-")
        .and_then(|id| uuid::Uuid::parse_str(id).ok());
    if id.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid local attachment endpoint",
        ));
    }
    let stop = AtomicBool::new(false);
    let deadline = Instant::now() + TIMEOUT;
    let handle = loop {
        let handle = unsafe {
            CreateFileW(
                wide(endpoint).as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            break handle;
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error().map(|v| v as u32) != Some(ERROR_PIPE_BUSY) {
            return Err(error);
        }
        paused(&stop, deadline)?;
    };
    let file = unsafe { std::fs::File::from_raw_handle(handle) };
    let mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
    if unsafe {
        SetNamedPipeHandleState(
            file.as_raw_handle(),
            &mode,
            std::ptr::null(),
            std::ptr::null(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let data = serde_json::to_vec(&WireRequest {
        token: token.to_owned(),
        request: request.clone(),
    })
    .map_err(io::Error::other)?;
    write_frame(&file, &data, &stop, deadline)?;
    let result =
        serde_json::from_slice(&read_frame(&file, &stop, deadline)?).map_err(io::Error::other)?;
    transfer(&file, &mut [1u8], true, &stop, deadline)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_ipc::owner_queue;
    fn req() -> AttachmentRequest {
        AttachmentRequest {
            session_id: "session".into(),
            request_id: "1".into(),
            expected_generation: 0,
            method: "inspect".into(),
            params: Value::Null,
        }
    }
    #[test]
    fn authenticated_round_trip_and_revoke() {
        let (sender, owner) = owner_queue("session".into(), 2);
        let server = Server::start(sender.clone()).unwrap();
        let endpoint = server.endpoint().to_owned();
        let token = server.token().to_owned();
        assert!(create_pipe(&endpoint).is_err());
        assert!(call(&endpoint, "wrong", &req()).is_err());
        assert!(owner.try_recv().is_none());
        let client = thread::spawn(move || call(&endpoint, &token, &req()));
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(pending) = owner.try_recv() {
                pending.reply(serde_json::json!({"ok":true}));
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(client.join().unwrap().unwrap()["ok"], true);
        let start = Instant::now();
        drop(server);
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(sender.submit(req()).is_err());
    }
    #[test]
    fn oversized_frame_rejected_before_allocation() {
        let (sender, owner) = owner_queue("session".into(), 1);
        let server = Server::start(sender).unwrap();
        let handle = unsafe {
            CreateFileW(
                wide(server.endpoint()).as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(handle, INVALID_HANDLE_VALUE);
        let file = unsafe { std::fs::File::from_raw_handle(handle) };
        let mode = PIPE_NOWAIT;
        assert_ne!(
            unsafe {
                SetNamedPipeHandleState(
                    file.as_raw_handle(),
                    &mode,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            0
        );
        transfer(
            &file,
            &mut ((MAX_FRAME_BYTES as u32) + 1).to_le_bytes(),
            true,
            &AtomicBool::new(false),
            Instant::now() + TIMEOUT,
        )
        .unwrap();
        thread::sleep(Duration::from_millis(30));
        assert!(owner.try_recv().is_none());
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;
    use crate::local_ipc::owner_queue;
    #[test]
    fn stalled_client_does_not_block_shutdown() {
        let (sender, _owner) = owner_queue("session".into(), 1);
        let server = Server::start(sender).unwrap();
        let handle = unsafe {
            CreateFileW(
                wide(server.endpoint()).as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(handle, INVALID_HANDLE_VALUE);
        let client = unsafe { std::fs::File::from_raw_handle(handle) };
        // Send a partial frame and retain the handle, exercising cancellation of a stalled peer.
        let mode = PIPE_NOWAIT;
        assert_ne!(
            unsafe {
                SetNamedPipeHandleState(
                    client.as_raw_handle(),
                    &mode,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            0
        );
        transfer(
            &client,
            &mut [4, 0],
            true,
            &AtomicBool::new(false),
            Instant::now() + TIMEOUT,
        )
        .unwrap();
        thread::sleep(Duration::from_millis(30));
        let start = Instant::now();
        drop(server);
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn client_rejects_remote_and_unrelated_paths() {
        let r = AttachmentRequest {
            session_id: "s".into(),
            request_id: "r".into(),
            expected_generation: 0,
            method: "inspect".into(),
            params: Value::Null,
        };
        for path in [
            r"\\remote\pipe\processcraft-00000000-0000-0000-0000-000000000000",
            r"C:\Windows\test",
            r"\\.\pipe\other",
        ] {
            assert_eq!(
                call(path, "token", &r).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }
}
#[cfg(test)]
mod acl_tests {
    use super::*;
    #[test]
    fn pipe_has_protected_single_user_dacl() {
        let pipe =
            create_pipe(&format!(r"\\.\pipe\processcraft-{}", uuid::Uuid::new_v4())).unwrap();
        unsafe {
            let mut token = std::ptr::null_mut();
            assert_ne!(
                OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token),
                0
            );
            let token = std::fs::File::from_raw_handle(token);
            let mut size = 0;
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                std::ptr::null_mut(),
                0,
                &mut size,
            );
            let mut user = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
            assert_ne!(
                GetTokenInformation(
                    token.as_raw_handle(),
                    TokenUser,
                    user.as_mut_ptr().cast(),
                    size,
                    &mut size
                ),
                0
            );
            let sid = (*(user.as_ptr() as *const TOKEN_USER)).User.Sid;
            let mut needed = 0;
            GetKernelObjectSecurity(
                pipe.as_raw_handle(),
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                0,
                &mut needed,
            );
            assert_ne!(needed, 0);
            let mut descriptor =
                vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
            let desc = descriptor.as_mut_ptr().cast();
            assert_ne!(
                GetKernelObjectSecurity(
                    pipe.as_raw_handle(),
                    OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                    desc,
                    needed,
                    &mut needed
                ),
                0
            );
            let mut owner = std::ptr::null_mut();
            let mut defaulted = 0;
            let mut control = 0;
            let mut revision = 0;
            let mut present = 0;
            let mut acl = std::ptr::null_mut();
            assert_ne!(
                GetSecurityDescriptorOwner(desc, &mut owner, &mut defaulted),
                0
            );
            assert_ne!(EqualSid(owner, sid), 0);
            assert_ne!(
                GetSecurityDescriptorControl(desc, &mut control, &mut revision),
                0
            );
            assert_ne!(control & SE_DACL_PROTECTED, 0);
            assert_ne!(
                GetSecurityDescriptorDacl(desc, &mut present, &mut acl, &mut defaulted),
                0
            );
            assert_ne!(present, 0);
            assert!(!acl.is_null());
            assert_eq!((*acl).AceCount, 1);
            let mut raw = std::ptr::null_mut();
            assert_ne!(GetAce(acl, 0, &mut raw), 0);
            let ace = &*(raw as *const ACCESS_ALLOWED_ACE);
            assert_eq!(ace.Header.AceType, 0);
            assert_ne!(
                EqualSid((&ace.SidStart as *const u32).cast_mut().cast(), sid),
                0
            );
        }
    }
}
