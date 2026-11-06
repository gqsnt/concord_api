#![allow(dead_code)]

use std::num::NonZeroUsize;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_google_drive::{ClientConfig, DriveClient, FileField, FileFields, NewFile};

pub const TOKEN: &str = "SYNTHETIC_TEST_BEARER_DO_NOT_USE";
pub const NAME: &str = "café-東京-🛰.bin";
pub const SMALL: &[u8] = include_bytes!("../../fixtures/small.bin");
pub const MULTIPART: &[u8] = include_bytes!("../../fixtures/multipart-related.bin");
pub const NEW_FILE: &[u8] = include_bytes!("../../fixtures/new-file.json");
pub const CREATED: &[u8] = include_bytes!("../../fixtures/file-created.json");
pub const RESUMED: &[u8] = include_bytes!("../../fixtures/file-resumable.json");
pub const FILES_1: &[u8] = include_bytes!("../../fixtures/files-page-1.json");
pub const FILES_2: &[u8] = include_bytes!("../../fixtures/files-page-2.json");
pub const START: &[u8] = include_bytes!("../../fixtures/start-token.json");
pub const CHANGES_1: &[u8] = include_bytes!("../../fixtures/changes-page-1.json");
pub const CHANGES_2: &[u8] = include_bytes!("../../fixtures/changes-page-2.json");
pub const CHANGES_3: &[u8] = include_bytes!("../../fixtures/changes-page-3.json");
pub const RATE_LIMIT: &[u8] = include_bytes!("../../fixtures/rate-limit.json");
pub const EXPIRED: &[u8] = include_bytes!("../../fixtures/expired-session.json");
pub const INVALID_FIELDS: &[u8] = include_bytes!("../../fixtures/invalid-fields.json");

pub fn client(server: &TestServer) -> DriveClient {
    DriveClient::with_config(
        TOKEN,
        ClientConfig {
            origin: server.url(),
            ..ClientConfig::default()
        },
    )
    .unwrap()
}

pub fn metadata() -> NewFile {
    NewFile {
        name: NAME.into(),
        ..NewFile::default()
    }
}

pub fn fields() -> FileFields {
    FileFields::only(&[FileField::Name, FileField::MimeType, FileField::Size])
}

pub fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

pub fn initiation(request: &Recorded) -> Reply {
    Reply::new(200).header(
        "location",
        format!(
            "http://{}/upload/drive/v3/files?uploadType=resumable&upload_id=SYNTHETIC_SESSION_ONLY",
            request.host
        ),
    )
}

pub fn auth(request: &Recorded) {
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
}

// Deterministic perturbation source, expanded from a hand-authored binary seed.
pub fn source() -> Vec<u8> {
    SMALL
        .iter()
        .copied()
        .cycle()
        .take(2 * concord_sdk_google_drive::CHUNK_ALIGNMENT + 37)
        .collect()
}
