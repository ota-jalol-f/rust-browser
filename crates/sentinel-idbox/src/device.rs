//! The real hardware path for `ReadDocument` (x64 plugin side), linked by the `device`
//! feature: the serial MRZ scanner over a COM port, the Elyctis port lookup over the
//! registry, the PC/SC antenna select, and the x86 chip helper over a CurrentUser named
//! pipe. All of this needs the physical ID BOX and the x86 Elyctis chain, so it is
//! verified on a device, not in CI; the logic it drives ([`crate::session`], the core
//! crate) is covered by the fake-backed tests.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Stream as LocalStream, ToNsName};
use sentinel_idbox_core::protocol::{self, Command as Cmd, HelperMessage, HelperRequest, MessageType};

use crate::serial::{ElyPortLocator, MrzScanner, RegistryView, Scanner, SerialPort};
use crate::session::{CardDetector, CardState, ChipEvents, ChipReader, ChipReaderStart, Devices};

/// Sub-folder of the plugin folder holding the x86 helper + the Elyctis chain.
const HELPER_SUBDIR: &str = "chip";
const HELPER_EXE: &str = "sentinel-idbox-helper.exe";
const PIPE_PREFIX: &str = protocol::PIPE_PREFIX;

/// The production [`Devices`]: real serial, registry, PC/SC and the x86 helper.
pub struct RealDevices;

impl RealDevices {
    /// Create the real device factory.
    pub fn new() -> Self {
        RealDevices
    }
}

impl Devices for RealDevices {
    fn create_scanner(&self) -> Box<dyn Scanner> {
        Box::new(MrzScanner::new(
            Box::new(|name| Box::new(RealSerialPort::new(name)) as Box<dyn SerialPort>),
            Box::new(|ms| std::thread::sleep(Duration::from_millis(ms))),
        ))
    }

    fn find_port(&self) -> Result<String, String> {
        let registry = WindowsRegistryView;
        let names = available_port_names();
        Ok(ElyPortLocator::new(&registry, names).find_port())
    }

    fn create_card_detector(&self) -> Box<dyn CardDetector> {
        Box::new(RealCardDetector { ctx: None })
    }

    fn create_chip_reader(&self) -> Box<dyn ChipReader> {
        Box::new(HelperChipReader::new())
    }
}

impl Default for RealDevices {
    fn default() -> Self {
        Self::new()
    }
}

// ---- serial ------------------------------------------------------------------------

struct RealSerialPort {
    name: String,
    port: Option<Box<dyn serialport::SerialPort>>,
}

impl RealSerialPort {
    fn new(name: &str) -> Self {
        RealSerialPort { name: name.to_string(), port: None }
    }
}

impl SerialPort for RealSerialPort {
    fn open(&mut self) -> Result<(), String> {
        let port = serialport::new(&self.name, 9600)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .timeout(Duration::from_millis(50))
            .open()
            .map_err(|e| e.to_string())?;
        self.port = Some(port);
        Ok(())
    }
    fn close(&mut self) {
        self.port = None;
    }
    fn is_open(&self) -> bool {
        self.port.is_some()
    }
    fn set_dtr(&mut self, on: bool) {
        if let Some(p) = &mut self.port {
            let _ = p.write_data_terminal_ready(on);
        }
    }
    fn bytes_to_read(&self) -> usize {
        self.port.as_ref().and_then(|p| p.bytes_to_read().ok()).unwrap_or(0) as usize
    }
    fn discard_in(&mut self) {
        if let Some(p) = &mut self.port {
            let _ = p.clear(serialport::ClearBuffer::Input);
        }
    }
    fn discard_out(&mut self) {
        if let Some(p) = &mut self.port {
            let _ = p.clear(serialport::ClearBuffer::Output);
        }
    }
    fn read(&mut self, buf: &mut [u8], count: usize) -> usize {
        let count = count.min(buf.len());
        let mut total = 0;
        if let Some(p) = &mut self.port {
            while total < count {
                match p.read(&mut buf[total..count]) {
                    Ok(0) => break,
                    Ok(n) => total += n,
                    Err(_) => break,
                }
            }
        }
        total
    }
    fn write(&mut self, data: &[u8]) {
        if let Some(p) = &mut self.port {
            let _ = p.write_all(data);
        }
    }
}

/// A 128-bit unpredictable hex token for the pipe name, seeded from the OS CSPRNG via
/// `RandomState` (its SipHash keys are drawn from the platform secure RNG), so no extra
/// dependency is needed.
fn random_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut token = String::with_capacity(32);
    for _ in 0..2 {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(std::process::id() as u64);
        token.push_str(&format!("{:016x}", hasher.finish()));
    }
    token
}

