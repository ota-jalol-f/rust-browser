//! The out-of-process IPC helper executable: connects to the host's named pipe
//! and serves requests until the pipe closes. It hosts a [`PluginRegistry`] and
//! answers `Request::Plugin` by dispatching to it — this is where an x86-only
//! vendor-SDK plugin (e.g. the idbox Elyctis chip reader) runs, isolated from the
//! x64 browser. Build for x86 with `--target i686-pc-windows-msvc`.

use sentinel_ipc::{Request, RequestExecutor, Response};
use sentinel_plugin::PluginRegistry;

/// The helper's executor: plugin passthrough plus a couple of trivial device
/// enumerations (stubbed) that the transport round-trip test exercises.
struct HelperExecutor {
    registry: PluginRegistry,
}

impl HelperExecutor {
    fn new() -> Self {
        let mut registry = PluginRegistry::new();
        // The idbox chip reader (fake device until the ElyMRTD FFI lands).
        let _ = registry.register(Box::new(sentinel_idbox::IdBoxPlugin::new()));
        HelperExecutor { registry }
    }
}

impl RequestExecutor for HelperExecutor {
    fn execute(&mut self, request: Request) -> Response {
        match request {
            Request::Plugin { id, request } => {
                let result = self.registry.dispatch(&id, &request);
                Response::Plugin { envelope: result.to_json() }
            }
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

    let mut exec = HelperExecutor::new();
    if let Err(e) = sentinel_ipc::serve(stream, &mut exec) {
        eprintln!("helper serve ended: {e}");
    }
}
