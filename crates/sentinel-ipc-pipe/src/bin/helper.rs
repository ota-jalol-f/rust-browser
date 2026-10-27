//! The out-of-process IPC helper executable: connects to the host's named pipe
//! and serves requests until the pipe closes. This stub uses a trivial executor;
//! a real x86 helper hosts a vendor SDK (e.g. the Elyctis chip reader) behind the
//! same [`RequestExecutor`] seam, built with `--target i686-pc-windows-msvc`.

use sentinel_ipc::{Request, RequestExecutor, Response};

/// Placeholder executor proving the transport end to end. Replace with a device /
/// vendor-SDK executor in a real helper.
struct HelperExecutor;

impl RequestExecutor for HelperExecutor {
    fn execute(&mut self, request: Request) -> Response {
        match request {
            Request::UsbList => Response::UsbDevices(Vec::new()),
            Request::SerialList => Response::SerialPorts(Vec::new()),
            _ => Response::Ack,
        }
    }
}

fn main() {
    let mut pipe = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--pipe" {
            pipe = args.next();
        }
    }
    let Some(pipe) = pipe else {
        eprintln!("usage: sentinel-ipc-helper --pipe <name>");
        std::process::exit(2);
    };

    let stream = match sentinel_ipc_pipe::connect(&pipe) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("helper connect failed: {e}");
            std::process::exit(1);
        }
    };

    let mut exec = HelperExecutor;
    if let Err(e) = sentinel_ipc::serve(stream, &mut exec) {
        eprintln!("helper serve ended: {e}");
    }
}