fn available_port_names() -> Vec<String> {
    serialport::available_ports()
        .map(|ports| ports.into_iter().map(|p| p.port_name).collect())
        .unwrap_or_default()
}

// ---- registry ----------------------------------------------------------------------

struct WindowsRegistryView;

impl RegistryView for WindowsRegistryView {
    fn sub_key_names(&self, path: &str) -> Option<Vec<String>> {
        let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
        hklm.open_subkey(path)
            .ok()
            .map(|key| key.enum_keys().filter_map(Result::ok).collect())
    }
    fn value(&self, path: &str, name: &str) -> Option<String> {
        let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
        hklm.open_subkey(path).ok()?.get_value::<String, _>(name).ok()
    }
}

// ---- PC/SC card detector -----------------------------------------------------------

struct RealCardDetector {
    ctx: Option<pcsc::Context>,
}

impl RealCardDetector {
    fn context(&mut self) -> Result<&pcsc::Context, String> {
        if self.ctx.is_none() {
            self.ctx = Some(pcsc::Context::establish(pcsc::Scope::System).map_err(|e| e.to_string())?);
        }
        Ok(self.ctx.as_ref().unwrap())
    }
}

impl CardDetector for RealCardDetector {
    fn open(&mut self) -> Result<(), String> {
        self.context().map(|_| ())
    }
    fn list_readers(&mut self) -> Result<Vec<String>, String> {
        let ctx = self.context()?;
        let mut buf = [0u8; 2048];
        let names = ctx.list_readers(&mut buf).map_err(|e| e.to_string())?;
        Ok(names.map(|c| c.to_string_lossy().into_owned()).collect())
    }
    fn get_state(&mut self, reader: &str, timeout_ms: i32) -> Result<CardState, String> {
        let name = std::ffi::CString::new(reader).map_err(|e| e.to_string())?;
        let ctx = self.context()?;
        let mut states = [pcsc::ReaderState::new(name.as_c_str(), pcsc::State::UNAWARE)];
        match ctx.get_status_change(Duration::from_millis(timeout_ms as u64), &mut states) {
            Ok(()) => {
                if states[0].event_state().contains(pcsc::State::PRESENT) {
                    Ok(CardState::Present)
                } else {
                    Ok(CardState::Absent)
                }
            }
            Err(pcsc::Error::Timeout) => Ok(CardState::Timeout),
            Err(e) => Err(e.to_string()),
        }
    }
}

// ---- chip helper over a named pipe -------------------------------------------------

/// A line-buffered duplex named-pipe connection.
struct PipeConn {
    stream: LocalStream,
    buf: Vec<u8>,
}

impl PipeConn {
    fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        self.stream.write_all(line.as_bytes())?;
        self.stream.write_all(b"\n")?;
        self.stream.flush()
    }

    /// The next `\n`-terminated line (CR stripped), or `None` at EOF.
    fn read_line(&mut self) -> std::io::Result<Option<String>> {
        loop {
            if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=pos).collect();
                let text = String::from_utf8_lossy(&line[..line.len() - 1]);
                return Ok(Some(text.trim_end_matches('\r').to_string()));
            }
            let mut chunk = [0u8; 512];
            let n = self.stream.read(&mut chunk)?;
            if n == 0 {
                if self.buf.is_empty() {
                    return Ok(None);
                }
                let text = String::from_utf8_lossy(&self.buf).to_string();
                self.buf.clear();
                return Ok(Some(text));
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }
}

/// Runs the x86 chip helper and speaks the protocol over a CurrentUser named pipe. The
/// read is synchronous: send `readChip`, forward every `status`/`alert` line, return the
/// `result`.
struct HelperChipReader {
    child: Option<Child>,
    conn: Option<PipeConn>,
    available: bool,
    unavailable_reason: Option<String>,
    next_id: i32,
}

impl HelperChipReader {
    fn new() -> Self {
        HelperChipReader {
            child: None,
            conn: None,
            available: false,
            unavailable_reason: None,
            next_id: 0,
        }
    }

    fn helper_path() -> std::path::PathBuf {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        dir.join(HELPER_SUBDIR).join(HELPER_EXE)
    }

