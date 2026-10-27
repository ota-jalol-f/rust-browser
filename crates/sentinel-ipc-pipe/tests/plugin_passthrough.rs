//! End-to-end: the `Request::Plugin` passthrough over a real Windows named pipe.
//! The host sends `Request::Plugin{id, request}`; the helper's registry dispatches
//! to its plugin and returns the `PluginResult` envelope. This is the transport a
//! real x86 helper will use for the idbox `ReadDocument` chip-read step (the helper
//! currently hosts an echo placeholder — see `src/bin/helper.rs`).

use std::path::Path;

use sentinel_ipc::{Request, Response};

#[test]
fn plugin_request_passthrough_and_unknown_id() {
    let exe = Path::new(env!("CARGO_BIN_EXE_sentinel-ipc-helper"));
    let mut helper = sentinel_ipc_pipe::spawn_helper(exe).expect("spawn helper");

    // A known plugin id → its envelope (the echo plugin returns the request as data).
    let echoed = helper
        .client()
        .call(Request::Plugin { id: "Test.Echo".to_string(), request: "ping-123".to_string() })
        .expect("echo call");
    match echoed {
        Response::Plugin { envelope } => {
            assert!(envelope.contains(r#""success":true"#), "env: {envelope}");
            assert!(envelope.contains("ping-123"), "env: {envelope}");
        }
        other => panic!("expected Response::Plugin, got {other:?}"),
    }

    // Unknown plugin id → a failing envelope, not a transport error.
    let unknown = helper
        .client()
        .call(Request::Plugin { id: "Nope".to_string(), request: "{}".to_string() })
        .expect("unknown call");
    match unknown {
        Response::Plugin { envelope } => assert!(envelope.contains(r#""success":false"#)),
        other => panic!("expected Response::Plugin, got {other:?}"),
    }

    helper.shutdown().expect("shutdown");
}
