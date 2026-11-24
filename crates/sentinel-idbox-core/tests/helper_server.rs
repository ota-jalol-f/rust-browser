//! `HelperServer` driven over in-memory buffers — the Rust counterpart of the
//! `HelperServer` half of `HelperIpcTests` (the real named-pipe client is covered on
//! the plugin side).

mod common;

use std::io::Cursor;

use common::{FakeChipSdk, FakeEly, INFO, READER};
use sentinel_idbox_core::helper::HelperServer;
use sentinel_idbox_core::protocol::{self, Command, HelperMessage, HelperRequest, MessageType};
use sentinel_idbox_core::texts;

fn serve(sdk: &mut FakeChipSdk, input: &str) -> Vec<HelperMessage> {
    let mut output: Vec<u8> = Vec::new();
    HelperServer::new(sdk).run(Cursor::new(input.as_bytes()), &mut output);
    String::from_utf8(output)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| protocol::deserialize::<HelperMessage>(l).unwrap())
        .collect()
}

#[test]
fn protocol_wire_format_is_stable() {
    let request = protocol::serialize(&HelperRequest {
        id: 2,
        cmd: Some(Command::READ_CHIP.to_string()),
        reader: Some("R".to_string()),
        mrz_info: Some("I".to_string()),
    });
    let status = protocol::serialize(&HelperMessage::status(2, "Узилди", false));

    assert_eq!(request, "{\"id\":2,\"cmd\":\"readChip\",\"reader\":\"R\",\"mrzInfo\":\"I\"}");
    assert_eq!(status, "{\"type\":\"status\",\"id\":2,\"text\":\"Узилди\",\"isError\":false}");
}

#[test]
fn server_unknown_command_and_bad_json_are_answered_not_fatal() {
    let mut output: Vec<u8> = Vec::new();
    let input = "not json\n{\"id\":3,\"cmd\":\"nope\"}\n{\"id\":4,\"cmd\":\"quit\"}\n{\"id\":5,\"cmd\":\"closeComm\"}\n";
    HelperServer::new(&mut FakeChipSdk::default()).run(Cursor::new(input.as_bytes()), &mut output);

    let text = String::from_utf8(output).unwrap();
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 3); // hello, bad request, unknown command; nothing after quit
    assert!(lines[0].starts_with("{\"type\":\"hello\",\"id\":0,\"protocol\":1,\"ok\":true"));
    assert!(lines[1].contains("Bad request"));
    assert!(lines[2].contains("Unknown command: nope"));
}

#[test]
fn hello_forwards_the_reader_init_error() {
    let mut sdk = FakeChipSdk {
        reader_manager_throws: Some("SCardEstablishContext error: -2146435043".into()),
        ..Default::default()
    };
    let out = serve(&mut sdk, "");
    assert_eq!(out[0].type_, MessageType::HELLO);
    assert_eq!(out[0].ok, Some(true));
    assert_eq!(out[0].reader_init_error.as_deref(), Some("SCardEstablishContext error: -2146435043\r\n"));
}

#[test]
fn read_happy_streams_statuses_then_result() {
    let mut sdk = FakeChipSdk::default();
    let request = protocol::serialize(&HelperRequest {
        id: 2,
        cmd: Some(Command::READ_CHIP.to_string()),
        reader: Some(READER.to_string()),
        mrz_info: Some(INFO.to_string()),
    });
    let out = serve(&mut sdk, &format!("{request}\n"));

    let statuses: Vec<String> = out
        .iter()
        .filter(|m| m.type_ == MessageType::STATUS)
        .map(|m| m.text.clone().unwrap_or_default())
        .collect();
    let result = out.iter().find(|m| m.type_ == MessageType::RESULT).unwrap();

    assert_eq!(result.code, Some(0));
    assert_eq!(result.dg1.as_ref().unwrap().len(), 10);
    assert!(result.photo.is_some());
    assert_eq!(statuses.first().map(String::as_str), Some(texts::CONNECTING));
    assert_eq!(statuses.last().map(String::as_str), Some(texts::DISCONNECTED));
    assert_eq!(statuses.len(), 10);
}

#[test]
fn close_comm_reaches_the_sdk() {
    let mut sdk = FakeChipSdk::default();
    let out = serve(&mut sdk, "{\"id\":5,\"cmd\":\"closeComm\"}\n");
    assert_eq!(sdk.close_comm_calls, 1);
    let result = out.iter().find(|m| m.type_ == MessageType::RESULT).unwrap();
    assert_eq!(result.ok, Some(true));
}

#[test]
fn ely_cannot_be_created_hello_not_ok_and_read_is_unavailable() {
    let mut sdk = FakeChipSdk {
        ely: Some(FakeEly::default()),
        create_throws: Some("Could not load file or assembly 'ElyMRTDdotNet'".into()),
        ..Default::default()
    };
    let out = serve(&mut sdk, "{\"id\":1,\"cmd\":\"readChip\",\"reader\":\"r\",\"mrzInfo\":\"i\"}\n");
    assert_eq!(out[0].ok, Some(false));
    assert!(out[0].error.as_deref().unwrap().contains("ElyMRTDdotNet"));
    let result = out.iter().find(|m| m.type_ == MessageType::RESULT).unwrap();
    assert_eq!(result.ok, Some(false));
    assert_eq!(result.error.as_deref(), Some("ElyMRTDDotNet is not available"));
}
