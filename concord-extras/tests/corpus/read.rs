//! Reads a corpus file: one case per line, fields separated by tabs, `#` for comments.
//! Within a field, `\\`, `\t`, `\n`, `\r`, `\0` and `\xHH` stand for their bytes.
#![allow(clippy::panic, dead_code)]

/// The cases of a corpus file, each a list of fields, with the line it came from.
pub fn cases(text: &str) -> Vec<(usize, Vec<Vec<u8>>)> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
        .map(|(n, line)| (n + 1, line.split('\t').map(unescape).collect()))
        .collect()
}

fn unescape(field: &str) -> Vec<u8> {
    let bytes = field.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        at += 1;
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        let next = bytes.get(at).copied();
        at += 1;
        match next {
            Some(b't') => out.push(b'\t'),
            Some(b'n') => out.push(b'\n'),
            Some(b'r') => out.push(b'\r'),
            Some(b'0') => out.push(0),
            Some(b'\\') => out.push(b'\\'),
            Some(b'x') => {
                let hex =
                    std::str::from_utf8(bytes.get(at..at + 2).unwrap_or_default()).unwrap_or("");
                let Ok(byte) = u8::from_str_radix(hex, 16) else {
                    panic!("a bad escape in the corpus field {field:?}");
                };
                out.push(byte);
                at += 2;
            }
            _ => panic!("a bad escape in the corpus field {field:?}"),
        }
    }
    out
}

/// A field as text.
pub fn text(field: &[u8]) -> &str {
    std::str::from_utf8(field).unwrap_or_default()
}