    fn try_start(&mut self) -> Result<ChipReaderStart, String> {
        let exe = Self::helper_path();
        if !exe.exists() {
            return Err(format!("chip helper not found: {}", exe.display()));
        }

        // Unpredictable name so a same-user process cannot guess it to squat the name or
        // connect to our listener (the chip read carries passport data). `create_sync`
        // fails if the name already exists, which rejects a squatter that somehow won the
        // race. Windows' default pipe DACL already restricts access to this user +
        // admins (not world). Residual hardening that needs raw winapi (this crate is
        // `#![forbid(unsafe_code)]`, so it is a tracked follow-up): an explicit
        // current-user-SID DACL, FILE_FLAG_FIRST_PIPE_INSTANCE, and verifying the client
        // PID with GetNamedPipeClientProcessId after accept.
        let pipe_name = format!("{PIPE_PREFIX}{}", random_token());
        let ns = pipe_name.clone().to_ns_name::<GenericNamespaced>().map_err(|e| e.to_string())?;
        let listener = ListenerOptions::new().name(ns).create_sync().map_err(|e| e.to_string())?;

        let child = Command::new(&exe)
            .arg("--pipe")
            .arg(&pipe_name)
            .arg("--parent")
            .arg(std::process::id().to_string())
            .current_dir(exe.parent().unwrap_or_else(|| std::path::Path::new(".")))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        self.child = Some(child);

        // accept() blocks; the parent-watch + the OS guarantee the helper connects
        // promptly, and a dead helper closes the pipe which unblocks the read below.
        let stream = listener.accept().map_err(|e| e.to_string())?;
        let mut conn = PipeConn { stream, buf: Vec::new() };

        let hello_line = conn
            .read_line()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no hello from the chip helper".to_string())?;
        let hello: HelperMessage = protocol::deserialize(&hello_line).map_err(|e| e.to_string())?;
        if hello.type_ != MessageType::HELLO {
            return Err(format!("unexpected first line from the chip helper: {hello_line}"));
        }

        self.conn = Some(conn);
        self.available = hello.ok == Some(true);
        if !self.available {
            self.unavailable_reason = Some(format!("ElyMRTD could not be created: {}", hello.error.unwrap_or_default()));
        }
        Ok(ChipReaderStart {
            available: self.available,
            unavailable_reason: self.unavailable_reason.clone(),
            reader_init_error: hello.reader_init_error,
        })
    }

    fn send(&mut self, request: &HelperRequest) -> bool {
        if let Some(conn) = &mut self.conn {
            conn.write_line(&protocol::serialize(request)).is_ok()
        } else {
            false
        }
    }
}

impl ChipReader for HelperChipReader {
    fn start(&mut self) -> ChipReaderStart {
        match self.try_start() {
            Ok(start) => start,
            Err(reason) => {
                self.available = false;
                self.unavailable_reason = Some(reason.clone());
                ChipReaderStart { available: false, unavailable_reason: Some(reason), reader_init_error: None }
            }
        }
    }

    fn close_comm(&mut self) {
        if !self.available {
            return;
        }
        self.next_id += 1;
        let id = self.next_id;
        if self.send(&HelperRequest { id, cmd: Some(Cmd::CLOSE_COMM.to_string()), ..Default::default() }) {
            // Drain its single result line so it does not desync the next read.
            if let Some(conn) = &mut self.conn {
                let _ = conn.read_line();
            }
        }
    }

    fn read(&mut self, reader: &str, mrz_info: &str, events: &mut dyn ChipEvents) -> HelperMessage {
        if !self.available {
            // FormReader's connect() == -1 path: the completion then uses the MRZ.
            events.status(sentinel_idbox_core::texts::CONNECTING, false);
            events.status(sentinel_idbox_core::texts::APPLET_ERROR, false);
            return HelperMessage::result(self.next_id);
        }

        self.next_id += 1;
        let id = self.next_id;
        let request = HelperRequest {
            id,
            cmd: Some(Cmd::READ_CHIP.to_string()),
            reader: Some(reader.to_string()),
            mrz_info: Some(mrz_info.to_string()),
        };
        if !self.send(&request) {
            let mut r = HelperMessage::result(id);
            r.fault = Some("chip helper write failed".to_string());
            return r;
        }

        loop {
            let line = match self.conn.as_mut().and_then(|c| c.read_line().ok().flatten()) {
                Some(l) => l,
                None => {
                    self.available = false;
                    let mut r = HelperMessage::result(id);
                    r.fault = Some("the chip helper exited".to_string());
                    return r;
                }
            };
            let msg: HelperMessage = match protocol::deserialize(&line) {
                Ok(m) => m,
                Err(_) => continue,
            };
            match msg.type_.as_str() {
                t if t == MessageType::STATUS => events.status(msg.text.as_deref().unwrap_or(""), msg.is_error == Some(true)),
                t if t == MessageType::ALERT => events.alert(msg.caption.as_deref().unwrap_or(""), msg.text.as_deref().unwrap_or("")),
                t if t == MessageType::RESULT => return msg,
                _ => {}
            }
        }
    }

    fn abort(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
        }
    }
}

impl Drop for HelperChipReader {
    fn drop(&mut self) {
        self.next_id += 1;
        let id = self.next_id;
        let _ = self.send(&HelperRequest { id, cmd: Some(Cmd::QUIT.to_string()), ..Default::default() });
        if let Some(child) = &mut self.child {
            let _ = child.wait();
        }
    }
}
