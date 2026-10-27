//! The ID BOX serial MRZ scanner and the Elyctis COM-port lookup — the plugin-side
//! (x64) ports of pv-winform `Scanner.cs` and `FormReader`'s registry port walk
//! (reference `Asbt.IdBox.Plugin.Serial`). The device I/O sits behind the [`SerialPort`]
//! and [`RegistryView`] seams; the framing and the walk are pure and host-testable.
//!
//! Unlike C#'s event-driven `DataReceived`, the Rust read is synchronous: [`MrzScanner::inquire`]
//! then [`MrzScanner::read_mrz`] polls the port for one frame — a better fit for the
//! single blocking `ReadDocument()` plugin call (no modal, no UI thread).

use std::collections::HashMap;

/// Makes a not-yet-open serial port for a name.
pub type PortFactory = Box<dyn Fn(&str) -> Box<dyn SerialPort>>;
/// `Thread.Sleep` (milliseconds); a no-op in tests.
pub type Sleeper = Box<dyn Fn(u64)>;

/// The `System.IO.Ports.SerialPort` members `Scanner` uses; a fake in tests, a real COM
/// port behind the `device` feature.
pub trait SerialPort {
    /// Open the port (9600 8N1). `Err` on failure (C# swallows it → `Connect` returns false).
    fn open(&mut self) -> Result<(), String>;
    /// Close the port.
    fn close(&mut self);
    /// Whether the port is open.
    fn is_open(&self) -> bool;
    /// Set DTR.
    fn set_dtr(&mut self, on: bool);
    /// Bytes available to read.
    fn bytes_to_read(&self) -> usize;
    /// Discard the input buffer.
    fn discard_in(&mut self);
    /// Discard the output buffer.
    fn discard_out(&mut self);
    /// Read up to `count` bytes into `buf`; returns the number actually read.
    fn read(&mut self, buf: &mut [u8], count: usize) -> usize;
    /// Write `data`.
    fn write(&mut self, data: &[u8]);
}

/// The MRZ scanner behind a seam, so the orchestration can be driven by a fake. The
/// real one is [`MrzScanner`].
pub trait Scanner {
    /// Open the port with DTR on and purge; `false` on any failure.
    fn connect(&mut self, port_name: &str) -> bool;
    /// Close the port.
    fn disconnect(&mut self) -> bool;
    /// Whether the port is open.
    fn is_connected(&self) -> bool;
    /// Turn continuous reading on/off.
    fn set_enable_reading(&mut self, value: bool);
    /// Ask the ID BOX to read the document on the glass.
    fn inquire(&mut self);
    /// Read one MRZ frame (up to `timeout_ms`), or `""`.
    fn read_mrz(&mut self, timeout_ms: i32) -> String;
}

/// The ID BOX MRZ scanner (`Scanner.cs`): framing `[cmd][lenHi][lenLo][payload]`, the
/// `'I'` frame carrying the MRZ as ASCII; every read error is swallowed to `""`.
pub struct MrzScanner {
    com: Option<Box<dyn SerialPort>>,
    create: PortFactory,
    sleep: Sleeper,
    enable_reading: bool,
}

impl MrzScanner {
    /// `create` makes a not-yet-open port for a name; `sleep` is `Thread.Sleep`
    /// (a no-op in tests).
    pub fn new(create: PortFactory, sleep: Sleeper) -> Self {
        MrzScanner { com: None, create, sleep, enable_reading: false }
    }

    /// `Connect`: open the port with DTR on and purge; `false` on any failure.
    pub fn connect(&mut self, port_name: &str) -> bool {
        let mut com = (self.create)(port_name);
        match com.open() {
            Ok(()) => {
                com.set_dtr(true);
                com.discard_in();
                com.discard_out();
                self.com = Some(com);
                true
            }
            Err(_) => {
                // C# keeps the field set even on failure; IsConnected then sees it closed.
                self.com = Some(com);
                false
            }
        }
    }

    /// `Disconnect`: close the port (swallows failures).
    pub fn disconnect(&mut self) -> bool {
        if let Some(com) = self.com.as_mut() {
            com.close();
        }
        true
    }

    /// `IsConnected`.
    pub fn is_connected(&self) -> bool {
        self.com.as_ref().map(|c| c.is_open()).unwrap_or(false)
    }

