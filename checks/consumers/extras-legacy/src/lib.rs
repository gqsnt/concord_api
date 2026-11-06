#[test]
fn legacy_paths_are_the_same_items() {
    use concord_extras::{Json, csv::{CsvOptions, CsvReader, HeaderMode}, stream::ReadError};
    fn options(value: CsvOptions) -> CsvOptions { value }
    fn marker(value: Option<Json>) -> Option<Json> { value }
    fn reader(value: CsvReader) -> CsvReader { value }
    fn error(value: Option<ReadError>) -> Option<ReadError> { value }
    let opts = options(concord_codecs::csv::CsvOptions::new(HeaderMode::Present, b','));
    let _ = marker(None::<concord_codecs::Json>);
    let _ = reader(concord_codecs::csv::CsvReader::new(opts).unwrap());
    let _: Option<concord_codecs::stream::ReadError> = error(None::<concord_codecs::stream::ReadError>);
}
