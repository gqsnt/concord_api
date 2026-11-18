//! C12.9 and C12.10: request signing, `concord::sign` (feature `sign`).
//!
//! Signatures are checked through `SigV4` (as middleware of a
//! description and as `SigV4::sign`), `Credentials`, `CanonicalPath`, `body_bytes` and
//! `check_origin`, over a `concord_testkit::Script`, on paused `tokio` time.
//!
//! An independent computation of AWS Signature Version 4 (module `aws` below)
//! checks AWS's four published S3 examples and every request reaching the transport.
//! It uses the request's `x-amz-date`, so callers' signatures are checked at the
//! time they actually send rather than against a separate reading of the clock.

mod common;

use concord::sign::{body_bytes, check_origin, CanonicalPath, Credentials, SigV4, SignError};
use concord::transport::{TransportError, TryInfo};
use concord::{http, reqwest, Base, Body, Middleware, Next, Retry};
use concord_testkit::Script;
use std::error::Error as StdError;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

concord::api! {
    client Store(origin: Base, signer: SigV4) {
        base origin;
        middleware signer.clone();
        GET get(key: String) { path "{*key}"; }
        GET ranged(key: String, range: String) {
            path "{*key}";
            headers { "range" = range; }
        }
        PUT put(key: String, bytes: Body) { path "{*key}"; body bytes; }
        PUT put_dated(key: String, bytes: Body, date: String, class: String) {
            path "{*key}";
            headers {
                "date" = date;
                "x-amz-storage-class" = class;
            }
            body bytes;
        }
        GET root() {}
        GET list(max_keys: u32, prefix: String) {
            query { "max-keys" = max_keys; prefix; }
        }
        GET listed(key: String) -> String {
            path "{*key}";
            read concord::Text;
            paginate concord::by_link();
        }
        GET search(term: String, tag: String) {
            path "search";
            query { "q" = term; "tag" = tag; }
        }
    }
}

// The independent computation ------------------------------------------------------------

/// AWS Signature Version 4, from AWS's published rules ("Create a canonical request",
/// "Create a string to sign", "Calculate the signature"), with no code of Concord.
mod aws {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::{Digest, Sha256};

    /// The hash of an empty payload.
    pub const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    pub fn sha256_hex(data: &[u8]) -> String {
        hex::encode(Sha256::digest(data))
    }

    fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("any key length");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    /// `UriEncode`: every byte but `A-Z a-z 0-9 - . _ ~` as `%XX`, upper-case hex; `/` too.
    pub fn encode(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|&byte| {
                if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                    (byte as char).to_string()
                } else {
                    format!("%{byte:02X}")
                }
            })
            .collect()
    }

    /// Percent-decoding; a `%` that does not start an escape is kept as it is.
    pub fn decode(text: &str) -> Vec<u8> {
        let bytes = text.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let pair = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                if let Some(value) = pair.and_then(|pair| u8::from_str_radix(pair, 16).ok()) {
                    out.push(value);
                    i += 3;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        out
    }

    /// The canonical query string, as AWS's reference signer reads a query: empty fields
    /// skipped, a raw `+` is a space, each name and value decoded and encoded again, a name
    /// with no `=` gets an empty value, the pairs sorted by name then by value.
    pub fn canonical_query(query: Option<&str>) -> String {
        let Some(query) = query else {
            return String::new();
        };
        let mut pairs: Vec<(String, String)> = query
            .split('&')
            .filter(|piece| !piece.is_empty())
            .map(|piece| {
                let (name, value) = piece.split_once('=').unwrap_or((piece, ""));
                let plain = |text: &str| decode(&text.replace('+', " "));
                (encode(&plain(name)), encode(&plain(value)))
            })
            .collect();
        pairs.sort();
        pairs
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// AWS's rule for every service but S3: the path is normalized (redundant slashes and
    /// literal dot segments removed, a trailing slash kept), then each segment, as it is
    /// written in the URL, is URI-encoded once more: `%20` becomes `%2520`.
    pub fn normalized_path(path: &str) -> String {
        let mut segments: Vec<String> = Vec::new();
        for segment in path.split('/') {
            match segment {
                "" | "." => {}
                ".." => {
                    segments.pop();
                }
                _ => segments.push(encode(segment.as_bytes())),
            }
        }
        let mut out = format!("/{}", segments.join("/"));
        if path.ends_with('/') && !segments.is_empty() {
            out.push('/');
        }
        out
    }

    /// A header value: trimmed, every run of spaces and tabs one space.
    pub fn header_value(value: &[u8]) -> String {
        let text = String::from_utf8_lossy(value);
        text.split([' ', '\t'])
            .filter(|word| !word.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// What a signature is computed from.
    pub struct Input<'a> {
        pub method: &'a str,
        pub canonical_uri: &'a str,
        pub canonical_query: &'a str,
        /// `(lower-case name, raw value)`, in the order sent; only the signed ones.
        pub headers: &'a [(String, Vec<u8>)],
        pub payload_hash: &'a str,
        pub amz_date: &'a str,
        pub region: &'a str,
        pub service: &'a str,
        pub access: &'a str,
        pub secret: &'a str,
    }

    pub struct Output {
        pub canonical_request: String,
        pub signed_headers: String,
        pub signature: String,
        pub authorization: String,
    }

    pub fn sign(input: &Input<'_>) -> Output {
        let mut names: Vec<&str> = input.headers.iter().map(|(n, _)| n.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        let canonical_headers: String = names
            .iter()
            .map(|name| {
                let values: Vec<String> = input
                    .headers
                    .iter()
                    .filter(|(n, _)| n == name)
                    .map(|(_, value)| header_value(value))
                    .collect();
                format!("{name}:{}\n", values.join(","))
            })
            .collect();
        let signed_headers = names.join(";");
        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            input.method,
            input.canonical_uri,
            input.canonical_query,
            canonical_headers,
            signed_headers,
            input.payload_hash
        );
        let day = &input.amz_date[..8];
        let scope = format!("{day}/{}/{}/aws4_request", input.region, input.service);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{scope}\n{}",
            input.amz_date,
            sha256_hex(canonical_request.as_bytes())
        );
        let key = hmac(format!("AWS4{}", input.secret).as_bytes(), day.as_bytes());
        let key = hmac(&key, input.region.as_bytes());
        let key = hmac(&key, input.service.as_bytes());
        let key = hmac(&key, b"aws4_request");
        let signature = hex::encode(hmac(&key, string_to_sign.as_bytes()));
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            input.access
        );
        Output {
            canonical_request,
            signed_headers,
            signature,
            authorization,
        }
    }

    /// Seconds since the epoch of `YYYYMMDDTHHMMSSZ`.
    pub fn epoch_seconds(amz_date: &str) -> i64 {
        assert_eq!(amz_date.len(), 16, "x-amz-date {amz_date:?}");
        assert!(
            amz_date.as_bytes()[8] == b'T' && amz_date.ends_with('Z'),
            "x-amz-date {amz_date:?}"
        );
        let number = |range: std::ops::Range<usize>| -> i64 {
            amz_date[range].parse().expect("digits in x-amz-date")
        };
        let (y, m, d) = (number(0..4), number(4..6), number(6..8));
        let (hh, mm, ss) = (number(9..11), number(11..13), number(13..15));
        // Days from civil (H. Hinnant).
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        days * 86_400 + hh * 3600 + mm * 60 + ss
    }
}

// What the tests share -------------------------------------------------------------------

/// The text that every secret holds, so that a printed line can be searched for it.
const MARK: &str = "s3cr3t";
const ACCESS: &str = "AKIDEXAMPLE";
const SECRET: &str = "wJalr-s3cr3t-EXAMPLEKEY";
const TOKEN: &str = "FwoG-s3cr3t-session-token";
const BUCKET: &str = "https://examplebucket.s3.amazonaws.com";

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn base(text: &str) -> Base {
    text.parse().expect("the base parses")
}

/// Which convention of the canonical path the judge follows.
#[derive(Clone, Copy, Debug)]
enum PathRule {
    Preserve,
    Normalize,
}

/// What the judge expects a signature to have been made with.
#[derive(Clone, Debug)]
struct Who {
    access: String,
    secret: String,
    token: Option<String>,
    region: String,
    service: String,
    path: PathRule,
}

impl Who {
    fn s3() -> Who {
        Who {
            access: ACCESS.to_owned(),
            secret: SECRET.to_owned(),
            token: None,
            region: "us-east-1".to_owned(),
            service: "s3".to_owned(),
            path: PathRule::Preserve,
        }
    }

    fn iam() -> Who {
        Who {
            service: "iam".to_owned(),
            path: PathRule::Normalize,
            ..Who::s3()
        }
    }

    fn token(mut self, token: &str) -> Who {
        self.token = Some(token.to_owned());
        self
    }

    fn keys(mut self, access: &str, secret: &str) -> Who {
        self.access = access.to_owned();
        self.secret = secret.to_owned();
        self
    }

