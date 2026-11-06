//! A tiny HTTP/1.1 loopback server and an independent `multipart/form-data` reader.
//!
//! Neither uses an HTTP or MIME library: the server records the bytes reqwest put on the wire,
//! and [`parse_multipart`] reads them the way a recipient does (RFC 7578 framing, RFC 9110
//! quoted strings, RFC 8187 extended values), sharing no code with reqwest's writer.
#![allow(dead_code, missing_docs)]

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// One request as observed on the wire.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub target: String,
    /// Header lines in wire order, names lowercased.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        let all: Vec<&str> = self
            .headers
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
            .collect();
        assert!(all.len() <= 1, "header {name} occurs {} times", all.len());
        all.first().copied()
    }
}

/// A scripted answer: `None` closes the connection without answering.
pub type Reply = Option<(u16, &'static str, &'static str)>;

/// A running loopback server; dropping it stops accepting connections.
pub struct Server {
    pub addr: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<Recorded>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    /// Answers every request with `reply` (status, content type, body).
    pub async fn reply(reply: Reply) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let log = Arc::clone(&log);
                tokio::spawn(async move {
                    let _ = serve(stream, log, reply).await;
                });
            }
        });
        Self {
            addr,
            requests,
            task,
        }
    }

    /// The only request received; panics unless exactly one arrived.
    pub fn single(&self) -> Recorded {
        let all = self.requests.lock().unwrap().clone();
        assert_eq!(all.len(), 1, "expected exactly one request");
        all.into_iter().next().unwrap()
    }

    /// Every request received so far, in arrival order.
    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(
    mut stream: tokio::net::TcpStream,
    log: Arc<Mutex<Vec<Recorded>>>,
    reply: Reply,
) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let head_end = loop {
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            break pos;
        }
        let mut chunk = [0u8; 4096];
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    buf.drain(..head_end + 4);
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().splitn(3, ' ');
    let method = request_line.next().unwrap_or_default().to_owned();
    let target = request_line.next().unwrap_or_default().to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    // reqwest sends a `Content-Length` for buffered parts; a chunked body would be a regression.
    let len: usize = headers
        .iter()
        .find(|(n, _)| n == "content-length")
        .map(|(_, v)| v.parse().expect("numeric content-length"))
        .expect("multipart body with a known length carries content-length");
    while buf.len() < len {
        let mut chunk = [0u8; 4096];
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = buf[..len].to_vec();
    log.lock().unwrap().push(Recorded {
        method,
        target,
        headers,
        body,
    });
    let Some((status, content_type, body)) = reply else {
        return Ok(());
    };
    let out = format!(
        "HTTP/1.1 {status} X\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(out.as_bytes()).await?;
    stream.flush().await
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// One part as a recipient reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPart {
    /// Decoded field name (from `name="..."` or `name*=utf-8''...`).
    pub name: String,
    /// Decoded file name, if any.
    pub file_name: Option<String>,
    /// The part's `Content-Type`, if any.
    pub content_type: Option<String>,
    pub content: Vec<u8>,
    /// The raw `Content-Disposition` value, for wire-form assertions.
    pub disposition: String,
}

/// The `boundary` parameter of a `multipart/form-data` content type (token or quoted string).
pub fn boundary_of(content_type: &str) -> String {
    let (kind, params) = content_type.split_once(';').expect("parameters");
    assert!(
        kind.trim().eq_ignore_ascii_case("multipart/form-data"),
        "{content_type}"
    );
    let params = parse_params(params);
    let boundary = params
        .into_iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("boundary"))
        .map(|(_, v)| v)
        .expect("boundary parameter");
    assert!(
        (1..=70).contains(&boundary.len()),
        "RFC 2046 boundary length"
    );
    boundary
}

