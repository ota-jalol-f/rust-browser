//! A synchronous request/response transport over any byte stream (`Read + Write`),
//! built on the length-delimited [`crate::wire`] framing and the correlation
//! envelopes.
//!
//! This is what carries a [`RequestExecutor`] out of process: the host runs a
//! [`Client`] over one end of a pipe, a small helper process runs [`serve`] over
//! the other end with the real executor. The motivating case is an **x86 helper**
//! for an x86-only vendor SDK (e.g. the Elyctis ID-card chip SDK) that an x64
//! host cannot load in-process — the helper isolates it, and a crash there never
//! takes down the browser.
//!
//! Transport-agnostic on purpose: it works over any stream, so it is unit-tested
//! here over a loopback TCP stream. The named-pipe binding and the helper
//! executable (which need platform code) live in a host crate, not here, keeping
//! this crate `#![forbid(unsafe_code)]` and dependency-free.

use std::io::{Read, Write};

use sentinel_core::{Error, Result};

use crate::executor::RequestExecutor;
use crate::message::{Request, RequestEnvelope, Response, ResponseEnvelope};
use crate::wire::{encode, FrameReader};

/// Read buffer size for one stream read; frames are reassembled by [`FrameReader`].
const READ_CHUNK: usize = 8192;

/// Serve requests from `stream` on `executor` until the peer closes it. Each
/// decoded [`RequestEnvelope`] is executed and its [`ResponseEnvelope`] (same
/// `id`) is written back. Returns `Ok(())` at EOF.
///
/// # Errors
/// [`Error`] on a framing/decode error or an I/O failure.
pub fn serve<S, E>(mut stream: S, executor: &mut E) -> Result<()>
where
    S: Read + Write,
    E: RequestExecutor,
{
    let mut reader = FrameReader::new();
    let mut buf = [0u8; READ_CHUNK];
    loop {
        // Drain any complete requests already buffered, then read more.
        while let Some(env) = reader.next_message::<RequestEnvelope>()? {
            let response = executor.execute(env.request);
            let frame = encode(&ResponseEnvelope { id: env.id, response })?;
            stream
                .write_all(&frame)
                .map_err(|e| Error::Engine(format!("ipc write failed: {e}")))?;
            stream.flush().map_err(|e| Error::Engine(format!("ipc flush failed: {e}")))?;
        }
        let n = stream
            .read(&mut buf)
            .map_err(|e| Error::Engine(format!("ipc read failed: {e}")))?;
        if n == 0 {
            return Ok(());
        }
        reader.push(&buf[..n]);
    }
}

/// The client half: sends [`Request`]s over a stream and reads back the correlated
/// [`Response`]. One outstanding call at a time (the helper serves serially).
pub struct Client<S> {
    stream: S,
    reader: FrameReader,
    next_id: u64,
    buf: [u8; READ_CHUNK],
}

impl<S: Read + Write> Client<S> {
    /// Wrap a connected stream.
    pub fn new(stream: S) -> Self {
        Self { stream, reader: FrameReader::new(), next_id: 1, buf: [0u8; READ_CHUNK] }
    }

    /// Send `request` and block until its correlated [`Response`] arrives.
    ///
    /// # Errors
    /// [`Error`] on framing/I/O failure, or if the stream closes before a response
    /// with the matching id arrives.
    pub fn call(&mut self, request: Request) -> Result<Response> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let frame = encode(&RequestEnvelope { id, request })?;
        self.stream
            .write_all(&frame)
            .map_err(|e| Error::Engine(format!("ipc write failed: {e}")))?;
        self.stream.flush().map_err(|e| Error::Engine(format!("ipc flush failed: {e}")))?;
        loop {
            if let Some(env) = self.reader.next_message::<ResponseEnvelope>()? {
                if env.id == id {
                    return Ok(env.response);
                }
                // A serial client never has a stale id; ignore and keep reading.
                continue;
            }
            let n = self
                .stream
                .read(&mut self.buf)
                .map_err(|e| Error::Engine(format!("ipc read failed: {e}")))?;
            if n == 0 {
                return Err(Error::Engine("ipc stream closed before response".into()));
            }
            self.reader.push(&self.buf[..n]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    /// A canned executor: distinct responses so the client can prove correlation.
    #[derive(Default)]
    struct Mock {
        calls: u32,
    }

    impl RequestExecutor for Mock {
        fn execute(&mut self, request: Request) -> Response {
            self.calls += 1;
            match request {
                Request::UsbList => Response::UsbDevices(Vec::new()),
                Request::SerialList => Response::SerialPorts(Vec::new()),
                _ => Response::Error { message: "unsupported".into() },
            }
        }
    }

    #[test]
    fn client_server_round_trip_over_a_stream() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        // Server end: serve the mock executor until the client disconnects.
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut exec = Mock::default();
            serve(stream, &mut exec).unwrap();
        });

        let mut client = Client::new(TcpStream::connect(addr).unwrap());
        assert!(matches!(client.call(Request::UsbList).unwrap(), Response::UsbDevices(_)));
        assert!(matches!(client.call(Request::SerialList).unwrap(), Response::SerialPorts(_)));
        assert!(matches!(
            client.call(Request::DllUnload { handle: 1 }).unwrap(),
            Response::Error { .. }
        ));

        drop(client); // closes the stream -> serve returns
        server.join().unwrap();
    }

    #[test]
    fn client_errors_when_stream_closes_before_response() {
        // A server that accepts then immediately drops the connection.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let _ = listener.accept().unwrap();
            // drop immediately
        });
        let mut client = Client::new(TcpStream::connect(addr).unwrap());
        // The call must fail; the exact error is platform-dependent (a write/read
        // reset on Windows, or EOF "closed before response" elsewhere).
        assert!(client.call(Request::UsbList).is_err());
        server.join().unwrap();
    }
}