    fn region(mut self, region: &str) -> Who {
        self.region = region.to_owned();
        self
    }

    fn credentials(&self) -> Credentials {
        let credentials = Credentials::new(self.access.clone(), self.secret.clone());
        match &self.token {
            Some(token) => credentials.session_token(token.clone()),
            None => credentials,
        }
    }

    fn signer(&self, origin: &Base) -> SigV4 {
        self.signer_with(origin, self.credentials())
    }

    fn signer_with(&self, origin: &Base, credentials: Credentials) -> SigV4 {
        let path = match self.path {
            PathRule::Preserve => CanonicalPath::Preserve,
            PathRule::Normalize => CanonicalPath::NormalizeAndEncode,
        };
        SigV4::new(
            origin,
            credentials,
            self.region.clone(),
            self.service.clone(),
            path,
        )
    }
}

/// The payload that the judge hashes.
#[derive(Clone, Copy)]
enum Payload<'a> {
    Bytes(&'a [u8]),
    Unsigned,
}

/// What the judge read from a signed request.
#[derive(Debug)]
struct Verdict {
    amz_date: String,
    signature: String,
    signed_headers: String,
}

/// The fields that AWS's rules and the rustdoc leave out of a signature.
const NOT_SIGNED: &[&str] = &[
    "authorization",
    "connection",
    "expect",
    "transfer-encoding",
    "user-agent",
    "x-amzn-trace-id",
    "keep-alive",
    "proxy-connection",
    "te",
    "trailer",
    "upgrade",
    "proxy-authorization",
    "proxy-authenticate",
];

fn one<'a>(headers: &'a http::HeaderMap, name: &str) -> Option<&'a http::HeaderValue> {
    let values: Vec<_> = headers.get_all(name).iter().collect();
    assert!(values.len() <= 1, "{name} is sent {} times", values.len());
    values.first().copied()
}

fn text(value: &http::HeaderValue) -> String {
    String::from_utf8_lossy(value.as_bytes()).into_owned()
}

/// Recomputes the signature of a request as it reached the transport, and compares the
/// whole `Authorization` value with it, byte for byte.
#[track_caller]
fn judge(
    method: &http::Method,
    url: &str,
    headers: &http::HeaderMap,
    who: &Who,
    payload: Payload<'_>,
) -> Verdict {
    let url = reqwest::Url::parse(url).expect("the URL parses");
    let authority = match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().expect("a host")),
        None => url.host_str().expect("a host").to_owned(),
    };
    if let Some(host) = one(headers, "host") {
        assert_eq!(text(host), authority, "a Host header sent is the URL's");
    }
    let amz_date = text(one(headers, "x-amz-date").expect("x-amz-date is sent"));
    let _ = aws::epoch_seconds(&amz_date);

    match &who.token {
        Some(token) => {
            let value = one(headers, "x-amz-security-token").expect("the session token is sent");
            assert_eq!(text(value), *token, "the session token");
            assert!(value.is_sensitive(), "the session token is sensitive");
        }
        None => assert!(
            headers.get("x-amz-security-token").is_none(),
            "no session token is configured, and one is sent"
        ),
    }

    let payload_hash = match payload {
        Payload::Bytes(bytes) => aws::sha256_hex(bytes),
        Payload::Unsigned => "UNSIGNED-PAYLOAD".to_owned(),
    };
    if who.service == "s3" {
        let value = one(headers, "x-amz-content-sha256").expect("S3 receives x-amz-content-sha256");
        assert_eq!(text(value), payload_hash, "x-amz-content-sha256");
    } else {
        assert!(
            headers.get("x-amz-content-sha256").is_none(),
            "a service other than S3 receives no x-amz-content-sha256"
        );
    }

    let named_by_connection: Vec<String> = headers
        .get_all("connection")
        .iter()
        .flat_map(|value| {
            String::from_utf8_lossy(value.as_bytes())
                .split(',')
                .map(|option| option.trim().to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .collect();
    let mut signed: Vec<(String, Vec<u8>)> = Vec::new();
    if headers.get("host").is_none() {
        signed.push(("host".to_owned(), authority.clone().into_bytes()));
    }
    for (name, value) in headers {
        let name = name.as_str();
        if NOT_SIGNED.contains(&name) || named_by_connection.iter().any(|named| named == name) {
            continue;
        }
        signed.push((name.to_owned(), value.as_bytes().to_vec()));
    }

    let canonical_uri = match who.path {
        PathRule::Preserve => url.path().to_owned(),
        PathRule::Normalize => aws::normalized_path(url.path()),
    };
    let canonical_query = aws::canonical_query(url.query());
    let expected = aws::sign(&aws::Input {
        method: method.as_str(),
        canonical_uri: &canonical_uri,
        canonical_query: &canonical_query,
        headers: &signed,
        payload_hash: &payload_hash,
        amz_date: &amz_date,
        region: &who.region,
        service: &who.service,
        access: &who.access,
        secret: &who.secret,
    });

    let authorization = one(headers, "authorization").expect("Authorization is sent");
    assert!(authorization.is_sensitive(), "Authorization is sensitive");
    let authorization = text(authorization);
    let sent_signed = authorization
        .split("SignedHeaders=")
        .nth(1)
        .and_then(|rest| rest.split(',').next())
        .unwrap_or("");
    assert_eq!(
        sent_signed, expected.signed_headers,
        "the signed headers of {url}"
    );
    assert_eq!(
        authorization, expected.authorization,
        "the Authorization of {url}; the judge's canonical request:\n{}",
        expected.canonical_request
    );
    Verdict {
        amz_date,
        signature: expected.signature,
        signed_headers: expected.signed_headers,
    }
}

/// [`judge`] on a request as the script received it.
#[track_caller]
fn judged(request: &concord_testkit::script::Request, who: &Who, payload: Payload<'_>) -> Verdict {
    judge(
        request.method(),
        request.url().as_str(),
        request.headers(),
        who,
        payload,
    )
}

/// [`judge`] on a request signed by `SigV4::sign`.
#[track_caller]
fn judged_raw(request: &reqwest::Request, who: &Who, payload: Payload<'_>) -> Verdict {
    judge(
        request.method(),
        request.url().as_str(),
        request.headers(),
        who,
        payload,
    )
}

fn store(script: &Script, origin: &str, signer: SigV4) -> Store {
    Store::new(base(origin), signer).with_transport(script.clone())
}

#[track_caller]
fn success<T, E: 'static>(result: Result<T, concord::Error<E>>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!(
            "expected a value, got an error {:?} (class {:?}, status {:?}, signing {:?})",
            error.kind(),
            error.class(),
            error.status(),
            SignError::find(&error)
        ),
    }
}

#[track_caller]
fn failure<T, E>(result: Result<T, concord::Error<E>>) -> concord::Error<E> {
    match result {
        Ok(_) => panic!("expected an error, got a value"),
        Err(error) => error,
    }
}

/// The variant of the signing refusal in an error's chain (its `Debug` is the name).
#[track_caller]
fn refusal(error: &(dyn StdError + 'static)) -> String {
    match SignError::find(error) {
        Some(signing) => format!("{signing:?}"),
        None => panic!("no SignError in the chain of: {error}"),
    }
}

/// Everything that an error prints: `Display` and `Debug` of it and of every source.
fn printed(error: &(dyn StdError + 'static)) -> String {
    let mut out = String::new();
    let mut next = Some(error);
    let mut depth = 0;
    while let Some(error) = next {
        out.push_str(&format!("{error}\n{error:?}\n"));
        next = error.source();
        depth += 1;
        if depth > 32 {
            break;
        }
    }
    out
}

fn wall_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("after 1970")
        .as_secs() as i64
}

/// A stream that records whether it was ever polled.
struct Probe {
    polled: Arc<AtomicBool>,
    chunks: Vec<bytes::Bytes>,
}

impl futures_core::Stream for Probe {
    type Item = Result<bytes::Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polled.store(true, Ordering::SeqCst);
        if self.chunks.is_empty() {
            Poll::Ready(None)
        } else {
            Poll::Ready(Some(Ok(self.chunks.remove(0))))
        }
    }
}

