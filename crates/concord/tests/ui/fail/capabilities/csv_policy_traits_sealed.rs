// §9.5: first-party CSV header/delimiter policies are closed library-owned
// markers; downstream crates cannot add policy implementations.
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
