//! Windows named-pipe binding for the [`sentinel_ipc`] transport, plus the host
//! side that spawns an out-of-process helper and talks to it.
//!
//! The host creates the pipe and spawns the helper exe with `--pipe <name>`; the
//! helper connects and runs [`sentinel_ipc::serve`] with its executor. This is how
//! an **x86-only vendor SDK** (e.g. the Elyctis ID-card chip reader) is isolated:
//! the x64 browser can't load an x86 DLL in-process, so a small x86 helper hosts
//! it and a crash there never takes down the browser. A byte duplex pipe is used
//! (not stdio) because vendor SDKs often write to stdout, which would corrupt a
//! stdout-based protocol.
//!
//! Windows-only. The `unsafe` Win32 FFI is confined here.

use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::process::{Child, Command};
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};

use sentinel_core::{Error, Result};
use sentinel_ipc::Client;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_BROKEN_PIPE, ERROR_PIPE_CONNECTED, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FlushFileBuffers, ReadFile, WriteFile, OPEN_EXISTING,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId,
};

// Win32 flag constants (hard-coded to avoid feature-path churn across windows-sys
// versions; values are stable ABI).
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const PIPE_ACCESS_DUPLEX: u32 = 0x0000_0003;
const PIPE_TYPE_BYTE: u32 = 0x0000_0000;
const PIPE_READMODE_BYTE: u32 = 0x0000_0000;
const PIPE_WAIT: u32 = 0x0000_0000;
// Fail if the name already exists (anti-squat), and refuse remote clients.
const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;
const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x0000_0008;
const PIPE_BUFFER: u32 = 64 * 1024;

/// Encode a Rust string as a NUL-terminated UTF-16 buffer for the `*W` Win32 APIs.
fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(once(0)).collect()
}

fn last_error() -> u32 {
    unsafe { GetLastError() }
}

/// A connected duplex named-pipe endpoint implementing [`Read`] + [`Write`].
pub struct PipeStream {
    handle: HANDLE,
}

// The handle is exclusively owned by this struct; moving it between threads is safe.
unsafe impl Send for PipeStream {}

impl PipeStream {
    /// PID of the process connected at the other end of the pipe (the client).
    ///
    /// # Errors
    /// [`Error`] if the query fails.
    pub fn client_process_id(&self) -> Result<u32> {
        let mut pid: u32 = 0;
        let ok = unsafe { GetNamedPipeClientProcessId(self.handle, &mut pid) };
        if ok == 0 {
            return Err(Error::Engine(format!(
                "GetNamedPipeClientProcessId failed: {}",
                last_error()
            )));
        }
        Ok(pid)
    }
}

impl Read for PipeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read: u32 = 0;
        let ok = unsafe {
            ReadFile(self.handle, buf.as_mut_ptr(), buf.len() as u32, &mut read, ptr::null_mut())
        };
        if ok == 0 {
            let err = last_error();
            // Peer closed its end: treat as clean EOF.
            if err == ERROR_BROKEN_PIPE {
                return Ok(0);
            }
            return Err(io::Error::from_raw_os_error(err as i32));
        }
        Ok(read as usize)
    }
}

impl Write for PipeStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut written: u32 = 0;
        let ok = unsafe {
            WriteFile(self.handle, buf.as_ptr(), buf.len() as u32, &mut written, ptr::null_mut())
        };
        if ok == 0 {
            return Err(io::Error::from_raw_os_error(last_error() as i32));
        }
        Ok(written as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        unsafe {
            FlushFileBuffers(self.handle);
        }
        Ok(())
    }
}

