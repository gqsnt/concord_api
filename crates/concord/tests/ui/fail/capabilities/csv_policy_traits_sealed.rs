use concord::records::{CsvDelimiter, CsvHeaderMode};

struct CustomHeader;

impl CsvHeaderMode for CustomHeader {
    const HAS_HEADER: bool = true;
}

struct PipeDelimiter;

impl CsvDelimiter for PipeDelimiter {
    const BYTE: u8 = b'|';
}

fn main() {}
