//! The per-user named pipe `\\.\pipe\RunBay-<user SID>`, restricted to the current user.

use std::ffi::c_void;
use std::fs::File;
use std::os::windows::io::FromRawHandle;

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE,
    HLOCAL, LocalFree,
};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_NONE,
    OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{HSTRING, PCWSTR, PWSTR};

/// The string SID of the current user, e.g. `S-1-5-21-…`.
pub fn current_user_sid() -> Result<String> {
    // SAFETY: standard token query; buffers are sized by the first call.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .context("OpenProcessToken failed")?;
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
        let mut buf = vec![0u8; len as usize];
        let result = GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr() as *mut c_void),
            len,
            &mut len,
        );
        let _ = CloseHandle(token);
        result.context("GetTokenInformation failed")?;
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut sid = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut sid).context("ConvertSidToStringSidW failed")?;
        let text = sid.to_string().unwrap_or_default();
        let _ = LocalFree(Some(HLOCAL(sid.0 as *mut c_void)));
        Ok(text)
    }
}

/// The pipe name for the current user. A custom data folder (development option
/// `--data-dir`) gets its own pipe, so a test instance never talks to the real one.
pub fn pipe_name(custom_data_dir: Option<&std::path::Path>) -> Result<String> {
    let base = format!(r"\\.\pipe\RunBay-{}", current_user_sid()?);
    Ok(match custom_data_dir {
        None => base,
        Some(dir) => {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            dir.to_string_lossy().to_lowercase().hash(&mut hasher);
            format!("{base}-{:016x}", hasher.finish())
        }
    })
}

/// Server side of the pipe. Each accepted connection is handed to `handler` on its own thread.
pub struct PipeServer;

impl PipeServer {
    /// Creates the first pipe instance. Fails when another process already owns the name,
    /// which means another RunBay instance is running.
    pub fn bind(name: &str) -> Result<PipeListener> {
        let sid = current_user_sid()?;
        let first = create_instance(name, &sid, true)?;
        Ok(PipeListener {
            name: name.to_string(),
            sid,
            next: Some(first),
        })
    }
}

/// A bound pipe that accepts connections.
pub struct PipeListener {
    name: String,
    sid: String,
    next: Option<HANDLE>,
}

// SAFETY: the pending handle is only used by the thread that owns the listener.
unsafe impl Send for PipeListener {}

impl PipeListener {
    /// Accepts connections forever on the calling thread.
    pub fn serve(mut self, handler: impl Fn(File) + Send + Sync + 'static) {
        let handler = std::sync::Arc::new(handler);
        loop {
            let handle = match self.next.take() {
                Some(h) => h,
                None => match create_instance(&self.name, &self.sid, false) {
                    Ok(h) => h,
                    Err(e) => {
                        log::error!("cannot create a pipe instance: {e:#}");
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        continue;
                    }
                },
            };
            // SAFETY: valid pipe handle; blocking connect.
            let connected = unsafe { ConnectNamedPipe(handle, None) };
            if let Err(e) = connected
                && e.code() != ERROR_PIPE_CONNECTED.to_hresult()
            {
                log::warn!("ConnectNamedPipe failed: {e}");
                // SAFETY: owned handle.
                unsafe {
                    let _ = CloseHandle(handle);
                }
                continue;
            }
            // SAFETY: the handle is owned and transferred to the File.
            let file = unsafe { File::from_raw_handle(handle.0) };
            let handler = handler.clone();
            std::thread::spawn(move || handler(file));
        }
    }
}

fn create_instance(name: &str, sid: &str, first: bool) -> Result<HANDLE> {
    // Only the current user (and nobody else, not even administrators) may connect.
    let sddl = HSTRING::from(format!("D:P(A;;GA;;;{sid})"));
    // SAFETY: the descriptor is freed after the pipe is created.
    unsafe {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
        .context("cannot build the pipe security descriptor")?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let mut open_mode = PIPE_ACCESS_DUPLEX;
        if first {
            open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        let handle = CreateNamedPipeW(
            &HSTRING::from(name),
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            64 * 1024,
            64 * 1024,
            0,
            Some(&attributes),
        );
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        if handle.is_invalid() {
            bail!(
                "cannot create the pipe {name}: {}",
                windows::core::Error::from_thread()
            );
        }
        Ok(handle)
    }
}

/// Connects to the pipe; `Ok(None)` when no instance is listening.
pub fn connect(name: &str, wait_ms: u32) -> Result<Option<File>> {
    let wide = HSTRING::from(name);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms as u64);
    loop {
        // SAFETY: plain CreateFileW on a pipe name.
        let result = unsafe {
            CreateFileW(
                &wide,
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        };
        match result {
            // SAFETY: the handle is owned by the File from here on.
            Ok(handle) => return Ok(Some(unsafe { File::from_raw_handle(handle.0) })),
            Err(e) if e.code() == ERROR_PIPE_BUSY.to_hresult() => {
                if std::time::Instant::now() >= deadline {
                    bail!("the RunBay pipe is busy");
                }
                // SAFETY: plain wait call.
                unsafe {
                    let _ = WaitNamedPipeW(&wide, 500);
                }
            }
            Err(_) => return Ok(None),
        }
    }
}
