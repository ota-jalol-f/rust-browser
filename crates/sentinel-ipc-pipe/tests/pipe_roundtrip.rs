//! End-to-end: spawn the helper process, connect over a real Windows named pipe,
//! and round-trip requests through serve/Client. Needs no SDK — it runs the
//! host-arch helper binary.

use std::path::Path;

use sentinel_ipc::{Request, Response};

#[test]
fn spawn_helper_and_round_trip() {
    let exe = Path::new(env!("CARGO_BIN_EXE_sentinel-ipc-helper"));
    let mut helper = sentinel_ipc_pipe::spawn_helper(exe).expect("spawn helper");

    let r1 = helper.client().call(Request::UsbList).expect("usb list");
    assert!(matches!(r1, Response::UsbDevices(_)), "got {r1:?}");

    let r2 = helper.client().call(Request::SerialList).expect("serial list");
    assert!(matches!(r2, Response::SerialPorts(_)), "got {r2:?}");

    let r3 = helper.client().call(Request::DllUnload { handle: 7 }).expect("ack");
    assert!(matches!(r3, Response::Ack), "got {r3:?}");

    helper.shutdown().expect("shutdown");
}