    /// `EnableReading` setter: write `43 00 01 00/01` (continuous reading off/on).
    pub fn set_enable_reading(&mut self, value: bool) {
        self.set_continuous(value);
        self.enable_reading = value;
    }

    /// Whether continuous reading is on.
    pub fn enable_reading(&self) -> bool {
        self.enable_reading
    }

    /// `Inquire`: write `49 00 00` (read the MRZ of the document on the glass).
    pub fn inquire(&mut self) {
        if let Some(com) = self.com.as_mut() {
            com.discard_in();
            com.write(&[0x49, 0, 0]);
        }
    }

    /// `ReadMRZ(timeout)`: poll one frame; the MRZ with `\r` turned into `\r\n`, or `""`.
    pub fn read_mrz(&mut self, timeout_ms: i32) -> String {
        let mut com = match self.com.take() {
            Some(c) => c,
            None => return String::new(),
        };
        let result = read_frame(com.as_mut(), self.sleep.as_ref(), self.enable_reading, timeout_ms);
        self.com = Some(com);
        result
    }

    fn set_continuous(&mut self, state: bool) {
        if let Some(com) = self.com.as_mut() {
            com.discard_in();
            com.write(&[0x43, 0, 1, if state { 1 } else { 0 }]);
        }
    }
}

impl Scanner for MrzScanner {
    fn connect(&mut self, port_name: &str) -> bool {
        MrzScanner::connect(self, port_name)
    }
    fn disconnect(&mut self) -> bool {
        MrzScanner::disconnect(self)
    }
    fn is_connected(&self) -> bool {
        MrzScanner::is_connected(self)
    }
    fn set_enable_reading(&mut self, value: bool) {
        MrzScanner::set_enable_reading(self, value)
    }
    fn inquire(&mut self) {
        MrzScanner::inquire(self)
    }
    fn read_mrz(&mut self, timeout_ms: i32) -> String {
        MrzScanner::read_mrz(self, timeout_ms)
    }
}

/// `Scanner.ReadMRZ`, verbatim: wait for a frame, read `[cmd][lenHi][lenLo]`, then the
/// payload; an `'I'` frame yields the MRZ. Any framing error is swallowed to `""`.
fn read_frame(com: &mut dyn SerialPort, sleep: &dyn Fn(u64), enable_reading: bool, timeout: i32) -> String {
    let mut mrz = String::new();
    // `None` is C#'s swallowed read error: the MRZ stays whatever was gathered ("").
    let _ = frame_inner(com, sleep, enable_reading, timeout, &mut mrz);
    mrz.replace('\r', "\r\n")
}

fn frame_inner(
    com: &mut dyn SerialPort,
    sleep: &dyn Fn(u64),
    enable_reading: bool,
    timeout: i32,
    mrz: &mut String,
) -> Option<()> {
    let mut response = [0u8; 256];
    let mut i = 0;

    // Wait for a response.
    while i < timeout {
        sleep(10);
        if com.bytes_to_read() > 2 {
            break;
        }
        i += 10;
    }

    if com.bytes_to_read() > 2 {
        if com.read(&mut response, 3) != 3 {
            return None;
        }
        let cmd = response[0];
        let length = response[1] as usize * 256 + response[2] as usize;

        if length > 0 {
            if length > response.len() {
                return None; // "Device communication error."
            }
            while i < timeout {
                sleep(10);
                if com.bytes_to_read() >= length {
                    break;
                }
                i += 10;
            }
            if com.read(&mut response, length) != length {
                return None;
            }
        }

        if cmd == b'I' {
            mrz.push_str(&String::from_utf8_lossy(&response[..length]));
            if enable_reading {
                com.discard_in();
                com.write(&[0x43, 0, 1, 1]);
            }
        }
        // 'V' (version) / 'W' (NNA version) are ignored, as in C#.
    }

    Some(())
}

/// Read-only HKLM access, as `FormReader`'s port lookup uses it.
pub trait RegistryView {
    /// `OpenSubKey(path).GetSubKeyNames()`; `None` when the key is missing (C#'s
    /// `NullReferenceException`, swallowed by the walk).
    fn sub_key_names(&self, path: &str) -> Option<Vec<String>>;
    /// `Registry.GetValue(path, name, null)`.
    fn value(&self, path: &str, name: &str) -> Option<String>;
}

