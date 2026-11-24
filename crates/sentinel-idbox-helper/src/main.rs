//! `sentinel-idbox-helper --pipe <name> [--parent <pid>]`
//!
//! The x86 chip helper: connects to the plugin's CurrentUser named pipe and runs
//! sentinel-idbox-core's `HelperServer` against the Elyctis SDK (real on `device`+x86, a
//! stub otherwise). Exits on `quit`, pipe EOF, or when the pipe breaks (parent gone).

use std::io::BufReader;
use std::process::ExitCode;

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{GenericNamespaced, Stream as LocalStream, ToNsName};
use sentinel_idbox_core::helper::HelperServer;
use sentinel_idbox_core::protocol::PIPE_PREFIX;

mod decoder;
mod sdk;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mut pipe_name: Option<String> = None;
    let mut i = 1;
    while i + 1 < args.len() {
        match args[i].as_str() {
            "--pipe" => pipe_name = Some(args[i + 1].clone()),
            "--parent" => {} // the pipe EOF already ends us when the parent dies
            _ => {}
        }
        i += 2;
    }

    let pipe_name = match pipe_name {
        Some(p) if p.starts_with(PIPE_PREFIX) => p,
        _ => {
            eprintln!("usage: sentinel-idbox-helper --pipe {PIPE_PREFIX}<id> [--parent <pid>]");
            return ExitCode::from(2);
        }
    };

    let name = match pipe_name.to_ns_name::<GenericNamespaced>() {
        Ok(n) => n,
        Err(e) => {
            eprintln!("bad pipe name: {e}");
            return ExitCode::from(2);
        }
    };

    let stream = match LocalStream::connect(name) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("pipe connect failed: {e}");
            return ExitCode::from(3);
        }
    };

    // `&Stream` implements both Read and Write for a duplex pipe, so the server can read
    // requests and write responses over the one connection.
    let mut sdk = sdk::make_sdk();
    HelperServer::new(sdk.as_mut()).run(BufReader::new(&stream), &stream);
    ExitCode::SUCCESS
}