fn counted_stream(chunks: &[&'static [u8]]) -> (Body, Arc<AtomicBool>) {
    let polled = Arc::new(AtomicBool::new(false));
    let stream = Probe {
        polled: polled.clone(),
        chunks: chunks
            .iter()
            .map(|c| bytes::Bytes::from_static(c))
            .collect(),
    };
    (Body::stream(stream), polled)
}

fn counted_request_stream(chunks: &[&'static [u8]]) -> (reqwest::Body, Arc<AtomicBool>) {
    let polled = Arc::new(AtomicBool::new(false));
    let stream = Probe {
        polled: polled.clone(),
        chunks: chunks
            .iter()
            .map(|c| bytes::Bytes::from_static(c))
            .collect(),
    };
    (reqwest::Body::wrap_stream(stream), polled)
}

fn raw(method: http::Method, url: &str) -> reqwest::Request {
    reqwest::Request::new(method, url.parse().expect("the URL parses"))
}

fn insert(request: &mut reqwest::Request, name: &str, value: &[u8]) {
    request.headers_mut().append(
        http::HeaderName::from_bytes(name.as_bytes()).expect("a header name"),
        http::HeaderValue::from_bytes(value).expect("a header value"),
    );
}

/// `method + full URL + every header name and value`, as the rustdoc counts a head.
fn head_size(request: &reqwest::Request) -> usize {
    request.method().as_str().len()
        + request.url().as_str().len()
        + request
            .headers()
            .iter()
            .map(|(name, value)| name.as_str().len() + value.as_bytes().len())
            .sum::<usize>()
}

// C12.9: the judge first proves itself on AWS's four published examples --------------------

#[test]
fn c12_9_the_judge_reproduces_the_four_published_aws_s3_signatures() {
    const PUT_HASH: &str = "44ce7dd67c959e0d3524ffac1771dfbba87d2b6b4b4e99e42034a8b803f8b072";
    assert_eq!(aws::sha256_hex(b""), aws::EMPTY);
    assert_eq!(aws::sha256_hex(b"Welcome to Amazon S3."), PUT_HASH);
    assert_eq!(aws::canonical_query(Some("lifecycle")), "lifecycle=");
    assert_eq!(
        aws::canonical_query(Some("prefix=J&max-keys=2")),
        "max-keys=2&prefix=J"
    );
    let h = |name: &str, value: &str| (name.to_owned(), value.as_bytes().to_vec());
    let host = "examplebucket.s3.amazonaws.com";
    let date = "20130524T000000Z";
    // (method, path, query, headers, payload hash, signed headers, hash of the canonical
    // request, signature): AWS's published values.
    let cases = [
        (
            "GET",
            "/test.txt",
            "",
            vec![
                h("host", host),
                h("range", "bytes=0-9"),
                h("x-amz-content-sha256", aws::EMPTY),
                h("x-amz-date", date),
            ],
            aws::EMPTY,
            "host;range;x-amz-content-sha256;x-amz-date",
            "7344ae5b7ee6c3e7e6b0fe0640412a37625d1fbfff95c48bbb2dc43964946972",
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41",
        ),
        (
            "PUT",
            "/test%24file.text",
            "",
            vec![
                h("date", "Fri, 24 May 2013 00:00:00 GMT"),
                h("host", host),
                h("x-amz-content-sha256", PUT_HASH),
                h("x-amz-date", date),
                h("x-amz-storage-class", "REDUCED_REDUNDANCY"),
            ],
            PUT_HASH,
            "date;host;x-amz-content-sha256;x-amz-date;x-amz-storage-class",
            "9e0e90d9c76de8fa5b200d8c849cd5b8dc7a3be3951ddb7f6a76b4158342019d",
            "98ad721746da40c64f1a55b78f14c238d841ea1380cd77a1b5971af0ece108bd",
        ),
        (
            "GET",
            "/",
            "lifecycle=",
            vec![
                h("host", host),
                h("x-amz-content-sha256", aws::EMPTY),
                h("x-amz-date", date),
            ],
            aws::EMPTY,
            "host;x-amz-content-sha256;x-amz-date",
            "9766c798316ff2757b517bc739a67f6213b4ab36dd5da2f94eaebf79c77395ca",
            "fea454ca298b7da1c68078a5d1bdbfbbe0d65c699e0f91ac7a200a0136783543",
        ),
        (
            "GET",
            "/",
            "max-keys=2&prefix=J",
            vec![
                h("host", host),
                h("x-amz-content-sha256", aws::EMPTY),
                h("x-amz-date", date),
            ],
            aws::EMPTY,
            "host;x-amz-content-sha256;x-amz-date",
            "df57d21db20da04d7fa30298dd4488ba3a2b47ca3a489c74750e0f1e7df1b9b7",
            "34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7",
        ),
    ];
    for (method, path, query, headers, payload, signed, hash, signature) in cases {
        let out = aws::sign(&aws::Input {
            method,
            canonical_uri: path,
            canonical_query: query,
            headers: &headers,
            payload_hash: payload,
            amz_date: date,
            region: "us-east-1",
            service: "s3",
            access: "AKIAIOSFODNN7EXAMPLE",
            secret: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        });
        assert_eq!(out.signed_headers, signed, "{method} {path}");
        assert_eq!(
            aws::sha256_hex(out.canonical_request.as_bytes()),
            hash,
            "{method} {path}"
        );
        assert_eq!(out.signature, signature, "{method} {path}");
        assert_eq!(
            out.authorization,
            format!(
                "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, \
                 SignedHeaders={signed}, Signature={signature}"
            )
        );
    }
}

// C12.9: what a generated client sends is signed as AWS specifies ---------------------------

#[tokio::test(start_paused = true)]
async fn c12_9_the_four_aws_example_requests_sent_through_a_client_are_signed_as_aws_specifies() {
    let who = Who::s3();
    let script = Script::new();
    for _ in 0..4 {
        script.answer(200);
    }
    let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
    success(
        client
            .ranged("test.txt".to_owned(), "bytes=0-9".to_owned())
            .await,
    );
    success(
        client
            .put_dated(
                "test$file.text".to_owned(),
                Body::new("Welcome to Amazon S3.", "text/plain"),
                "Fri, 24 May 2013 00:00:00 GMT".to_owned(),
                "REDUCED_REDUNDANCY".to_owned(),
            )
            .await,
    );
    success(
        client
            .root()
            .edit_request(|request| request.url_mut().set_query(Some("lifecycle")))
            .await,
    );
    success(client.list(2, "J".to_owned()).await);

    let requests = script.requests();
    assert_eq!(requests.len(), 4);
    let urls: Vec<&str> = requests.iter().map(|r| r.url().as_str()).collect();
    assert_eq!(
        urls,
        [
            "https://examplebucket.s3.amazonaws.com/test.txt",
            "https://examplebucket.s3.amazonaws.com/test%24file.text",
            "https://examplebucket.s3.amazonaws.com/?lifecycle",
            "https://examplebucket.s3.amazonaws.com/?max-keys=2&prefix=J",
        ]
    );
    let get = judged(&requests[0], &who, Payload::Bytes(b""));
    assert!(get.signed_headers.contains("range"), "{get:?}");
    let put = judged(&requests[1], &who, Payload::Bytes(b"Welcome to Amazon S3."));
    assert_eq!(
        requests[1].header("x-amz-content-sha256"),
        Some("44ce7dd67c959e0d3524ffac1771dfbba87d2b6b4b4e99e42034a8b803f8b072")
    );
    for name in ["date", "x-amz-storage-class", "x-amz-date", "host"] {
        assert!(put.signed_headers.split(';').any(|n| n == name), "{name}");
    }
    judged(&requests[2], &who, Payload::Bytes(b""));
    judged(&requests[3], &who, Payload::Bytes(b""));
}

#[tokio::test(start_paused = true)]
async fn c12_9_a_query_with_repeated_empty_escaped_and_unordered_keys_is_signed_canonically() {
    let who = Who::s3();
    let script = Script::new();
    let raw_queries = [
        "b=2&a=&B=1&k=z&k=a&k=a&flag&sp=a%20b&sl=%2F&t=~&%2Akey=%2A&Z=%C3%A9",
        "x-id=PutObject&uploads",
        "k=2&k=10&k=1",
    ];
    for _ in 0..raw_queries.len() + 1 {
        script.answer(200);
    }
    let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
    for query in raw_queries {
        success(
            client
                .root()
                .edit_request(move |request| request.url_mut().set_query(Some(query)))
                .await,
        );
    }
    // Values that the description itself must escape.
    success(
        client
            .search("a b/c+d=e&f~é".to_owned(), "x*y'(z)!".to_owned())
            .await,
    );

    let requests = script.requests();
    assert_eq!(requests.len(), 4);
    for (request, query) in requests.iter().zip(raw_queries) {
        assert!(request.url().as_str().ends_with(query), "{}", request.url());
        judged(request, &who, Payload::Bytes(b""));
    }
    judged(&requests[3], &who, Payload::Bytes(b""));
}

#[tokio::test(start_paused = true)]
async fn c12_9_a_raw_plus_empty_fields_lower_case_escapes_and_reserved_characters_are_canonical() {
    // The independent query encoder treats raw plus as space and skips empty fields.
    assert_eq!(
        aws::canonical_query(Some("a=b+c&&d=%2B&e=%2f&f=a*b!&")),
        "a=b%20c&d=%2B&e=%2F&f=a%2Ab%21"
    );
    let who = Who::s3();
    let raw_queries = [
        "a=b+c&&d=%2B&&",
        "e=%2f&f=a*b!(x)&g=x:y@z,$;/?&h",
        "%7e=lower%7e&k=%e2%82%ac&K=%E2%82%AC",
        "+lead=+&sp+ace=1&sp%20ace=0",
        "&&&only=1",
    ];
    let script = Script::new();
    for _ in 0..raw_queries.len() {
        script.answer(200);
    }
    let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
    for query in raw_queries {
        success(
            client
                .root()
                .edit_request(move |request| request.url_mut().set_query(Some(query)))
                .await,
        );
    }
    let requests = script.requests();
    assert_eq!(requests.len(), raw_queries.len());
    for (request, query) in requests.iter().zip(raw_queries) {
        // The URL is sent as it was written; only the canonical form changes.
        assert_eq!(request.url().as_str(), format!("{BUCKET}/?{query}"));
        judged(request, &who, Payload::Bytes(b""));
    }
}

#[tokio::test(start_paused = true)]
async fn c12_9_headers_with_several_values_extra_spaces_and_upper_case_names_are_signed_canonically(
) {
    let who = Who::s3();
    let script = Script::new();
    script.answer(200);
    let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
    success(
        client
            .get("k".to_owned())
            .edit_request(|request| {
                insert(request, "X-Amz-Meta-Upper", b"  Mixed   Case\t\tvalue  ");
                insert(request, "x-dup", b"a");
                insert(request, "x-dup", b"  b   c ");
                insert(request, "x-dup", b"");
                insert(request, "x-empty", b"");
                insert(request, "x-tabs", b"\tone\t \ttwo\t");
            })
            .await,
    );
    let request = &script.requests()[0];
    let verdict = judged(request, &who, Payload::Bytes(b""));
    for name in ["x-amz-meta-upper", "x-dup", "x-empty", "x-tabs"] {
        assert!(
            verdict.signed_headers.split(';').any(|n| n == name),
            "{name} in {}",
            verdict.signed_headers
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c12_9_the_headers_that_a_transport_may_change_are_left_out() {
    let who = Who::s3();
    let script = Script::new();
    script.answer(200);
    let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
    success(
        client
            .get("k".to_owned())
            .edit_request(|request| {
                insert(request, "user-agent", b"agent/1");
                insert(request, "expect", b"100-continue");
                insert(request, "x-amzn-trace-id", b"Root=1-abc");
                insert(request, "keep-alive", b"timeout=5");
                insert(request, "proxy-connection", b"keep-alive");
                insert(request, "te", b"trailers");
                insert(request, "trailer", b"x-check");
                insert(request, "upgrade", b"h2c");
                insert(request, "proxy-authorization", b"Basic eA==");
                insert(request, "proxy-authenticate", b"Basic");
                insert(request, "connection", b"X-Hop, close");
                insert(request, "x-hop", b"hop");
                insert(request, "x-kept", b"kept");
            })
            .await,
    );
    let request = &script.requests()[0];
    let verdict = judged(request, &who, Payload::Bytes(b""));
    assert_eq!(
        verdict.signed_headers,
        "host;x-amz-content-sha256;x-amz-date;x-kept"
    );
    // Excluded headers are still sent as the caller gave them.
    for (name, value) in [
        ("user-agent", "agent/1"),
        ("expect", "100-continue"),
        ("x-amzn-trace-id", "Root=1-abc"),
        ("keep-alive", "timeout=5"),
        ("proxy-connection", "keep-alive"),
        ("te", "trailers"),
        ("trailer", "x-check"),
        ("upgrade", "h2c"),
        ("proxy-authorization", "Basic eA=="),
        ("proxy-authenticate", "Basic"),
        ("connection", "X-Hop, close"),
        ("x-hop", "hop"),
    ] {
        let values: Vec<_> = request.headers().get_all(name).iter().collect();
        assert_eq!(values.len(), 1, "{name}");
        assert_eq!(values[0].as_bytes(), value.as_bytes(), "{name}");
    }
}

#[tokio::test(start_paused = true)]
async fn c12_9_a_connection_option_naming_host_or_an_amz_field_is_refused() {
    let who = Who::s3();
    for named in [
        &b"host"[..],
        b"Host",
        b"x-amz-date",
        b"keep-alive, X-Amz-Content-Sha256",
        b"x-amz-meta-anything",
        b"Authorization",
        b"close, authorization",
    ] {
        let script = Script::new();
        script.answer(200);
        let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
        let error = failure(
            client
                .get("k".to_owned())
                .edit_request(move |request| {
                    insert(request, "x-amz-meta-anything", b"v");
                    insert(request, "connection", named);
                })
                .await,
        );
        assert_eq!(
            refusal(&error),
            "InvalidRequest",
            "{}",
            String::from_utf8_lossy(named)
        );
        assert!(script.requests().is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn c12_9_a_header_value_that_is_not_text_is_refused_before_anything_is_sent() {
    let who = Who::s3();
    let script = Script::new();
    script.answer(200);
    let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
    let error = failure(
        client
            .get("k".to_owned())
            .edit_request(|request| insert(request, "x-amz-meta-name", b"caf\xe9-s3cr3t"))
            .await,
    );
    assert_eq!(refusal(&error), "InvalidHeaderText(..)");
    assert!(script.requests().is_empty());
    assert!(!printed(&error).contains(MARK));
}

#[tokio::test(start_paused = true)]
async fn c12_9_preserve_signs_an_aws_encoded_path_as_it_is_sent() {
    let who = Who::s3();
    let keys = [
        (
            "photos/2024/a b+c$d~é.txt",
            "/photos/2024/a%20b%2Bc%24d~%C3%A9.txt",
        ),
        ("folder/", "/folder/"),
        ("a//b", "/a//b"),
        ("x%2Fy", "/x%252Fy"),
        ("UPPER/lower-._~", "/UPPER/lower-._~"),
    ];
    let script = Script::new();
    for _ in 0..keys.len() + 1 {
        script.answer(200);
    }
    let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
    for (key, _) in keys {
        success(client.get(key.to_owned()).await);
    }
    // An upper-case escape written by the caller is kept and signed as it is.
    success(
        client
            .root()
            .edit_request(|request| request.url_mut().set_path("/a%2Fb%E2%82%AC"))
            .await,
    );
    let requests = script.requests();
    for (request, (_, path)) in requests.iter().zip(keys) {
        assert_eq!(request.url().as_str(), format!("{BUCKET}{path}"));
        judged(request, &who, Payload::Bytes(b""));
    }
    assert_eq!(
        requests[keys.len()].url().as_str(),
        format!("{BUCKET}/a%2Fb%E2%82%AC")
    );
    judged(&requests[keys.len()], &who, Payload::Bytes(b""));
}

#[tokio::test(start_paused = true)]
async fn c12_9_preserve_refuses_a_path_that_is_not_aws_encoded() {
    let who = Who::s3();
    for path in [
        "/a%2fb",
        "/a!b",
        "/a*b",
        "/a'b",
        "/a(b)",
        "/a,b",
        "/a;b",
        "/a:b",
        "/a@b",
        "/%e2%82%ac",
    ] {
        let script = Script::new();
        script.answer(200);
        let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
        let error = failure(
            client
                .root()
                .edit_request(move |request| request.url_mut().set_path(path))
                .await,
        );
        assert_eq!(refusal(&error), "InvalidRequest", "{path}");
        assert!(script.requests().is_empty(), "{path}");
    }
}

#[tokio::test(start_paused = true)]
async fn c12_9_normalize_and_encode_encodes_the_path_once_more_as_aws_does_for_other_services() {
    // The judge on the example of the rustdoc, and on AWS's rule itself.
    assert_eq!(aws::normalized_path("/a%20b"), "/a%2520b");
    let who = Who::iam();
    let origin = "https://iam.amazonaws.com";
    let paths = [
        ("/a//b", "/a/b"),
        ("/a%20b/c%2fd", "/a%2520b/c%252fd"),
        ("/%7Euser/x!y*z", "/%257Euser/x%21y%2Az"),
        ("/caf%C3%A9/", "/caf%25C3%25A9/"),
        ("/docs/", "/docs/"),
        ("/", "/"),
        // The URL parser reads `%2E%2E` as `..` before anything signs (the rustdoc: "the
        // URL parser may already have removed dot segments"); three escaped dots are a name.
        ("/a/%2E%2E%2E/b", "/a/%252E%252E%252E/b"),
        ("/a/.x/b", "/a/.x/b"),
        ("/a%252F", "/a%25252F"),
        ("/UPPER/lower-._~", "/UPPER/lower-._~"),
    ];
    let script = Script::new();
    for _ in 0..paths.len() {
        script.answer(200);
    }
    let client = store(&script, origin, who.signer(&base(origin)));
    for (path, _) in paths {
        success(
            client
                .root()
                .edit_request(move |request| request.url_mut().set_path(path))
                .await,
        );
    }
    let requests = script.requests();
    assert_eq!(requests.len(), paths.len());
    for (request, (path, canonical)) in requests.iter().zip(paths) {
        // The URL is not rewritten.
        assert_eq!(request.url().as_str(), format!("{origin}{path}"));
        assert_eq!(aws::normalized_path(path), canonical, "the judge on {path}");
        judged(request, &who, Payload::Bytes(b""));
    }
}

#[tokio::test(start_paused = true)]
async fn c12_9_one_path_is_signed_as_it_is_for_s3_and_encoded_again_for_other_services() {
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    let s3 = Who::s3();
    let iam = Who::s3().region("us-east-1");
    let iam = Who {
        service: "iam".to_owned(),
        path: PathRule::Normalize,
        ..iam
    };
    for who in [&s3, &iam] {
        let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
        success(client.get("a b".to_owned()).await);
    }
    let requests = script.requests();
    for request in &requests {
        assert_eq!(request.url().as_str(), format!("{BUCKET}/a%20b"));
    }
    judged(&requests[0], &s3, Payload::Bytes(b""));
    judged(&requests[1], &iam, Payload::Bytes(b""));
}

#[tokio::test(start_paused = true)]
async fn c12_9_a_session_token_is_sent_signed_and_sensitive() {
    let who = Who::s3().token(TOKEN);
    let script = Script::new();
    script.answer(200);
    let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
    success(client.get("k".to_owned()).await);
    let verdict = judged(&script.requests()[0], &who, Payload::Bytes(b""));
    assert!(
        verdict
            .signed_headers
            .split(';')
            .any(|n| n == "x-amz-security-token"),
        "{}",
        verdict.signed_headers
    );
}

#[tokio::test(start_paused = true)]
async fn c12_9_bodies_of_bytes_are_hashed_whole_at_every_size() {
    let not_text: &[u8] = b"\x00\xff\xfe\r\n\x80binary";
    let sizes = [0usize, 1, 65_535, 65_536, 65_537, 3 * 65_536 + 1];
    let mut bodies: Vec<Vec<u8>> = vec![not_text.to_vec()];
    for size in sizes {
        bodies.push((0..size).map(|i| (i % 251) as u8).collect());
    }
    for who in [Who::s3(), Who::iam()] {
        let origin = if who.service == "s3" {
            BUCKET
        } else {
            "https://iam.amazonaws.com"
        };
        let script = Script::new();
        for _ in 0..bodies.len() + 1 {
            script.answer(200);
        }
        let client = store(&script, origin, who.signer(&base(origin)));
        for body in &bodies {
            let body = Body::new(body.clone(), "application/octet-stream");
            success(client.put("object".to_owned(), body).await);
        }
        // No body at all.
        success(client.get("object".to_owned()).await);
        let requests = script.requests();
        assert_eq!(requests.len(), bodies.len() + 1);
        for (request, body) in requests.iter().zip(&bodies) {
            assert_eq!(request.body(), Some(&body[..]));
            judged(request, &who, Payload::Bytes(body));
        }
        judged(&requests[bodies.len()], &who, Payload::Bytes(b""));
    }
}

#[tokio::test(start_paused = true)]
async fn c12_9_an_unsigned_stream_over_https_is_sent_with_unsigned_payload() {
    let who = Who::s3();
    let script = Script::new();
    script.answer(200).read_upload();
    let signer = who.signer(&base(BUCKET)).allow_unsigned_payload();
    let client = store(&script, BUCKET, signer);
    let (body, polled) = counted_stream(&[b"first ", b"second"]);
    success(client.put("stream".to_owned(), body).await);
    assert!(polled.load(Ordering::SeqCst));
    let request = &script.requests()[0];
    assert_eq!(request.upload().bytes(), b"first second");
    judged(request, &who, Payload::Unsigned);
}

#[tokio::test(start_paused = true)]
async fn c12_9_a_buffered_body_is_still_hashed_when_unsigned_payload_is_allowed() {
    let who = Who::s3();
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    let signer = who.signer(&base(BUCKET)).allow_unsigned_payload();
    let client = store(&script, BUCKET, signer);
    success(
        client
            .put("k".to_owned(), Body::new("payload", "text/plain"))
            .await,
    );
    success(
        client
            .put("k".to_owned(), Body::new("", "text/plain"))
            .await,
    );
    let requests = script.requests();
    judged(&requests[0], &who, Payload::Bytes(b"payload"));
    judged(&requests[1], &who, Payload::Bytes(b""));
}

#[tokio::test(start_paused = true)]
async fn c12_9_each_attempt_is_signed_anew_with_its_own_time() {
    let who = Who::s3().token(TOKEN);
    let script = Script::new();
    script.answer(503).after(secs(10));
    script.answer(503).after(secs(7));
    script.answer(200);
    let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
    success(
        client
            .put("k".to_owned(), Body::new("same body", "text/plain"))
            .with_retry(Retry::standard())
            .await,
    );
    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    let verdicts: Vec<Verdict> = requests
        .iter()
        .map(|r| judged(r, &who, Payload::Bytes(b"same body")))
        .collect();
    for pair in [(0, 1), (1, 2)] {
        let (a, b) = (&verdicts[pair.0], &verdicts[pair.1]);
        assert_ne!(a.signature, b.signature, "attempts {pair:?}");
        let virtual_gap = requests[pair.1].at() - requests[pair.0].at();
        let gap = aws::epoch_seconds(&b.amz_date) - aws::epoch_seconds(&a.amz_date);
        let floor = virtual_gap.as_secs() as i64;
        assert!(
            gap == floor || gap == floor + 1,
            "attempts {pair:?}: x-amz-date moved {gap} s for {virtual_gap:?} of time"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c12_9_the_signing_time_starts_at_the_wall_clock_and_follows_tokio_time() {
    let who = Who::s3();
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    script.answer(200);
    let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
    let before = wall_now();
    success(client.get("k".to_owned()).await);
    let after = wall_now();
    tokio::time::advance(secs(3600)).await;
    success(client.get("k".to_owned()).await);
    tokio::time::advance(secs(86_400 * 400)).await;
    success(client.get("k".to_owned()).await);
    let requests = script.requests();
    let dates: Vec<i64> = requests
        .iter()
        .map(|r| aws::epoch_seconds(&judged(r, &who, Payload::Bytes(b"")).amz_date))
        .collect();
    assert!(
        before - 1 <= dates[0] && dates[0] <= after + 1,
        "{} not within [{before}, {after}]",
        dates[0]
    );
    assert_eq!(dates[1] - dates[0], 3600);
    assert_eq!(dates[2] - dates[1], 86_400 * 400);
}

#[tokio::test(start_paused = true)]
async fn c12_9_replaced_credentials_reach_every_clone_and_are_used_whole() {
    let a = Who::s3()
        .keys("AKIDFIRST", "first-s3cr3t")
        .token("first-token");
    let b = Who::s3().keys("AKIDSECOND", "second-s3cr3t");
    let c = Who::s3()
        .keys("AKIDTHIRD", "third-s3cr3t")
        .token("third-token");
    let credentials = a.credentials();
    let script = Script::new();
    for _ in 0..6 {
        script.answer(200);
    }
    // Two signers, two clients: the second caller holds another clone.
    let one = store(
        &script,
        BUCKET,
        a.signer_with(&base(BUCKET), credentials.clone()),
    );
    let two = store(
        &script,
        BUCKET,
        a.signer_with(&base(BUCKET), credentials.clone()),
    );
    let keeper = credentials.clone();
    drop(credentials);

    success(one.get("k".to_owned()).await);
    success(two.get("k".to_owned()).await);
    keeper.replace(b.credentials());
    success(one.get("k".to_owned()).await);
    success(two.get("k".to_owned()).await);
    keeper.replace(c.credentials());
    success(two.get("k".to_owned()).await);
    success(one.get("k".to_owned()).await);

    let requests = script.requests();
    assert_eq!(requests.len(), 6);
    for (request, who) in requests.iter().zip([&a, &a, &b, &b, &c, &c]) {
        judged(request, who, Payload::Bytes(b""));
    }
}

/// A caller's middleware that replaces the credentials just before attempt `at`.
#[derive(Debug)]
struct ReplaceBefore {
    at: u32,
    credentials: Credentials,
    next: Mutex<Option<Credentials>>,
}

impl Middleware for ReplaceBefore {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        if info.number() == self.at {
            if let Some(replacement) = self.next.lock().expect("not poisoned").take() {
                self.credentials.replace(replacement);
            }
        }
        next.send(request, info).await
    }
}

#[tokio::test(start_paused = true)]
async fn c12_9_credentials_replaced_between_two_attempts_sign_the_second_whole() {
    let a = Who::s3()
        .keys("AKIDFIRST", "first-s3cr3t")
        .token("first-token");
    let b = Who::s3().keys("AKIDSECOND", "second-s3cr3t");
    let credentials = a.credentials();
    let script = Script::new();
    script.answer(503);
    script.answer(200);
    let client = Store::new(
        base(BUCKET),
        a.signer_with(&base(BUCKET), credentials.clone()),
    )
    .with_middleware(ReplaceBefore {
        at: 2,
        credentials,
        next: Mutex::new(Some(b.credentials())),
    })
    .with_transport(script.clone());
    success(
        client
            .get("k".to_owned())
            .with_retry(Retry::standard())
            .await,
    );
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    judged(&requests[0], &a, Payload::Bytes(b""));
    judged(&requests[1], &b, Payload::Bytes(b""));
}

#[tokio::test(start_paused = true)]
async fn c12_9_stale_signing_fields_are_replaced_and_a_stale_token_is_removed() {
    let who = Who::s3();
    let script = Script::new();
    script.answer(200);
    let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
    success(
        client
            .put("k".to_owned(), Body::new("fresh", "text/plain"))
            .edit_request(|request| {
                insert(request, "authorization", b"AWS4-HMAC-SHA256 stale");
                insert(request, "authorization", b"second stale");
                insert(request, "x-amz-date", b"20000101T000000Z");
                insert(request, "x-amz-content-sha256", b"UNSIGNED-PAYLOAD");
                insert(request, "x-amz-security-token", b"stale-token");
                insert(request, "x-amz-security-token", b"stale-token-2");
            })
            .await,
    );
    judged(&script.requests()[0], &who, Payload::Bytes(b"fresh"));
}

#[tokio::test(start_paused = true)]
async fn c12_9_a_conflicting_host_is_refused_and_a_port_is_signed_in_host() {
    let who = Who::s3();
    let script = Script::new();
    script.answer(200);
    let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
    let error = failure(
        client
            .get("k".to_owned())
            .edit_request(|request| insert(request, "host", b"other.example.test"))
            .await,
    );
    assert_eq!(refusal(&error), "InvalidRequest");
    assert!(script.requests().is_empty());

    // A Host equal to the URL's is accepted and signed.
    script.answer(200);
    success(
        client
            .get("k".to_owned())
            .edit_request(|request| insert(request, "host", b"examplebucket.s3.amazonaws.com"))
            .await,
    );
    let verdict = judged(&script.requests()[0], &who, Payload::Bytes(b""));
    assert_eq!(
        verdict.signed_headers,
        "host;x-amz-content-sha256;x-amz-date"
    );

    let origin = "https://store.example.test:8443";
    script.answer(200);
    script.answer(200);
    let client = store(&script, origin, who.signer(&base(origin)));
    success(client.get("k".to_owned()).await);
    success(
        client
            .get("k".to_owned())
            .edit_request(|request| insert(request, "host", b"store.example.test:8443"))
            .await,
    );
    let error = failure(
        client
            .get("k".to_owned())
            .edit_request(|request| insert(request, "host", b"store.example.test"))
            .await,
    );
    assert_eq!(refusal(&error), "InvalidRequest", "a Host without its port");
    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[1].url().as_str(),
        "https://store.example.test:8443/k"
    );
    judged(&requests[1], &who, Payload::Bytes(b""));
    judged(&requests[2], &who, Payload::Bytes(b""));
}

#[tokio::test(start_paused = true)]
async fn c12_9_sign_signs_a_request_for_another_region_and_host() {
    // The signer's own origin and region are another host and `us-east-1`.
    let configured = Who::s3();
    let signer = configured.signer(&base(BUCKET));
    let who = Who::s3().region("eu-west-1");
    let url = "https://photos.s3.eu-west-1.amazonaws.com/reports/nov%20ember.txt?versionId=3";
    let mut request = raw(http::Method::PUT, url);
    *request.body_mut() = Some(reqwest::Body::from("hello"));
    insert(&mut request, "content-type", b"text/plain");
    signer
        .sign(&mut request, "eu-west-1")
        .expect("sign for another region and host");
    let first = judged_raw(&request, &who, Payload::Bytes(b"hello"));
    // `sign` accepts this host; `check_origin` rejects it against the configured base.
    assert_eq!(
        format!(
            "{:?}",
            check_origin(&request, &base(BUCKET)).expect_err("another host")
        ),
        "WrongOrigin"
    );

    // The same request signed twice: the fields of the first signature are replaced, not
    // added to; the time has moved.
    tokio::time::advance(secs(5)).await;
    signer.sign(&mut request, "eu-west-1").expect("sign again");
    let second = judged_raw(&request, &who, Payload::Bytes(b"hello"));
    assert_eq!(
        aws::epoch_seconds(&second.amz_date) - aws::epoch_seconds(&first.amz_date),
        5
    );
    assert_ne!(first.signature, second.signature);
    // The body is untouched.
    assert_eq!(body_bytes(&request).expect("bytes"), b"hello");
}

#[tokio::test(start_paused = true)]
async fn c12_9_sign_changes_nothing_when_it_refuses() {
    let signer = Who::s3().signer(&base(BUCKET));
    let mut request = raw(
        http::Method::PUT,
        "https://examplebucket.s3.amazonaws.com/k",
    );
    insert(&mut request, "authorization", b"stale");
    insert(&mut request, "x-amz-date", b"20000101T000000Z");
    let (body, polled) = counted_request_stream(&[b"stream"]);
    *request.body_mut() = Some(body);
    let before = request.headers().clone();
    let error = signer
        .sign(&mut request, "us-east-1")
        .expect_err("a stream is refused");
    assert_eq!(format!("{error:?}"), "StreamingBody");
    assert_eq!(*request.headers(), before);
    assert!(!polled.load(Ordering::SeqCst));

    // A head over its bound: refused, nothing changed.
    let mut request = raw(
        http::Method::GET,
        "https://examplebucket.s3.amazonaws.com/k",
    );
    insert(&mut request, "x-pad", "p".repeat(70_000).as_bytes());
    let before = request.headers().clone();
    let error = signer
        .sign(&mut request, "us-east-1")
        .expect_err("too large");
    assert_eq!(format!("{error:?}"), "TooLarge");
    assert_eq!(*request.headers(), before);
}

#[tokio::test(start_paused = true)]
async fn c12_9_a_head_of_64_kib_after_signing_is_signed_and_one_byte_more_is_refused() {
    let who = Who::iam();
    let url = "https://iam.amazonaws.com/";
    let signer = who.signer(&base(url));
    let padded = |length: usize| {
        let mut request = raw(http::Method::GET, url);
        insert(&mut request, "x-pad", "p".repeat(length).as_bytes());
        request
    };
    // How many bytes signing adds, with a short pad: the same names and lengths at any size.
    let mut sample = padded(1);
    let small = head_size(&sample);
    signer.sign(&mut sample, "us-east-1").expect("a small head");
    let added = head_size(&sample) - small;
    let length = 65_536 - added - (small - 1);

    let mut at_bound = padded(length);
    signer
        .sign(&mut at_bound, "us-east-1")
        .expect("a head of exactly 64 KiB after signing");
    assert_eq!(head_size(&at_bound), 65_536);
    judged_raw(&at_bound, &who, Payload::Bytes(b""));

    let mut over = padded(length + 1);
    let before = over.headers().clone();
    let error = signer
        .sign(&mut over, "us-east-1")
        .expect_err("one byte more");
    assert_eq!(format!("{error:?}"), "TooLarge");
    assert_eq!(*over.headers(), before);
}

#[tokio::test(start_paused = true)]
async fn c12_9_credential_fields_of_16_kib_are_used_and_one_byte_more_is_refused() {
    let url = "https://examplebucket.s3.amazonaws.com/k";
    let at_bound = "A".repeat(16_384);
    let over = "A".repeat(16_385);
    let cases = [
        (Who::s3().keys(&at_bound, SECRET), true),
        (Who::s3().keys(ACCESS, &at_bound), true),
        (Who::s3().token(&at_bound), true),
        (Who::s3().keys(&over, SECRET), false),
        (Who::s3().keys(ACCESS, &over), false),
        (Who::s3().token(&over), false),
    ];
    for (index, (who, accepted)) in cases.iter().enumerate() {
        let signer = who.signer(&base(BUCKET));
        let mut request = raw(http::Method::GET, url);
        let before = request.headers().clone();
        let result = signer.sign(&mut request, "us-east-1");
        if *accepted {
            result.unwrap_or_else(|e| panic!("case {index}: {e:?}"));
            judged_raw(&request, who, Payload::Bytes(b""));
        } else {
            let error = result.expect_err("one byte over");
            assert_eq!(format!("{error:?}"), "TooLarge", "case {index}");
            assert_eq!(*request.headers(), before, "case {index}");
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c12_9_a_region_and_a_service_of_256_bytes_are_used_and_257_are_refused() {
    let url = "https://examplebucket.s3.amazonaws.com/k";
    let long = "a".repeat(256);
    let longer = "a".repeat(257);
    // Through `SigV4::sign`'s own region.
    let signer = Who::s3().signer(&base(BUCKET));
    let mut request = raw(http::Method::GET, url);
    signer
        .sign(&mut request, &long)
        .expect("a region of 256 bytes");
    judged_raw(&request, &Who::s3().region(&long), Payload::Bytes(b""));
    let mut request = raw(http::Method::GET, url);
    let error = signer
        .sign(&mut request, &longer)
        .expect_err("a region of 257 bytes");
    assert_eq!(format!("{error:?}"), "TooLarge");
    assert!(request.headers().is_empty());

    // A service of 256 bytes, and 257.
    let who = Who {
        service: long.clone(),
        ..Who::s3()
    };
    let mut request = raw(http::Method::GET, url);
    who.signer(&base(BUCKET))
        .sign(&mut request, "us-east-1")
        .expect("a service of 256 bytes");
    judged_raw(&request, &who, Payload::Bytes(b""));
    let who = Who {
        service: longer,
        ..Who::s3()
    };
    let mut request = raw(http::Method::GET, url);
    let error = who
        .signer(&base(BUCKET))
        .sign(&mut request, "us-east-1")
        .expect_err("a service of 257 bytes");
    assert_eq!(format!("{error:?}"), "TooLarge");
    assert!(request.headers().is_empty());
}

// C12.10: never what it cannot see, never for the wrong host --------------------------------

#[tokio::test(start_paused = true)]
async fn c12_10_a_stream_is_refused_without_being_polled() {
    let http = "http://examplebucket.s3.amazonaws.com";
    for (who, origin) in [
        (Who::s3(), BUCKET),
        (Who::iam(), BUCKET),
        (Who::s3(), http),
        (Who::iam(), http),
    ] {
        let script = Script::new();
        script.answer(200);
        let client = store(&script, origin, who.signer(&base(origin)));
        let (body, polled) = counted_stream(&[b"never read"]);
        let error = failure(
            client
                .put("k".to_owned(), body)
                .with_retry(Retry::standard())
                .await,
        );
        assert_eq!(refusal(&error), "StreamingBody");
        assert!(!polled.load(Ordering::SeqCst), "the stream was polled");
        assert!(script.requests().is_empty());
        assert_eq!(script.unused(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn c12_10_an_unsigned_stream_over_http_is_refused_without_being_polled() {
    let who = Who::s3();
    let origin = "http://examplebucket.s3.amazonaws.com";
    let script = Script::new();
    script.answer(200);
    let signer = who.signer(&base(origin)).allow_unsigned_payload();
    let client = store(&script, origin, signer);
    let (body, polled) = counted_stream(&[b"never read"]);
    let error = failure(client.put("k".to_owned(), body).await);
    assert_eq!(refusal(&error), "InsecureUnsignedPayload");
    assert!(!polled.load(Ordering::SeqCst), "the stream was polled");
    assert!(script.requests().is_empty());

    // The same signer over HTTP still signs a body of bytes.
    success(
        client
            .put("k".to_owned(), Body::new("bytes", "text/plain"))
            .await,
    );
    judged(&script.requests()[0], &who, Payload::Bytes(b"bytes"));
}

#[tokio::test(start_paused = true)]
async fn c12_10_a_request_to_another_origin_is_refused_before_anything_is_sent() {
    let who = Who::s3();
    let others = [
        "https://other.s3.amazonaws.com",
        "http://examplebucket.s3.amazonaws.com",
        "https://examplebucket.s3.amazonaws.com:8443",
        "https://examplebucket.s3.amazonaws.com.evil.test",
        "https://s3.amazonaws.com",
    ];
    for other in others {
        let script = Script::new();
        script.answer(200);
        let client = Store::new(base(BUCKET), who.signer(&base(BUCKET)))
            .with_base(base(other))
            .with_transport(script.clone());
        let error = failure(
            client
                .get("k".to_owned())
                .with_retry(Retry::standard())
                .await,
        );
        assert_eq!(refusal(&error), "WrongOrigin", "{other}");
        assert!(script.requests().is_empty(), "{other}");
    }
    // The same origin, written with its default port or another path, is signed.
    for same in [
        "https://examplebucket.s3.amazonaws.com:443",
        "https://examplebucket.s3.amazonaws.com/prefix/",
        "https://EXAMPLEBUCKET.s3.amazonaws.com",
    ] {
        let script = Script::new();
        script.answer(200);
        let client = Store::new(base(BUCKET), who.signer(&base(BUCKET)))
            .with_base(base(same))
            .with_transport(script.clone());
        success(client.get("k".to_owned()).await);
        judged(&script.requests()[0], &who, Payload::Bytes(b""));
    }
}

#[tokio::test(start_paused = true)]
async fn c12_10_the_origin_is_checked_before_a_key_is_used() {
    // Credentials that cannot sign: their refusal shows when the origin is right.
    let bad = [
        Who::s3().keys("AKID/BAD", SECRET),
        Who::s3().keys(ACCESS, &"A".repeat(16_385)),
    ];
    for who in &bad {
        let script = Script::new();
        let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
        let error = failure(client.get("k".to_owned()).await);
        let at_origin = refusal(&error);
        let expected = if who.access.contains('/') {
            "InvalidCredentials"
        } else {
            "TooLarge"
        };
        assert_eq!(at_origin, expected);
        let client = Store::new(base(BUCKET), who.signer(&base(BUCKET)))
            .with_base(base("https://other.example.test"))
            .with_transport(script.clone());
        let error = failure(client.get("k".to_owned()).await);
        assert_eq!(refusal(&error), "WrongOrigin");
        assert!(script.requests().is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn c12_9_invalid_scope_is_what_the_rustdoc_names_and_nothing_more() {
    let url = "https://examplebucket.s3.amazonaws.com/k";
    let with = |access: &str, secret: &str, token: Option<&str>, region: &str, service: &str| Who {
        access: access.to_owned(),
        secret: secret.to_owned(),
        token: token.map(str::to_owned),
        region: region.to_owned(),
        service: service.to_owned(),
        path: PathRule::Preserve,
    };
    let refused = [
        with("", SECRET, None, "us-east-1", "s3"),
        with("AKID.X", SECRET, None, "us-east-1", "s3"),
        with("AKID/X", SECRET, None, "us-east-1", "s3"),
        with("AKID X", SECRET, None, "us-east-1", "s3"),
        with("AKIDé", SECRET, None, "us-east-1", "s3"),
        with(ACCESS, "", None, "us-east-1", "s3"),
        with(ACCESS, SECRET, Some(""), "us-east-1", "s3"),
        with(ACCESS, SECRET, None, "", "s3"),
        with(ACCESS, SECRET, None, "us east-1", "s3"),
        with(ACCESS, SECRET, None, "us-east-1/x", "s3"),
        with(ACCESS, SECRET, None, "us-east-1", ""),
        with(ACCESS, SECRET, None, "us-east-1", "s3.x"),
    ];
    for (index, who) in refused.iter().enumerate() {
        let mut request = raw(http::Method::GET, url);
        let error = who
            .signer(&base(BUCKET))
            .sign(&mut request, &who.region)
            .expect_err("invalid scope");
        assert_eq!(format!("{error:?}"), "InvalidCredentials", "case {index}");
        assert!(request.headers().is_empty(), "case {index}");
    }
    // What the rustdoc allows: letters of either case, digits, hyphens and underscores;
    // any non-empty secret.
    let accepted = [
        with("AKID_x-9", "any / secret + = .", None, "US-East_1", "s3"),
        with(
            ACCESS,
            SECRET,
            Some("token with / + = and spaces"),
            "eu-west-1",
            "s3",
        ),
    ];
    for who in accepted {
        let mut request = raw(http::Method::GET, url);
        who.signer(&base(BUCKET))
            .sign(&mut request, &who.region)
            .unwrap_or_else(|e| panic!("{who:?}: {e:?}"));
        judged_raw(&request, &who, Payload::Bytes(b""));
    }
}

#[tokio::test(start_paused = true)]
async fn c12_9_clones_of_a_signer_share_the_credentials_and_the_time_anchor() {
    let a = Who::s3().keys("AKIDFIRST", "first-s3cr3t");
    let b = Who::s3()
        .keys("AKIDSECOND", "second-s3cr3t")
        .token("second-token");
    let credentials = a.credentials();
    let signer = a.signer_with(&base(BUCKET), credentials.clone());
    let early_clone = signer.clone();
    let script = Script::new();
    for _ in 0..3 {
        script.answer(200);
    }
    success(
        store(&script, BUCKET, signer.clone())
            .get("k".to_owned())
            .await,
    );
    tokio::time::advance(secs(3600)).await;
    // A clone made before the first use, used an hour of Tokio time later.
    success(
        store(&script, BUCKET, early_clone)
            .get("k".to_owned())
            .await,
    );
    credentials.replace(b.credentials());
    tokio::time::advance(secs(60)).await;
    // A clone made after the first use and after the replacement.
    success(
        store(&script, BUCKET, signer.clone())
            .get("k".to_owned())
            .await,
    );
    let requests = script.requests();
    let dates: Vec<i64> = [(&requests[0], &a), (&requests[1], &a), (&requests[2], &b)]
        .iter()
        .map(|(request, who)| {
            aws::epoch_seconds(&judged(request, who, Payload::Bytes(b"")).amz_date)
        })
        .collect();
    assert_eq!(
        dates[1] - dates[0],
        3600,
        "the clones share the time anchor"
    );
    assert_eq!(dates[2] - dates[1], 60);
}

#[tokio::test(start_paused = true)]
async fn c12_9_every_page_of_a_paginated_call_is_signed_anew() {
    let who = Who::s3().token(TOKEN);
    let script = Script::new();
    script
        .answer(200)
        .header(
            "link",
            "<https://examplebucket.s3.amazonaws.com/list?page=2>; rel=\"next\"",
        )
        .body("one")
        .after(secs(3));
    script
        .answer(200)
        .header(
            "link",
            "<https://examplebucket.s3.amazonaws.com/list?page=3&b=+>; rel=\"next\"",
        )
        .body("two")
        .after(secs(4));
    script.answer(200).body("three");
    let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
    let mut pages = client.listed("list".to_owned()).pages();
    for expected in ["one", "two", "three"] {
        match pages.try_next().await {
            Ok(Some(page)) => assert_eq!(page, expected),
            Ok(None) => panic!("the end before {expected}"),
            Err(error) => panic!("{expected}: {error} {:?}", SignError::find(&error)),
        }
    }
    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    let verdicts: Vec<Verdict> = requests
        .iter()
        .map(|request| judged(request, &who, Payload::Bytes(b"")))
        .collect();
    for (i, gap) in [(1, 3), (2, 4)] {
        assert_eq!(
            aws::epoch_seconds(&verdicts[i].amz_date)
                - aws::epoch_seconds(&verdicts[i - 1].amz_date),
            gap
        );
        assert_ne!(verdicts[i].signature, verdicts[i - 1].signature);
    }
}

#[tokio::test(start_paused = true)]
async fn c12_10_a_request_whose_framing_the_transport_would_change_is_refused() {
    let who = Who::s3();
    type Fields = &'static [(&'static str, &'static [u8])];
    let cases: [(&str, Fields); 7] = [
        (
            "length and chunked",
            &[("content-length", b"7"), ("transfer-encoding", b"chunked")],
        ),
        ("a shorter length", &[("content-length", b"3")]),
        ("a longer length", &[("content-length", b"8")]),
        (
            "a length given twice",
            &[("content-length", b"7"), ("content-length", b"7")],
        ),
        (
            "a length that is not a number",
            &[("content-length", b"seven")],
        ),
        ("a signed length", &[("content-length", b"+7")]),
        (
            "Connection naming Authorization",
            &[("connection", b"authorization")],
        ),
    ];
    for (case, headers) in cases {
        let script = Script::new();
        script.answer(200);
        let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
        let error = failure(
            client
                .put("k".to_owned(), Body::new("payload", "text/plain"))
                .edit_request(move |request| {
                    for (name, value) in headers {
                        insert(request, name, value);
                    }
                })
                .await,
        );
        assert_eq!(refusal(&error), "InvalidRequest", "{case}");
        assert!(script.requests().is_empty(), "{case}");
    }
    // The true length of a body of bytes is accepted, and signed.
    let script = Script::new();
    script.answer(200);
    let client = store(&script, BUCKET, who.signer(&base(BUCKET)));
    success(
        client
            .put("k".to_owned(), Body::new("payload", "text/plain"))
            .edit_request(|request| insert(request, "content-length", b"7"))
            .await,
    );
    let verdict = judged(&script.requests()[0], &who, Payload::Bytes(b"payload"));
    assert!(
        verdict
            .signed_headers
            .split(';')
            .any(|n| n == "content-length"),
        "{}",
        verdict.signed_headers
    );
}

#[tokio::test(start_paused = true)]
async fn c12_10_unsigned_payload_is_for_s3_only_and_another_service_refuses_the_stream() {
    let who = Who::iam();
    let origin = "https://iam.amazonaws.com";
    let script = Script::new();
    script.answer(200);
    let signer = who.signer(&base(origin)).allow_unsigned_payload();
    let client = store(&script, origin, signer);
    let (body, polled) = counted_stream(&[b"never read"]);
    let error = failure(client.put("k".to_owned(), body).await);
    assert_eq!(refusal(&error), "StreamingBody");
    assert!(!polled.load(Ordering::SeqCst), "the stream was polled");
    assert!(script.requests().is_empty());
    // A body of bytes is still hashed and signed.
    success(
        client
            .put("k".to_owned(), Body::new("bytes", "text/plain"))
            .await,
    );
    judged(&script.requests()[0], &who, Payload::Bytes(b"bytes"));
}

#[test]
fn c12_10_check_origin_compares_scheme_host_and_effective_port_only() {
    let base = base("https://api.example.test/v1");
    for same in [
        "https://api.example.test/v2/x?y=1",
        "https://api.example.test:443/",
        "https://API.example.test/v1",
        "https://api.example.test",
    ] {
        let request = raw(http::Method::GET, same);
        check_origin(&request, &base).unwrap_or_else(|e| panic!("{same}: {e:?}"));
    }
    for other in [
        "http://api.example.test/v1",
        "https://api.example.test:8443/v1",
        "https://api.example.test.evil.test/v1",
        "https://example.test/v1",
        "https://other.example.test/v1",
        "http://api.example.test:443/v1",
    ] {
        let request = raw(http::Method::GET, other);
        let error = check_origin(&request, &base).expect_err(other);
        assert_eq!(format!("{error:?}"), "WrongOrigin", "{other}");
    }
    // An explicit non-default port in the base.
    let base = self::base("http://127.0.0.1:8080");
    check_origin(&raw(http::Method::GET, "http://127.0.0.1:8080/x"), &base).expect("same");
    let error = check_origin(&raw(http::Method::GET, "http://127.0.0.1/x"), &base)
        .expect_err("port 80 is another origin");
    assert_eq!(format!("{error:?}"), "WrongOrigin");
}

#[test]
fn c12_10_body_bytes_borrows_bytes_and_refuses_a_stream_without_polling_it() {
    let mut request = raw(http::Method::PUT, "https://example.test/");
    assert_eq!(body_bytes(&request).expect("no body"), b"");
    *request.body_mut() = Some(reqwest::Body::from(Vec::new()));
    assert_eq!(body_bytes(&request).expect("empty"), b"");
    *request.body_mut() = Some(reqwest::Body::from(b"\x00\xffbytes".to_vec()));
    assert_eq!(body_bytes(&request).expect("bytes"), b"\x00\xffbytes");

    let (body, polled) = counted_request_stream(&[b"chunk"]);
    *request.body_mut() = Some(body);
    insert(&mut request, "content-length", b"5");
    let error = body_bytes(&request).expect_err("a stream");
    assert_eq!(format!("{error:?}"), "StreamingBody");
    assert!(!polled.load(Ordering::SeqCst));
}

#[tokio::test(start_paused = true)]
async fn c12_10_no_key_token_or_signature_is_printed() {
    let who = Who::s3().token(TOKEN);
    let credentials = who.credentials();
    let signer = who.signer_with(&base(BUCKET), credentials.clone());
    for text in [format!("{credentials:?}"), format!("{signer:?}")] {
        for value in [ACCESS, SECRET, TOKEN, MARK] {
            assert!(!text.contains(value), "{text}");
        }
    }

    // A response that fails after a signed request: nothing of the signature is printed.
    let script = Script::new();
    script.answer(403).body("denied");
    let client = store(&script, BUCKET, signer.clone());
    let error = failure(client.get("k".to_owned()).await);
    let request = &script.requests()[0];
    let verdict = judged(request, &who, Payload::Bytes(b""));
    let text = printed(&error);
    for value in [ACCESS, SECRET, TOKEN, MARK, verdict.signature.as_str()] {
        assert!(!text.contains(value), "{value} in {text}");
    }

    // Refusals of every kind print no value.
    let refusals: Vec<(Store, Body)> = vec![
        (
            Store::new(base(BUCKET), signer.clone())
                .with_base(base("https://other-s3cr3t.example.test"))
                .with_transport(script.clone()),
            Body::new("b", "text/plain"),
        ),
        (
            store(&script, BUCKET, signer.clone()),
            counted_stream(&[b"s3cr3t"]).0,
        ),
        (
            store(
                &script,
                BUCKET,
                Who::s3().keys("AKID/s3cr3t", SECRET).signer(&base(BUCKET)),
            ),
            Body::new("b", "text/plain"),
        ),
    ];
    for (client, body) in refusals {
        let error = failure(client.put("k-s3cr3t".to_owned(), body).await);
        let _ = refusal(&error);
        let text = printed(&error);
        for value in [ACCESS, SECRET, TOKEN, MARK] {
            assert!(!text.contains(value), "{value} in {text}");
        }
    }
    assert_eq!(script.requests().len(), 1);
}