/// The Elyctis COM-port lookup: walk `HKLM\SYSTEM\CurrentControlSet\Enum` for devices
/// whose `Device Parameters\PortName` is one of the system's serial ports, map it to its
/// `FriendlyName`, and take the **last** port whose friendly name contains "ely".
pub struct ElyPortLocator<'a> {
    registry: &'a dyn RegistryView,
    port_names: Vec<String>,
}

/// The registry root the walk starts from.
pub const ENUM_ROOT: &str = "SYSTEM\\CurrentControlSet\\Enum";

impl<'a> ElyPortLocator<'a> {
    /// A locator over `registry`, considering only `port_names` (`SerialPort.GetPortNames()`).
    pub fn new(registry: &'a dyn RegistryView, port_names: Vec<String>) -> Self {
        ElyPortLocator { registry, port_names }
    }

    /// `set_port()`: the Elyctis port, or `""` when none.
    pub fn find_port(&self) -> String {
        let mut port = String::new();
        for (key, value) in self.build_port_name_hash() {
            if value.to_lowercase().contains("ely") {
                port = key;
            }
        }
        port
    }

    /// `BuildPortNameHash`: port name → friendly name, in discovery order.
    pub fn build_port_name_hash(&self) -> Vec<(String, String)> {
        let mut map: HashMap<String, String> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        self.mine(ENUM_ROOT, &mut map, &mut order);
        order.into_iter().map(|k| (k.clone(), map[&k].clone())).collect()
    }

