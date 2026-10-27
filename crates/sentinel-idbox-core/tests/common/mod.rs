//! Shared test doubles — the Rust counterparts of the C# `TestDoubles.cs` fakes.

#![allow(dead_code)]

use sentinel_idbox_core::chip::{ElyMrtd, PhotoDecoder};
use sentinel_idbox_core::helper::ChipSdk;

pub const READER: &str = "ELYCTIS CL reader 0123456789ABCDEF 0";
pub const INFO: &str = "AA1234567890010113001019";
pub const TD3: &str = "P<UZBTOSHMATOV<<ALISHER<<<<<<<<<<<<<<<<<<<<<\r\nAA12345678UZB9001011M30010193123456789012324\r\n";
pub const TD1: &str = "IDUZBAD1234567741508850123456<\r\n8508155F3208140UZB<<<<<<<<<<<6\r\nRASULOVA<<MALIKA<<<<<<<<<<<<<<\r\n";

/// `ElyMRTDDotNet` with scripted answers; every call logged (reference `FakeEly`).
pub struct FakeEly {
    pub calls: Vec<String>,
    pub connect_result: i32,
    pub connect_throws: Option<String>,
    pub antenna: Vec<u8>,
    pub antenna_throws: Option<String>,
    pub bac: bool,
    pub dg1_result: i32,
    pub dg2_result: i32,
    pub dg6_result: i32,
    pub photo: Option<Vec<u8>>,
    pub categories: Option<Vec<String>>,
    pub disconnect_throws: Option<String>,
}

impl Default for FakeEly {
    fn default() -> Self {
        FakeEly {
            calls: Vec::new(),
            connect_result: 0,
            connect_throws: None,
            antenna: vec![0],
            antenna_throws: None,
            bac: true,
            dg1_result: 0,
            dg2_result: 0,
            dg6_result: 0,
            photo: Some(vec![0xFF, 0xD8, 0xFF, 0xD9]),
            categories: Some(Vec::new()),
            disconnect_throws: None,
        }
    }
}

impl ElyMrtd for FakeEly {
    fn connect(&mut self, reader_name: &str) -> Result<i32, String> {
        self.calls.push(format!("connect:{reader_name}"));
        match &self.connect_throws {
            Some(e) => Err(e.clone()),
            None => Ok(self.connect_result),
        }
    }
    fn get_select_antenna(&mut self) -> Result<Vec<u8>, String> {
        self.calls.push("getSelectAntenna".into());
        match &self.antenna_throws {
            Some(e) => Err(e.clone()),
            None => Ok(self.antenna.clone()),
        }
    }
    fn establish_bac(&mut self, mrz_info: &str) -> Result<bool, String> {
        self.calls.push(format!("establishBAC:{mrz_info}"));
        Ok(self.bac)
    }
    fn read_dg1(&mut self) -> Result<i32, String> {
        self.calls.push("readDG1".into());
        Ok(self.dg1_result)
    }
    fn read_dg2(&mut self) -> Result<i32, String> {
        self.calls.push("readDG2".into());
        Ok(self.dg2_result)
    }
    fn read_dg6(&mut self) -> Result<i32, String> {
        self.calls.push("readDG6".into());
        Ok(self.dg6_result)
    }
    fn get_name(&mut self) -> String { "ALISHER".into() }
    fn get_surname(&mut self) -> String { "TOSHMATOV".into() }
    fn get_birth_date(&mut self) -> String { "01/01/90".into() }
    fn get_nationality(&mut self) -> String { "UZB".into() }
    fn get_issuing_state(&mut self) -> String { "UZB".into() }
    fn get_country_name(&mut self, code: &str) -> String {
        if code == "UZB" { "Uzbekistan".into() } else { code.into() }
    }
    fn get_sex(&mut self) -> String { "M".into() }
    fn get_validity_date(&mut self) -> String { "01/01/30".into() }
    fn get_doc_num(&mut self) -> String { "AA1234567".into() }
    fn get_document_type(&mut self) -> String { "P".into() }
    fn get_optional_data(&mut self) -> String { "31234567890123".into() }
    fn get_family_name(&mut self) -> String { "TOSHMATOV".into() }
    fn get_given_names(&mut self) -> String { "ALISHER".into() }
    fn get_expiry_date(&mut self) -> String { "01/01/30".into() }
    fn get_issuing_date(&mut self) -> String { "01/01/20".into() }
    fn get_issuing_authority(&mut self) -> String { "IIV".into() }
    fn get_issuing_country(&mut self) -> String { "UZB".into() }
    fn get_licence_number(&mut self) -> String { "AF1234567".into() }
    fn get_categories(&mut self) -> Result<Option<Vec<String>>, String> {
        Ok(self.categories.clone())
    }
    fn get_photo(&mut self) -> Result<Option<Vec<u8>>, String> {
        self.calls.push("getPhoto".into());
        Ok(self.photo.clone())
    }
    fn disconnect(&mut self) -> Result<(), String> {
        self.calls.push("disconnect".into());
        match &self.disconnect_throws {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
}

#[derive(Default)]
pub struct FakeDecoder {
    pub decode_fails: bool,
    pub encode_fails: bool,
}

impl PhotoDecoder for FakeDecoder {
    fn decode(&self, photo: &[u8]) -> Option<Vec<u8>> {
        if self.decode_fails {
            None
        } else {
            Some(photo.to_vec())
        }
    }
    fn to_jpeg_base64(&self, decoded: &[u8]) -> Option<String> {
        if self.encode_fails {
            None
        } else {
            Some(base64(decoded))
        }
    }
}

/// Standard base64 (matches C#'s `Convert.ToBase64String`).
pub fn base64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    out
}

/// `IChipSdk` with scripted behaviour (reference `FakeChipSdk`).
pub struct FakeChipSdk {
    pub ely: Option<FakeEly>,
    pub create_throws: Option<String>,
    pub reader_manager_throws: Option<String>,
    pub close_comm_throws: Option<String>,
    pub close_comm_calls: usize,
    pub decoder: FakeDecoder,
}

impl Default for FakeChipSdk {
    fn default() -> Self {
        FakeChipSdk {
            ely: Some(FakeEly::default()),
            create_throws: None,
            reader_manager_throws: None,
            close_comm_throws: None,
            close_comm_calls: 0,
            decoder: FakeDecoder::default(),
        }
    }
}

impl ChipSdk for FakeChipSdk {
    fn create_ely(&mut self) -> Result<Box<dyn ElyMrtd>, String> {
        if let Some(e) = &self.create_throws {
            return Err(e.clone());
        }
        let ely = self.ely.take().unwrap_or_default();
        Ok(Box::new(ely))
    }
    fn init_reader_manager(&mut self) -> Result<(), String> {
        match &self.reader_manager_throws {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
    fn close_comm(&mut self) -> Result<(), String> {
        self.close_comm_calls += 1;
        match &self.close_comm_throws {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
    fn photo_decoder(&self) -> &dyn PhotoDecoder {
        &self.decoder
    }
}
