#![cfg(any(feature = "csv", feature = "ndjson"))]

use concord_core::contracts::{Buffered, ItemStream, ResponseRepresentation};
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    #[allow(dead_code)]
    id: u32,
}

#[cfg(feature = "csv")]
type ConfCsv = concord_extras::records::Csv<
    concord_extras::records::HasHeader,
    concord_extras::records::SemicolonDelimiter,
>;

#[cfg(feature = "csv")]
type HeaderlessTsv = concord_extras::records::Csv<
    concord_extras::records::NoHeader,
    concord_extras::records::TabDelimiter,
>;

#[test]
fn concrete_record_decoders_expose_finite_public_core_contracts() {
    fn complete_page_spec<S>()
    where
        S: ResponseRepresentation<Vec<Row>, Driver = Buffered>,
    {
    }
    fn item_stream_spec<S>()
    where
        S: ResponseRepresentation<ItemStream<Row>>,
    {
    }

    #[cfg(feature = "csv")]
    {
        complete_page_spec::<concord_extras::records::Csv>();
        item_stream_spec::<concord_extras::records::Csv>();
        assert_eq!(
            <concord_extras::records::Csv as ResponseRepresentation<Vec<Row>>>::ACCEPT,
            Some("text/csv")
        );
        complete_page_spec::<ConfCsv>();
        item_stream_spec::<HeaderlessTsv>();
    }
    #[cfg(feature = "ndjson")]
    {
        complete_page_spec::<concord_extras::records::NdJson>();
        item_stream_spec::<concord_extras::records::NdJson>();
        assert_eq!(
            <concord_extras::records::NdJson as ResponseRepresentation<Vec<Row>>>::ACCEPT,
            Some("application/x-ndjson")
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
    fn representation<R: ResponseRepresentation<Vec<Row>>>() {}

    header_mode::<HasHeader>();
    header_mode::<NoHeader>();
    delimiter::<CommaDelimiter>();
    delimiter::<SemicolonDelimiter>();
    delimiter::<TabDelimiter>();
    representation::<ConfCsv>();
    representation::<HeaderlessTsv>();
}