    fn mine(&self, path: &str, map: &mut HashMap<String, String>, order: &mut Vec<String>) {
        if map.len() >= self.port_names.len() {
            return;
        }
        let subkeys = match self.registry.sub_key_names(path) {
            Some(s) => s,
            None => return, // C#: "Error accessing key .. Skipping.."
        };

        if subkeys.iter().any(|s| s == "Device Parameters") && path != ENUM_ROOT {
            let port = match self.registry.value(&format!("{path}\\Device Parameters"), "PortName") {
                Some(p) if self.port_names.contains(&p) => p,
                _ => return,
            };
            let mut friendly = self.registry.value(path, "FriendlyName").unwrap_or_else(|| "N/A".to_string());
            if !friendly.contains(&port) {
                friendly = format!("{friendly} ({port})");
            }
            if !map.contains_key(&port) {
                order.push(port.clone());
            }
            map.insert(port, friendly);
        } else {
            for sub in subkeys {
                self.mine(&format!("{path}\\{sub}"), map, order);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    type Bytes = Rc<RefCell<VecDeque<u8>>>;
    type Log = Rc<RefCell<Vec<Vec<u8>>>>;
    type Calls = Rc<RefCell<Vec<String>>>;

    // ---- a byte-queue COM port (reference FakeSerialPort). The incoming queue is
    // shared so the test can feed it *after* connect() has purged the port. ----
    struct FakeSerialPort {
        incoming: Bytes,
        writes: Log,
        calls: Calls,
        throw_on_open: bool,
        max_read_per_call: usize,
        open: bool,
    }

    impl SerialPort for FakeSerialPort {
        fn open(&mut self) -> Result<(), String> {
            self.calls.borrow_mut().push("Open".into());
            if self.throw_on_open {
                return Err("Access to the port is denied.".into());
            }
            self.open = true;
            Ok(())
        }
        fn close(&mut self) {
            self.calls.borrow_mut().push("Close".into());
            self.open = false;
        }
        fn is_open(&self) -> bool {
            self.open
        }
        fn set_dtr(&mut self, _on: bool) {}
        fn bytes_to_read(&self) -> usize {
            self.incoming.borrow().len()
        }
        fn discard_in(&mut self) {
            self.calls.borrow_mut().push("DiscardIn".into());
            self.incoming.borrow_mut().clear();
        }
        fn discard_out(&mut self) {
            self.calls.borrow_mut().push("DiscardOut".into());
        }
        fn read(&mut self, buf: &mut [u8], count: usize) -> usize {
            let mut inc = self.incoming.borrow_mut();
            let n = count.min(inc.len()).min(self.max_read_per_call);
            for b in buf.iter_mut().take(n) {
                *b = inc.pop_front().unwrap();
            }
            n
        }
        fn write(&mut self, data: &[u8]) {
            self.writes.borrow_mut().push(data.to_vec());
        }
    }

    fn frame(cmd: u8, payload: &[u8]) -> Vec<u8> {
        let mut f = vec![cmd, (payload.len() >> 8) as u8, (payload.len() & 0xFF) as u8];
        f.extend_from_slice(payload);
        f
    }

    /// A scanner wired to one shared fake port the test can feed and inspect.
    struct Harness {
        scanner: MrzScanner,
        incoming: Bytes,
        writes: Log,
        calls: Calls,
    }

    fn connected(throw_on_open: bool, max_read: usize) -> Harness {
        let incoming: Bytes = Rc::new(RefCell::new(VecDeque::new()));
        let writes: Log = Rc::new(RefCell::new(Vec::new()));
        let calls: Calls = Rc::new(RefCell::new(Vec::new()));

        let (w, c, inc) = (writes.clone(), calls.clone(), incoming.clone());
        let create = Box::new(move |_name: &str| {
            Box::new(FakeSerialPort {
                incoming: inc.clone(),
                writes: w.clone(),
                calls: c.clone(),
                throw_on_open,
                max_read_per_call: max_read,
                open: false,
            }) as Box<dyn SerialPort>
        });
        let mut scanner = MrzScanner::new(create, Box::new(|_| {}));
        scanner.connect("COM3");
        Harness { scanner, incoming, writes, calls }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
    }

    /// Feeds a frame into the (already connected) port and returns one `read_mrz`.
    fn read_one(h: &mut Harness, bytes: &[u8]) -> String {
        h.incoming.borrow_mut().extend(bytes.iter().copied());
        h.scanner.read_mrz(1000)
    }

    #[test]
    fn connect_opens_with_dtr_and_purges() {
        let h = connected(false, usize::MAX);
        assert!(h.scanner.is_connected());
        assert_eq!(h.calls.borrow().join(","), "Open,DiscardIn,DiscardOut");
    }

    #[test]
    fn connect_failure_is_false_not_a_panic() {
        let writes: Log = Rc::new(RefCell::new(Vec::new()));
        let calls: Calls = Rc::new(RefCell::new(Vec::new()));
        let incoming: Bytes = Rc::new(RefCell::new(VecDeque::new()));
        let (w, c, inc) = (writes.clone(), calls.clone(), incoming.clone());
        let create = Box::new(move |_: &str| {
            Box::new(FakeSerialPort {
                incoming: inc.clone(),
                writes: w.clone(),
                calls: c.clone(),
                throw_on_open: true,
                max_read_per_call: usize::MAX,
                open: false,
            }) as Box<dyn SerialPort>
        });
        let mut scanner = MrzScanner::new(create, Box::new(|_| {}));
        assert!(!scanner.connect(""));
        assert!(!scanner.is_connected());
    }

    #[test]
    fn enable_reading_false_and_inquire_write_the_commands() {
        let mut h = connected(false, usize::MAX);
        h.writes.borrow_mut().clear();
        h.scanner.set_enable_reading(false);
        h.scanner.inquire();
        let writes: Vec<String> = h.writes.borrow().iter().map(|w| hex(w)).collect();
        assert_eq!(writes, vec!["43 00 01 00", "49 00 00"]);
    }

    #[test]
    fn i_frame_delivers_the_mrz_with_crlf() {
        let mut h = connected(false, usize::MAX);
        let mrz = read_one(&mut h, &frame(b'I', b"LINE1\rLINE2\r"));
        assert_eq!(mrz, "LINE1\r\nLINE2\r\n");
    }

    #[test]
    fn i_frame_with_continuous_reading_reenables_it() {
        let mut h = connected(false, usize::MAX);
        h.scanner.set_enable_reading(true);
        h.writes.borrow_mut().clear();
        let mrz = read_one(&mut h, &frame(b'I', b"X\r"));
        assert_eq!(mrz, "X\r\n");
        let writes: Vec<String> = h.writes.borrow().iter().map(|w| hex(w)).collect();
        assert_eq!(writes, vec!["43 00 01 01"]);
    }

    #[test]
    fn version_empty_and_short_header_are_silent() {
        let mut h = connected(false, usize::MAX);
        assert_eq!(read_one(&mut h, &frame(b'V', b"1.02")), "");
        assert_eq!(read_one(&mut h, &frame(b'I', b"")), "");
        assert_eq!(read_one(&mut h, &[0x49, 0x00]), "");
    }

    #[test]
    fn oversized_frame_is_swallowed() {
        let mut h = connected(false, usize::MAX);
        assert_eq!(read_one(&mut h, &[0x49, 0x01, 0x01, 0x41]), ""); // length 257 > 256
    }

    #[test]
    fn short_payload_read_is_swallowed() {
        let mut h = connected(false, 3);
        assert_eq!(read_one(&mut h, &frame(b'I', b"LINE1\r")), "");
    }

    #[test]
    fn disconnect_closes_the_port() {
        let mut h = connected(false, usize::MAX);
        assert!(h.scanner.disconnect());
        assert!(!h.scanner.is_connected());
        assert_eq!(h.calls.borrow().last().unwrap(), "Close");
    }

    // ---- ElyPortLocator (reference FakeRegistry) ----
    #[derive(Default)]
    struct FakeRegistry {
        keys: HashMap<String, Vec<String>>,
        values: HashMap<String, String>,
    }

    impl FakeRegistry {
        fn new() -> Self {
            let mut r = FakeRegistry::default();
            r.keys.insert(ENUM_ROOT.to_string(), Vec::new());
            r
        }
        fn device(mut self, relative: &str, port: &str, friendly: Option<&str>) -> Self {
            let mut path = ENUM_ROOT.to_string();
            for part in relative.split('\\') {
                let entry = self.keys.entry(path.clone()).or_default();
                if !entry.contains(&part.to_string()) {
                    entry.push(part.to_string());
                }
                path = format!("{path}\\{part}");
                self.keys.entry(path.clone()).or_default();
            }
            self.keys.get_mut(&path).unwrap().push("Device Parameters".to_string());
            self.keys.insert(format!("{path}\\Device Parameters"), Vec::new());
            self.values.insert(format!("{path}\\Device Parameters|PortName"), port.to_string());
            if let Some(f) = friendly {
                self.values.insert(format!("{path}|FriendlyName"), f.to_string());
            }
            self
        }
        fn broken(mut self, relative: &str) -> Self {
            self.keys.get_mut(ENUM_ROOT).unwrap().push(relative.to_string());
            self
        }
    }

    impl RegistryView for FakeRegistry {
        fn sub_key_names(&self, path: &str) -> Option<Vec<String>> {
            self.keys.get(path).cloned()
        }
        fn value(&self, path: &str, name: &str) -> Option<String> {
            self.values.get(&format!("{path}|{name}")).cloned()
        }
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn port_locator_takes_the_last_ely_port() {
        let registry = FakeRegistry::new()
            .device("FTDIBUS\\VID_0403\\0000", "COM4", Some("USB Serial Port (COM4)"))
            .device("USB\\VID_1234&PID_0001\\A1", "COM3", Some("Elyctis ID BOX (COM3)"))
            .device("USB\\VID_1234&PID_0001\\A2", "COM7", Some("ELYCTIS reader"));
        let locator = ElyPortLocator::new(&registry, names(&["COM3", "COM4", "COM7"]));

        assert_eq!(locator.find_port(), "COM7");
        let hash: Vec<String> = locator.build_port_name_hash().into_iter().map(|(k, v)| format!("{k}={v}")).collect();
        assert_eq!(
            hash,
            vec!["COM4=USB Serial Port (COM4)", "COM3=Elyctis ID BOX (COM3)", "COM7=ELYCTIS reader (COM7)"]
        );
    }

    #[test]
    fn port_locator_no_ely_is_empty_and_broken_keys_skipped() {
        let registry = FakeRegistry::new()
            .broken("ACPI")
            .device("FTDIBUS\\VID_0403\\0000", "COM4", Some("USB Serial Port (COM4)"))
            .device("USB\\VID_9999\\X", "COM9", None);
        let locator = ElyPortLocator::new(&registry, names(&["COM4", "COM9"]));

        assert_eq!(locator.find_port(), "");
        let com9 = locator
            .build_port_name_hash()
            .into_iter()
            .find(|(k, _)| k == "COM9")
            .map(|(_, v)| v)
            .unwrap();
        assert_eq!(com9, "N/A (COM9)");
    }

    #[test]
    fn port_locator_ignores_ports_not_in_get_port_names() {
        let registry = FakeRegistry::new().device("USB\\VID_1234\\A1", "COM3", Some("Elyctis ID BOX (COM3)"));
        let locator = ElyPortLocator::new(&registry, names(&["COM1"]));
        assert_eq!(locator.find_port(), "");
    }
}
