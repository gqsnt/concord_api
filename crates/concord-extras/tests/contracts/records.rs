#![cfg(any(feature = "csv", feature = "ndjson"))]

use concord_core::contracts::{Buffered, PageResponseSpec, ResponseSpec};
use concord_core::records::RecordDecoder;
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    #[allow(dead_code)]
    id: u32,
}

#[cfg(feature = "csv")]
type ConfCsv<T> = concord_extras::records::Csv<
    T,
    concord_extras::records::HasHeader,
    concord_extras::records::SemicolonDelimiter,
>;

#[cfg(feature = "csv")]
type HeaderlessTsv<T> = concord_extras::records::Csv<
    T,
    concord_extras::records::NoHeader,
    concord_extras::records::TabDelimiter,
>;

#[test]
fn concrete_record_decoders_expose_finite_public_core_contracts() {
    fn complete_page_spec<S>()
    where
        S: ResponseSpec<Output = Vec<Row>, Mode = Buffered> + PageResponseSpec,
    {
    }

    #[cfg(feature = "csv")]
    {
        complete_page_spec::<concord_extras::records::Csv<Row>>();
        assert_eq!(
            <concord_extras::records::Csv<Row> as RecordDecoder>::accept(),
            Some("text/csv")
        );
        assert_eq!(
            <concord_extras::records::Csv<Row> as RecordDecoder>::max_buffered_bytes().get(),
            1024 * 1024
        );
        assert_eq!(
            <HeaderlessTsv<(u32,)> as RecordDecoder>::accept(),
            Some("text/csv")
        );
        assert_eq!(
            <ConfCsv<Row> as RecordDecoder>::max_buffered_bytes().get(),
            1024 * 1024
        );
    }
    #[cfg(feature = "ndjson")]
    {
        complete_page_spec::<concord_extras::records::NdJson<Row>>();
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

#[cfg(feature = "csv")]
#[test]
fn csv_policy_markers_and_configured_aliases_satisfy_the_public_bounds() {
    use concord_extras::records::{
        CommaDelimiter, CsvDelimiter, CsvHeaderMode, HasHeader, NoHeader, SemicolonDelimiter,
        TabDelimiter,
    };

    fn header_mode<H: CsvHeaderMode>() {}
    fn delimiter<D: CsvDelimiter>() {}
    fn decoder<D: RecordDecoder>() {}

    header_mode::<HasHeader>();
    header_mode::<NoHeader>();
    delimiter::<CommaDelimiter>();
    delimiter::<SemicolonDelimiter>();
    delimiter::<TabDelimiter>();
    decoder::<ConfCsv<Row>>();
    decoder::<HeaderlessTsv<(String, u32)>>();
}