/// Splits a body into parts (RFC 2046 §5.1.1 with RFC 7578 constraints): no preamble, each
/// delimiter is `CRLF--boundary`, the last one is followed by `--`.
pub fn parse_multipart(body: &[u8], boundary: &str) -> Vec<ParsedPart> {
    let first = format!("--{boundary}\r\n");
    assert!(
        body.starts_with(first.as_bytes()),
        "body starts with the first delimiter"
    );
    let delimiter = format!("\r\n--{boundary}");
    let mut rest = &body[first.len()..];
    let mut parts = Vec::new();
    loop {
        let end = find(rest, delimiter.as_bytes()).expect("closing delimiter");
        parts.push(parse_part(&rest[..end]));
        rest = &rest[end + delimiter.len()..];
        if let Some(after) = rest.strip_prefix(b"--") {
            assert_eq!(after, b"\r\n", "nothing but CRLF after the close delimiter");
            return parts;
        }
        rest = rest.strip_prefix(b"\r\n").expect("CRLF after a delimiter");
    }
}

fn parse_part(raw: &[u8]) -> ParsedPart {
    let head_end = find(raw, b"\r\n\r\n").expect("part header block");
    let head = std::str::from_utf8(&raw[..head_end]).expect("UTF-8 part headers");
    let content = raw[head_end + 4..].to_vec();
    let mut disposition = None;
    let mut content_type = None;
    for line in head.split("\r\n") {
        let (name, value) = line.split_once(':').expect("header line");
        match name.to_ascii_lowercase().as_str() {
            "content-disposition" => {
                assert!(disposition.is_none(), "one Content-Disposition");
                disposition = Some(value.trim().to_owned());
            }
            "content-type" => content_type = Some(value.trim().to_owned()),
            other => panic!("unexpected part header {other}"),
        }
    }
    let disposition = disposition.expect("Content-Disposition");
    let (kind, params) = disposition.split_once(';').expect("disposition parameters");
    assert_eq!(kind.trim(), "form-data");
    let mut name = None;
    let mut file_name = None;
    for (key, value) in parse_params(params) {
        match key.as_str() {
            "name" => name = Some(value),
            "name*" => name = Some(ext_value(&value)),
            "filename" => file_name = Some(value),
            other => panic!("unexpected disposition parameter {other}"),
        }
    }
    ParsedPart {
        name: name.expect("name parameter"),
        file_name,
        content_type,
        content,
        disposition,
    }
}

/// `; key=value` parameters; a value is a token, an RFC 9110 quoted string (`\x` reads as `x`;
/// like common recipients, any character may follow the backslash), or an RFC 8187 extended
/// value kept raw.
fn parse_params(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    loop {
        while matches!(chars.peek(), Some(' ' | '\t' | ';')) {
            chars.next();
        }
        if chars.peek().is_none() {
            return out;
        }
        let mut key = String::new();
        for c in chars.by_ref() {
            if c == '=' {
                break;
            }
            key.push(c);
        }
        let mut value = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            loop {
                match chars.next().expect("unterminated quoted string") {
                    '\\' => value.push(chars.next().expect("quoted-pair")),
                    '"' => break,
                    c => value.push(c),
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c == ';' {
                    break;
                }
                value.push(c);
                chars.next();
            }
        }
        out.push((key.trim().to_ascii_lowercase(), value.trim_end().to_owned()));
    }
}

/// Decodes an RFC 8187 `ext-value`: `utf-8'<lang>'<percent-encoded>`.
fn ext_value(raw: &str) -> String {
    let mut fields = raw.splitn(3, '\'');
    let charset = fields.next().unwrap();
    assert!(charset.eq_ignore_ascii_case("utf-8"), "{raw}");
    let _language = fields.next().expect("language field");
    let encoded = fields.next().expect("value field").as_bytes();
    let mut bytes = Vec::new();
    let mut i = 0;
    while i < encoded.len() {
        if encoded[i] == b'%' {
            let hex = std::str::from_utf8(&encoded[i + 1..i + 3]).unwrap();
            bytes.push(u8::from_str_radix(hex, 16).expect("hex escape"));
            i += 3;
        } else {
            assert!(
                encoded[i].is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&encoded[i]),
                "attr-char"
            );
            bytes.push(encoded[i]);
            i += 1;
        }
    }
    String::from_utf8(bytes).expect("UTF-8 ext-value")
}