impl Drop for PipeStream {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

/// Connect to an existing named pipe `\\.\pipe\<pipe_name>` (the helper/client side).
/// Retries briefly while the host is still creating the pipe.
///
/// # Errors
/// [`Error`] if the pipe cannot be opened within the retry window.
pub fn connect(pipe_name: &str) -> Result<PipeStream> {
    let wname = wide(&format!(r"\\.\pipe\{pipe_name}"));
    for _ in 0..100 {
        let handle = unsafe {
            CreateFileW(
                wname.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE && !handle.is_null() {
            return Ok(PipeStream { handle });
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err(Error::Engine(format!("connect to pipe {pipe_name} timed out: {}", last_error())))
}

/// A created named pipe awaiting a single client connection (the host/server side).
pub struct PipeListener {
    handle: HANDLE,
}

impl PipeListener {
    /// Create the pipe `\\.\pipe\<pipe_name>` (one instance, byte duplex, blocking).
    ///
    /// # Errors
    /// [`Error`] if the pipe cannot be created.
    pub fn create(pipe_name: &str) -> Result<Self> {
        let wname = wide(&format!(r"\\.\pipe\{pipe_name}"));
        let handle = unsafe {
            CreateNamedPipeW(
                wname.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                PIPE_BUFFER,
                PIPE_BUFFER,
                0,
                ptr::null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            return Err(Error::Engine(format!("CreateNamedPipe failed: {}", last_error())));
        }
        Ok(Self { handle })
    }

    /// Block until a client connects, consuming the listener and returning the
    /// connected duplex stream.
    ///
    /// # Errors
    /// [`Error`] if the connection fails.
    pub fn accept(self) -> Result<PipeStream> {
        let ok = unsafe { ConnectNamedPipe(self.handle, ptr::null_mut()) };
        if ok == 0 {
            let err = last_error();
            // The client may have connected between create and accept.
            if err != ERROR_PIPE_CONNECTED {
                return Err(Error::Engine(format!("ConnectNamedPipe failed: {err}")));
            }
        }
        let handle = self.handle;
        std::mem::forget(self); // hand the handle to the stream; don't close it here
        Ok(PipeStream { handle })
    }
}

impl Drop for PipeListener {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

/// A running helper process and the [`Client`] talking to it over the pipe.
pub struct Helper {
    child: Child,
    client: Client<PipeStream>,
}

impl Helper {
    /// The IPC client for sending requests to the helper.
    pub fn client(&mut self) -> &mut Client<PipeStream> {
        &mut self.client
    }

    /// Close the pipe (ending the helper's serve loop) and wait for it to exit.
    pub fn shutdown(self) -> Result<()> {
        let Helper { mut child, client } = self;
        drop(client); // closes the pipe -> helper's serve() returns -> helper exits
        child.wait().map_err(|e| Error::Engine(format!("helper wait failed: {e}")))?;
        Ok(())
    }
}

/// Unique-per-call pipe name component.
fn unique() -> u64 {
    static N: AtomicU64 = AtomicU64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

/// Nanoseconds since the epoch, for pipe-name entropy.
fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Spawn `helper_exe` as an out-of-process host, connected over a fresh named pipe.
/// Returns once the helper has connected; use [`Helper::client`] to call it.
///
/// # Errors
/// [`Error`] if the pipe or process cannot be created, or the helper never connects.
pub fn spawn_helper(helper_exe: &Path) -> Result<Helper> {
    let name = format!("sentinel-ipc-{}-{:x}-{:x}", std::process::id(), nanos(), unique());
    let listener = PipeListener::create(&name)?;
    let mut child = Command::new(helper_exe)
        .arg("--pipe")
        .arg(&name)
        .spawn()
        .map_err(|e| Error::Engine(format!("spawn helper {}: {e}", helper_exe.display())))?;
    let stream = listener.accept()?;
    // Anti-impersonation: accept only our spawned helper as the client.
    let client_pid = stream.client_process_id()?;
    if client_pid != child.id() {
        let _ = child.kill();
        return Err(Error::Engine(format!(
            "unexpected pipe client pid {client_pid} (expected {})",
            child.id()
        )));
    }
    Ok(Helper { child, client: Client::new(stream) })
}
