#![cfg(any(feature = "csv", feature = "ndjson"))]

use concord_core::records::RecordDecoder;
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    #[allow(dead_code)]
    id: u32,
}

#[test]
fn concrete_record_decoders_expose_finite_public_core_contracts() {
    #[cfg(feature = "csv")]
    {
        assert_eq!(
            <concord_extras::records::Csv<Row> as RecordDecoder>::accept(),
            Some("text/csv")
        );
        assert_eq!(
            <concord_extras::records::Csv<Row> as RecordDecoder>::max_buffered_bytes().get(),
            1024 * 1024
        );
    }
    #[cfg(feature = "ndjson")]
    {
        assert_eq!(
            <concord_extras::records::NdJson<Row> as RecordDecoder>::accept(),
            Some("application/x-ndjson")
        );
        assert_eq!(
            <concord_extras::records::NdJson<Row> as RecordDecoder>::max_buffered_bytes().get(),
            1024 * 1024
        );
    }
}
