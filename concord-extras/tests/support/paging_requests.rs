use concord_testkit::{script::Request, Script};

fn transcript(requests: &[Request]) -> Vec<u8> {
    use std::io::Write;
    let mut bytes = Vec::new();
    for request in requests {
        writeln!(bytes, "{} {}", request.method(), request.url()).unwrap();
        // Header names have no relative order. The values of each name keep their order.
        let mut names: Vec<_> = request.headers().keys().collect();
        names.sort_unstable_by_key(|name| name.as_str());
        for name in names {
            for value in request.headers().get_all(name) {
                write!(bytes, "{name}: ").unwrap();
                bytes.extend_from_slice(value.as_bytes());
                bytes.push(b'\n');
            }
        }
        if let Some(body) = request.body() {
            write!(bytes, "\n{} bytes\n", body.len()).unwrap();
            bytes.extend_from_slice(body);
        } else {
            bytes.extend_from_slice(b"\nno body\n");
        }
    }
    bytes
}

pub(super) fn assert_requests(data: &str, script: &Script, name: &str) {
    let section = format!("=== {name} ===\n");
    let (_, expected) = data.split_once(&section).unwrap();
    let expected = expected.split("=== ").next().unwrap().as_bytes();
    let sent = transcript(&script.requests());
    assert!(
        sent == expected,
        "{name}\nexpected:\n{}\nrecorded:\n{}",
        String::from_utf8_lossy(expected),
        String::from_utf8_lossy(&sent),
    );
    assert_eq!(script.unused(), 0);
}
