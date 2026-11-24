//! Length-delimited framing for IPC messages over a byte stream (pipe/socket).
//!
//! Each frame is a 4-byte big-endian length prefix followed by that many bytes of
//! JSON. [`encode`] produces a frame; [`FrameReader`] accumulates incoming bytes
//! and yields complete frames, tolerating reads that split or coalesce frames —
//! the reality of stream transports.

use serde::de::DeserializeOwned;
use serde::Serialize;
use sentinel_core::{Error, Result};

/// Maximum accepted frame payload size (16 MiB), guarding against a corrupt or
/// hostile length prefix causing an unbounded allocation.
pub const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

/// Serialize `value` to a length-prefixed JSON frame.
///
/// # Errors
/// [`Error::Engine`] if serialization fails or the payload exceeds [`MAX_FRAME_LEN`].
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(value)
        .map_err(|e| Error::Engine(format!("ipc encode failed: {e}")))?;
    if payload.len() > MAX_FRAME_LEN {
        return Err(Error::Engine(format!(
            "ipc frame too large: {} > {MAX_FRAME_LEN}",
            payload.len()
        )));
    }
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Accumulates bytes from a stream and yields whole decoded messages.
///
/// Feed it whatever a read returns via [`FrameReader::push`]; pull complete
/// messages with [`FrameReader::next_message`] until it returns `None`.
#[derive(Debug, Default)]
pub struct FrameReader {
    buf: Vec<u8>,
}

impl FrameReader {
    /// Create an empty reader.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append freshly-read bytes to the internal buffer.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Decode and remove the next complete message, if one is fully buffered.
    ///
    /// Returns `Ok(None)` when more bytes are needed.
    ///
    /// # Errors
    /// [`Error::Engine`] if a frame announces a length over [`MAX_FRAME_LEN`] or
    /// its payload is not valid JSON for `T`.
    pub fn next_message<T: DeserializeOwned>(&mut self) -> Result<Option<T>> {
        if self.buf.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        if len > MAX_FRAME_LEN {
            return Err(Error::Engine(format!("ipc frame too large: {len} > {MAX_FRAME_LEN}")));
        }
        if self.buf.len() < 4 + len {
            return Ok(None); // payload not fully arrived yet
        }
        let payload = self.buf[4..4 + len].to_vec();
        // Drop the consumed frame from the front of the buffer.
        self.buf.drain(..4 + len);
        let value = serde_json::from_slice(&payload)
            .map_err(|e| Error::Engine(format!("ipc decode failed: {e}")))?;
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Request, RequestEnvelope};

    fn sample(id: u64) -> RequestEnvelope {
        RequestEnvelope { id, request: Request::SerialList }
    }

    #[test]
    fn encode_then_read_one() {
        let frame = encode(&sample(1)).unwrap();
        let mut r = FrameReader::new();
        r.push(&frame);
        let got: RequestEnvelope = r.next_message().unwrap().unwrap();
        assert_eq!(got, sample(1));
        // Buffer drained: no more messages.
        assert!(r.next_message::<RequestEnvelope>().unwrap().is_none());
    }

    #[test]
    fn frame_split_across_chunks_is_reassembled() {
        let frame = encode(&sample(2)).unwrap();
        let (a, b) = frame.split_at(3); // split inside the length prefix
        let mut r = FrameReader::new();
        r.push(a);
        assert!(r.next_message::<RequestEnvelope>().unwrap().is_none());
        r.push(b);
        assert_eq!(r.next_message::<RequestEnvelope>().unwrap().unwrap(), sample(2));
    }

    #[test]
    fn two_coalesced_frames_yield_in_order() {
        let mut bytes = encode(&sample(10)).unwrap();
        bytes.extend(encode(&sample(11)).unwrap());
        let mut r = FrameReader::new();
        r.push(&bytes);
        assert_eq!(r.next_message::<RequestEnvelope>().unwrap().unwrap(), sample(10));
        assert_eq!(r.next_message::<RequestEnvelope>().unwrap().unwrap(), sample(11));
        assert!(r.next_message::<RequestEnvelope>().unwrap().is_none());
    }

    #[test]
    fn oversized_length_prefix_is_rejected() {
        let mut r = FrameReader::new();
        // Announce a 1 GiB frame — must error, not attempt to allocate/wait forever.
        r.push(&(1024u32 * 1024 * 1024).to_be_bytes());
        r.push(&[0u8; 8]);
        assert!(r.next_message::<RequestEnvelope>().is_err());
    }

    #[test]
    fn corrupt_payload_errors() {
        let mut r = FrameReader::new();
        r.push(&3u32.to_be_bytes());
        r.push(b"{ x"); // invalid JSON
        assert!(r.next_message::<RequestEnvelope>().is_err());
    }
}
