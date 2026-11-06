#[test]
fn sse_reader_constructs() {
    use concord_codecs::sse::{SseOptions, SseReader};
    let _ = SseReader::new(SseOptions::new());
}
