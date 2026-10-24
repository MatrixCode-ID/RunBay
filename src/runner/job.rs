//! A Windows Job Object that owns the whole process tree of a run.

use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::process::Child;

use anyhow::{Context, Result};
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::System::IO::{CreateIoCompletionPort, GetQueuedCompletionStatus, OVERLAPPED};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_ASSOCIATE_COMPLETION_PORT, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectAssociateCompletionPortInformation, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows::Win32::System::Threading::INFINITE;

/// Message posted to the completion port when the last process of the job exits.
const JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO: u32 = 4;

/// A job with `KILL_ON_JOB_CLOSE` and a completion port that reports when it is empty.
pub struct Job {
    job: HANDLE,
    port: HANDLE,
}

// SAFETY: the handles are owned by this value and the Win32 calls used are thread-safe.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Job {
    /// Creates the job and its completion port.
    pub fn new() -> Result<Self> {
        // SAFETY: plain Win32 calls with valid arguments; handles are closed in Drop.
        unsafe {
            let job = CreateJobObjectW(None, None).context("CreateJobObjectW failed")?;
            let mut job = Job {
                job,
                port: HANDLE::default(),
            };
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job.job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const c_void,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .context("cannot set the job limits")?;
            job.port = CreateIoCompletionPort(INVALID_HANDLE_VALUE, None, 0, 1)
                .context("CreateIoCompletionPort failed")?;
            let assoc = JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
                CompletionKey: std::ptr::null_mut(),
                CompletionPort: job.port,
            };
            SetInformationJobObject(
                job.job,
                JobObjectAssociateCompletionPortInformation,
                &assoc as *const _ as *const c_void,
                size_of::<JOBOBJECT_ASSOCIATE_COMPLETION_PORT>() as u32,
            )
            .context("cannot associate the job completion port")?;
            Ok(job)
        }
    }

    /// Puts a freshly spawned process (and so its future children) into the job.
    pub fn assign(&self, child: &Child) -> Result<()> {
        // SAFETY: the child handle is valid while `child` lives.
        unsafe {
            AssignProcessToJobObject(self.job, HANDLE(child.as_raw_handle()))
                .context("cannot assign the process to the job")
        }
    }

    /// Kills every process in the job.
    pub fn terminate(&self) {
        // SAFETY: valid job handle.
        if let Err(e) = unsafe { TerminateJobObject(self.job, 1) } {
            log::warn!("TerminateJobObject failed: {e}");
        }
    }

    /// Blocks until the last process of the job has exited.
    pub fn wait_empty(&self) {
        loop {
            let mut message = 0u32;
            let mut key = 0usize;
            let mut overlapped: *mut OVERLAPPED = std::ptr::null_mut();
            // SAFETY: valid port handle and out pointers.
            let ok = unsafe {
                GetQueuedCompletionStatus(
                    self.port,
                    &mut message,
                    &mut key,
                    &mut overlapped,
                    INFINITE,
                )
            };
            if ok.is_err() {
                log::warn!("GetQueuedCompletionStatus failed: {:?}", ok);
                return;
            }
            if message == JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO {
                return;
            }
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: handles are owned by self.
        unsafe {
            if !self.port.is_invalid() {
                let _ = CloseHandle(self.port);
            }
            let _ = CloseHandle(self.job);
        }
    }
}
