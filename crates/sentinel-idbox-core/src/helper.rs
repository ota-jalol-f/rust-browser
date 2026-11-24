//! The helper side of the wire protocol: runs the SDK start, then serves requests until
//! `quit` or EOF (reference `Asbt.IdBox.Shared.HelperServer`). Transport-agnostic (any
//! [`BufRead`]/[`Write`]) so it can run over a real named pipe in the i686 helper, and
//! over in-memory buffers in tests. The device itself sits behind the [`ChipSdk`] seam.

use std::io::{BufRead, Write};

use crate::chip::{ChipReadJob, ElyMrtd, PhotoDecoder};
use crate::errortext;
use crate::protocol::{self, Command, HelperMessage, HelperRequest, MessageType, VERSION};

/// The SDK objects the `FormReader` constructor creates, behind one seam: the real x86
/// Elyctis SDK in the helper (`device` feature), a fake in tests.
pub trait ChipSdk {
    /// `new ElyMRTDDotNet()`: the chip-reader handle, or `Err(message)` when it cannot
    /// be created (the chip is then unavailable → the MRZ fallback).
    fn create_ely(&mut self) -> Result<Box<dyn ElyMrtd>, String>;
    /// `new SCardManager().EstablishContext(...) + new ElyRDRAntennaSelect(...)`.
    fn init_reader_manager(&mut self) -> Result<(), String>;
    /// `ElyRDRControl.RDR_CloseComm()`.
    fn close_comm(&mut self) -> Result<(), String>;
    /// The chip-photo decoder (FreeImage in the helper).
    fn photo_decoder(&self) -> &dyn PhotoDecoder;
}

/// Runs [`ChipSdk`] over a line protocol: `hello`, then one `result` per request (with
/// interleaved `status`/`alert` lines for a chip read).
pub struct HelperServer<'a> {
    sdk: &'a mut dyn ChipSdk,
    ely: Option<Box<dyn ElyMrtd>>,
}

impl<'a> HelperServer<'a> {
    /// A server over `sdk`.
    pub fn new(sdk: &'a mut dyn ChipSdk) -> Self {
        HelperServer { sdk, ely: None }
    }

    /// Send `hello`, then serve requests until `quit` or EOF.
    pub fn run<R: BufRead, W: Write>(&mut self, input: R, mut output: W) {
        let hello = self.hello();
        write_line(&mut output, &hello);

        for line in input.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            if line.is_empty() {
                continue;
            }

            let request: HelperRequest = match protocol::deserialize(&line) {
                Ok(r) => r,
                Err(e) => {
                    let mut bad = HelperMessage::result(0);
                    bad.error = Some(format!("Bad request: {e}"));
                    write_line(&mut output, &bad);
                    continue;
                }
            };

            if request.cmd.as_deref() == Some(Command::QUIT) {
                return;
            }

            let response = self.handle(request, &mut output);
            write_line(&mut output, &response);
        }
    }

    fn hello(&mut self) -> HelperMessage {
        let mut hello = HelperMessage {
            type_: MessageType::HELLO.to_string(),
            protocol: Some(VERSION),
            ok: Some(true),
            ..Default::default()
        };

        match self.sdk.create_ely() {
            Ok(ely) => self.ely = Some(ely),
            Err(err) => {
                hello.ok = Some(false);
                hello.error = Some(errortext::get_all_messages(&[&err]));
            }
        }

        if let Err(err) = self.sdk.init_reader_manager() {
            hello.reader_init_error = Some(errortext::get_all_messages(&[&err]));
        }

        hello
    }

    fn handle<W: Write>(&mut self, request: HelperRequest, output: &mut W) -> HelperMessage {
        let mut result = HelperMessage::result(request.id);
        match request.cmd.as_deref() {
            Some(Command::CLOSE_COMM) => {
                match self.sdk.close_comm() {
                    Ok(()) => result.ok = Some(true),
                    Err(e) => {
                        result.ok = Some(false);
                        result.error = Some(errortext::get_all_messages(&[&e]));
                    }
                }
                result
            }
            Some(Command::READ_CHIP) => {
                if self.ely.is_none() {
                    result.ok = Some(false);
                    result.error = Some("ElyMRTDDotNet is not available".to_string());
                    return result;
                }

                let reader = request.reader.unwrap_or_default();
                let mrz_info = request.mrz_info.unwrap_or_default();
                let decoder = self.sdk.photo_decoder();
                let ely = self.ely.as_mut().unwrap().as_mut();
                let mut send = |m: HelperMessage| write_line(output, &m);
                ChipReadJob::new(ely, decoder, request.id, &mut send).run(&reader, &mrz_info)
            }
            other => {
                result.ok = Some(false);
                result.error = Some(format!("Unknown command: {}", other.unwrap_or("")));
                result
            }
        }
    }
}

fn write_line<W: Write>(output: &mut W, message: &HelperMessage) {
    let mut line = protocol::serialize(message);
    line.push('\n');
    let _ = output.write_all(line.as_bytes());
    let _ = output.flush();
}
