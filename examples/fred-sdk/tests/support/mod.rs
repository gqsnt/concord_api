use concord_example_testserver::TestServer;
use concord_sdk_fred::{Fred, FredConfig, concord};

pub const KEY: &str = "secret-fred-test-key";

/// Runtime synthetic fixture: a valid stored archive whose wire size exceeds the cap.
pub fn oversized_archive() -> Vec<u8> {
    use std::io::{Cursor, Write};
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(
            "SYN_A.csv",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
    writer
        .write_all(&vec![b'x'; concord_sdk_fred::BODY_LIMIT])
        .unwrap();
    writer.finish().unwrap().into_inner()
}

pub fn api(server: &TestServer) -> Fred {
    Fred::new(
        concord_sdk_fred::http_client().unwrap(),
        FredConfig {
            api_key: KEY.into(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
}

pub fn kind(error: &concord_sdk_fred::Error) -> concord::ErrorKind {
    match error {
        concord_sdk_fred::Error::Concord(e) => e.kind(),
        other => panic!("expected Concord error, got {other:?}"),
    }
}

pub fn safe(error: &concord_sdk_fred::Error) {
    assert!(!format!("{error}").contains(KEY));
    assert!(!format!("{error:?}").contains(KEY));
    assert!(std::error::Error::source(error).is_none());
}
