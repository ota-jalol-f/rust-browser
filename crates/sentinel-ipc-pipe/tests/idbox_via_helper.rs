//! End-to-end: run the idbox plugin inside the out-of-process helper. The host
//! sends `Request::Plugin{"Asbt.IdBox", …}` over the real named pipe; the helper's
//! registry dispatches to the (fake-device) idbox plugin and returns its envelope.
//! This is the path a real x86 helper will use with the Elyctis ElyMRTD FFI.

use std::path::Path;

use sentinel_ipc::{Request, Response};

#[test]
fn idbox_read_chip_uid_and_document_via_helper() {
    let exe = Path::new(env!("CARGO_BIN_EXE_sentinel-ipc-helper"));
    let mut helper = sentinel_ipc_pipe::spawn_helper(exe).expect("spawn helper");

    // ReadChipUid → 14 hex chars in a success envelope.
    let uid = helper
        .client()
        .call(Request::Plugin {
            id: "Asbt.IdBox".to_string(),
            request: r#"{"method":"readChipUid"}"#.to_string(),
        })
        .expect("readChipUid call");
    match uid {
        Response::Plugin { envelope } => {
            assert!(envelope.contains(r#""success":true"#), "env: {envelope}");
            assert!(envelope.contains("04a1b2c3d4e5f6"), "env: {envelope}");
        }
        other => panic!("expected Response::Plugin, got {other:?}"),
    }

    // ReadDocument → reader-result JSON in `data`.
    let doc = helper
        .client()
        .call(Request::Plugin { id: "Asbt.IdBox".to_string(), request: "{}".to_string() })
        .expect("readDocument call");
    match doc {
        Response::Plugin { envelope } => {
            assert!(envelope.contains("DocumentNumber"), "env: {envelope}");
            assert!(envelope.contains("BirthDate"), "env: {envelope}");
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
