#[test]
fn csv_reader_constructs() {
    use concord_codecs::csv::{CsvOptions, CsvReader, HeaderMode};
    assert!(CsvReader::new(CsvOptions::new(HeaderMode::Present, b',')).is_ok());
}
