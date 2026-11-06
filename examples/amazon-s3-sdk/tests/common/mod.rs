//! Test support shared by `protocol.rs` and `workflows.rs`:
//!
//! - an **independent** SigV4 verifier written from AWS's documented algorithm (it shares only
//!   the SHA-256/HMAC primitives with the SDK's signer): it rebuilds the canonical request from
//!   what arrived on the wire, the string to sign and the signature;
//! - a stateful fake S3 (buckets as host labels, objects, conditional and ranged reads, fixture
//!   listing pages) on the loopback `TestServer`;
//! - synthetic credentials and a frozen clock.
//!
//! Nothing here contacts AWS. A fake server proves the SDK's side of the contract as the
//! fixtures and the documentation describe it, not AWS conformance.

#![allow(dead_code)] // Each test crate uses a different part of this module.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use concord_example_testserver::{Recorded, Reply};
use concord_sdk_amazon_s3::{Clock, Credentials, Region, S3Client, http_client_builder};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

/// AWS's documentation example access key (the "Examples: Signature Calculations" section of
/// the single-chunk SigV4 page). It is published as an example and is not a working key.
pub const DOC_ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
/// The documentation example secret paired with [`DOC_ACCESS_KEY`].
pub const DOC_SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";

/// Synthetic credentials for every other test: deliberately not shaped like AWS keys.
pub const SYNTHETIC_ACCESS_KEY: &str = "SYNTHETIC-ACCESS-KEY-A";
pub const SYNTHETIC_SECRET: &str = "synthetic-secret-a-not-a-real-key";
pub const OTHER_ACCESS_KEY: &str = "SYNTHETIC-ACCESS-KEY-B";
pub const OTHER_SECRET: &str = "synthetic-secret-b-not-a-real-key";
pub const SYNTHETIC_SESSION_TOKEN: &str = "synthetic-session-token-not-real";

pub fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {path}: {e}"))
}

pub fn fixture_text(name: &str) -> String {
    String::from_utf8(fixture(name)).expect("UTF-8 fixture")
}

/// `2013-05-24T00:00:00Z`, the time of AWS's signing examples.
pub fn doc_time() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_369_353_600)
}

/// `2026-11-06T12:00:00Z`, the frozen time of the other tests.
pub fn frozen_time() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_793_966_400)
}

pub fn synthetic_credentials() -> Credentials {
    Credentials::new(SYNTHETIC_ACCESS_KEY, SYNTHETIC_SECRET).unwrap()
}

pub fn other_credentials() -> Credentials {
    Credentials::new(OTHER_ACCESS_KEY, OTHER_SECRET).unwrap()
}

/// A client for `region` whose endpoint is `http://s3.test:<port>`, every listed bucket host
/// resolved to the loopback server, with synthetic credentials and the frozen clock.
pub fn client_for(addr: SocketAddr, region: &str, buckets: &[&str]) -> S3Client {
    let mut http = http_client_builder();
    for bucket in buckets {
        http = http.resolve(&format!("{bucket}.s3.test"), addr);
    }
    S3Client::new(
        http.build().unwrap(),
        Region::new(region).unwrap(),
        synthetic_credentials(),
    )
    .unwrap()
    .with_endpoint(&format!("http://s3.test:{}", addr.port()))
    .unwrap()
    .with_clock(Clock::fixed(frozen_time()))
}

// ---------------------------------------------------------------------------------------------
// The independent SigV4 verifier.
// ---------------------------------------------------------------------------------------------

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key).unwrap();
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// `HMAC(HMAC(HMAC(HMAC("AWS4" + secret, date), region), service), "aws4_request")`.
pub fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k_region = hmac(&k_date, region.as_bytes());
    let k_service = hmac(&k_region, service.as_bytes());
    hmac(&k_service, b"aws4_request")
}

pub fn signature(signing_key: &[u8], string_to_sign: &str) -> String {
    hex(&hmac(signing_key, string_to_sign.as_bytes()))
}

/// AWS's `UriEncode`: every byte except `A-Z a-z 0-9 - . _ ~`, uppercase hex; `/` kept when
/// `keep_slash`.
pub fn uri_encode(text: &str, keep_slash: bool) -> String {
    let mut out = String::new();
    for &b in text.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) || (keep_slash && b == b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Decodes `%XX` escapes only (a `+` is a literal plus, as in a path).
pub fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The parts of an `Authorization: AWS4-HMAC-SHA256 ...` header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Authorization {
    pub access_key: String,
    pub date: String,
    pub region: String,
    pub service: String,
    pub signed_headers: Vec<String>,
    pub signature: String,
}

pub fn parse_authorization(value: &str) -> Option<Authorization> {
    let rest = value.strip_prefix("AWS4-HMAC-SHA256 ")?;
    let mut credential = None;
    let mut signed = None;
    let mut signature = None;
    for part in rest.split(',') {
        let (k, v) = part.trim().split_once('=')?;
        match k {
            "Credential" => credential = Some(v),
            "SignedHeaders" => signed = Some(v),
            "Signature" => signature = Some(v),
            _ => return None,
        }
    }
    let scope: Vec<&str> = credential?.split('/').collect();
    let [access_key, date, region, service, "aws4_request"] = scope[..] else {
        return None;
    };
    Some(Authorization {
        access_key: access_key.to_owned(),
        date: date.to_owned(),
        region: region.to_owned(),
        service: service.to_owned(),
        signed_headers: signed?.split(';').map(str::to_owned).collect(),
        signature: signature?.to_owned(),
    })
}

/// A request as the verifier sees it: method, raw target, header lines in order, body.
pub struct WireRequest<'a> {
    pub method: &'a str,
    pub target: &'a str,
    pub headers: Vec<(String, String)>,
    pub body: &'a [u8],
}

impl<'a> WireRequest<'a> {
    pub fn from_recorded(recorded: &'a Recorded) -> Self {
        Self {
            method: &recorded.method,
            target: &recorded.target,
            headers: recorded.headers.clone(),
            body: &recorded.body,
        }
    }

    pub fn header_values(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }
}

/// Why the verifier refused a request (each is a 403 from the fake S3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    MissingAuthorization,
    MalformedAuthorization,
    PathNotCanonical,
    QueryNotDecodable,
    HostNotSigned,
    AmzHeaderNotSigned,
    SignedHeaderMissing,
    SignedHeadersNotSorted,
    MissingDate,
    DateScopeMismatch,
    PayloadHashMismatch,
    UnknownAccessKey,
    SignatureDoesNotMatch,
}

/// What the verifier computed.
#[derive(Clone, Debug)]
pub struct Verified {
    pub authorization: Authorization,
    pub canonical_request: String,
    pub string_to_sign: String,
}

/// The canonical request of a wire request, following AWS's documented steps: the path must
/// already be in S3's canonical form (it is used as sent, without normalization); query
/// parameters are decoded, `UriEncode`d and sorted; signed header values are trimmed, inner
/// whitespace collapsed, repeated lines joined with `,`.
pub fn canonical_request(
    wire: &WireRequest<'_>,
    signed_headers: &[String],
    payload_hash: &str,
) -> Result<String, Refusal> {
    let (path, query) = match wire.target.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (wire.target, None),
    };
    let decoded = percent_decode(path).ok_or(Refusal::PathNotCanonical)?;
    if uri_encode(&decoded, true) != path {
        return Err(Refusal::PathNotCanonical);
    }
    let mut pairs = Vec::new();
    for pair in query.unwrap_or("").split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let k = percent_decode(k).ok_or(Refusal::QueryNotDecodable)?;
        let v = percent_decode(v).ok_or(Refusal::QueryNotDecodable)?;
        pairs.push((uri_encode(&k, false), uri_encode(&v, false)));
    }
    pairs.sort();
    let query = pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let mut headers = String::new();
    for name in signed_headers {
        let values = wire.header_values(name);
        if values.is_empty() {
            return Err(Refusal::SignedHeaderMissing);
        }
        let joined = values
            .iter()
            .map(|v| v.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join(",");
        headers.push_str(&format!("{name}:{joined}\n"));
    }
    Ok(format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        wire.method,
        path,
        query,
        headers,
        signed_headers.join(";"),
        payload_hash
    ))
}

pub fn string_to_sign(amz_date: &str, scope: &str, canonical_request: &str) -> String {
    format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    )
}

/// Verifies a wire request's SigV4 `Authorization` header as S3 would.
pub fn verify(
    wire: &WireRequest<'_>,
    secret_for: impl Fn(&str) -> Option<String>,
) -> Result<Verified, Refusal> {
    let [authorization] = wire.header_values("authorization")[..] else {
        return Err(Refusal::MissingAuthorization);
    };
    let auth = parse_authorization(authorization).ok_or(Refusal::MalformedAuthorization)?;
    if auth.service != "s3" {
        return Err(Refusal::MalformedAuthorization);
    }
    let mut sorted = auth.signed_headers.clone();
    sorted.sort();
    sorted.dedup();
    if sorted != auth.signed_headers {
        return Err(Refusal::SignedHeadersNotSorted);
    }
    if !auth.signed_headers.iter().any(|h| h == "host") {
        return Err(Refusal::HostNotSigned);
    }
    for (name, _) in &wire.headers {
        if name.starts_with("x-amz-") && !auth.signed_headers.contains(name) {
            return Err(Refusal::AmzHeaderNotSigned);
        }
    }
    let [amz_date] = wire.header_values("x-amz-date")[..] else {
        return Err(Refusal::MissingDate);
    };
    if !amz_date.starts_with(&auth.date) || amz_date.len() != 16 {
        return Err(Refusal::DateScopeMismatch);
    }
    let payload_hash = match wire.header_values("x-amz-content-sha256")[..] {
        ["UNSIGNED-PAYLOAD"] => "UNSIGNED-PAYLOAD".to_owned(),
        [hash] if hash == sha256_hex(wire.body) => hash.to_owned(),
        _ => return Err(Refusal::PayloadHashMismatch),
    };
    let canonical = canonical_request(wire, &auth.signed_headers, &payload_hash)?;
    let scope = format!(
        "{}/{}/{}/aws4_request",
        auth.date, auth.region, auth.service
    );
    let to_sign = string_to_sign(amz_date, &scope, &canonical);
    let secret = secret_for(&auth.access_key).ok_or(Refusal::UnknownAccessKey)?;
    let key = signing_key(&secret, &auth.date, &auth.region, &auth.service);
    if signature(&key, &to_sign) != auth.signature {
        return Err(Refusal::SignatureDoesNotMatch);
    }
    Ok(Verified {
        authorization: auth,
        canonical_request: canonical,
        string_to_sign: to_sign,
    })
}

/// A native request (already signed by the SDK) as wire input for the verifier: the target
/// reqwest sends, the headers present on the request, the body bytes.
pub fn wire_of(
    request: &concord::reqwest::Request,
) -> (String, String, Vec<(String, String)>, Vec<u8>) {
    let url = request.url();
    let target = match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => url.path().to_owned(),
    };
    let headers = request
        .headers()
        .iter()
        .map(|(n, v)| (n.as_str().to_owned(), v.to_str().unwrap().to_owned()))
        .collect();
    let body = request
        .body()
        .and_then(|b| b.as_bytes())
        .map(<[u8]>::to_vec)
        .unwrap_or_default();
    (request.method().as_str().to_owned(), target, headers, body)
}

/// Verifies a native signed request with the synthetic and documentation credentials.
pub fn verify_native(request: &concord::reqwest::Request) -> Result<Verified, Refusal> {
    let (method, target, headers, body) = wire_of(request);
    let wire = WireRequest {
        method: &method,
        target: &target,
        headers,
        body: &body,
    };
    verify(&wire, known_secret)
}

pub fn known_secret(access_key: &str) -> Option<String> {
    match access_key {
        SYNTHETIC_ACCESS_KEY => Some(SYNTHETIC_SECRET.to_owned()),
        OTHER_ACCESS_KEY => Some(OTHER_SECRET.to_owned()),
        DOC_ACCESS_KEY => Some(DOC_SECRET_KEY.to_owned()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// S3 replies.
// ---------------------------------------------------------------------------------------------

pub fn error_reply(status: u16, code: &str, message: &str) -> Reply {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Error><Code>{code}</Code><Message>{message}</Message><RequestId>FAKE-REQUEST-ID</RequestId><HostId>fake-host-id</HostId></Error>"
    );
    Reply::text(status, "application/xml", body)
        .header("x-amz-request-id", "FAKE-REQUEST-ID")
        .header("x-amz-id-2", "fake-host-id")
}

pub fn xml_reply(body: impl AsRef<[u8]>) -> Reply {
    Reply::text(200, "application/xml", body)
        .header("x-amz-request-id", "FAKE-REQUEST-ID")
        .header("x-amz-id-2", "fake-host-id")
}

// ---------------------------------------------------------------------------------------------
// A stateful fake S3.
// ---------------------------------------------------------------------------------------------

/// One stored object.
#[derive(Clone, Debug)]
pub struct StoredObject {
    pub bytes: Vec<u8>,
    pub content_type: String,
    pub etag: String,
    pub last_modified: String,
    pub metadata: BTreeMap<String, String>,
    pub storage_class: Option<String>,
}

/// A listing served from fixture pages: the page for no token, then one page per token.
#[derive(Clone, Debug)]
pub struct FixtureListing {
    pub prefix: String,
    pub first: Vec<u8>,
    pub next: BTreeMap<String, Vec<u8>>,
}

#[derive(Default)]
pub struct FakeState {
    pub objects: BTreeMap<(String, String), StoredObject>,
    pub listings: BTreeMap<String, FixtureListing>,
    pub verified: Vec<Verified>,
    pub refusals: Vec<Refusal>,
    pub puts: u64,
}

/// The fake S3: `respond` is the `TestServer` responder.
#[derive(Clone, Default)]
pub struct FakeS3 {
    pub state: Arc<Mutex<FakeState>>,
}

fn bucket_of(host: &str) -> Option<String> {
    let name = host.split(':').next()?;
    name.strip_suffix(".s3.test").map(str::to_owned)
}

fn header<'a>(recorded: &'a Recorded, name: &str) -> Option<&'a str> {
    recorded
        .headers
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

/// RFC 9110 `If-Match`/`If-None-Match` comparison for one tag (S3 sends one).
fn etag_matches(condition: &str, etag: &str) -> bool {
    condition.trim() == "*" || condition.split(',').any(|c| c.trim() == etag)
}

impl FakeS3 {
    pub fn insert(&self, bucket: &str, key: &str, object: StoredObject) {
        self.state
            .lock()
            .unwrap()
            .objects
            .insert((bucket.to_owned(), key.to_owned()), object);
    }

    pub fn object(&self, bucket: &str, key: &str) -> Option<StoredObject> {
        self.state
            .lock()
            .unwrap()
            .objects
            .get(&(bucket.to_owned(), key.to_owned()))
            .cloned()
    }

    pub fn refusals(&self) -> Vec<Refusal> {
        self.state.lock().unwrap().refusals.clone()
    }

    pub fn verified(&self) -> Vec<Verified> {
        self.state.lock().unwrap().verified.clone()
    }

    pub fn respond(&self, recorded: &Recorded) -> Reply {
        let wire = WireRequest::from_recorded(recorded);
        let mut state = self.state.lock().unwrap();
        match verify(&wire, known_secret) {
            Ok(verified) => state.verified.push(verified),
            Err(refusal) => {
                state.refusals.push(refusal);
                let code = match refusal {
                    Refusal::SignatureDoesNotMatch => "SignatureDoesNotMatch",
                    Refusal::UnknownAccessKey => "InvalidAccessKeyId",
                    Refusal::PayloadHashMismatch => "XAmzContentSHA256Mismatch",
                    _ => "AccessDenied",
                };
                if recorded.method == "HEAD" {
                    return Reply::new(403).header("x-amz-request-id", "FAKE-REQUEST-ID");
                }
                return error_reply(
                    403,
                    code,
                    "The request signature we calculated does not match",
                );
            }
        }
        let Some(bucket) = bucket_of(&recorded.host) else {
            return error_reply(400, "InvalidRequest", "unexpected host");
        };
        let path = recorded.path();
        let key = percent_decode(path.strip_prefix('/').unwrap_or(path)).unwrap_or_default();
        let query: BTreeMap<String, String> = recorded
            .query()
            .unwrap_or("")
            .split('&')
            .filter(|p| !p.is_empty())
            .map(|p| {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                (percent_decode(k).unwrap(), percent_decode(v).unwrap())
            })
            .collect();
        match (recorded.method.as_str(), key.is_empty()) {
            ("GET", true) if query.get("list-type").map(String::as_str) == Some("2") => {
                let Some(listing) = state.listings.get(&bucket) else {
                    return error_reply(404, "NoSuchBucket", "The specified bucket does not exist");
                };
                if query.get("prefix") != Some(&listing.prefix) {
                    return error_reply(400, "InvalidArgument", "unexpected prefix");
                }
                match query.get("continuation-token") {
                    None => xml_reply(listing.first.clone()),
                    Some(token) => match listing.next.get(token) {
                        Some(page) => xml_reply(page.clone()),
                        None => error_reply(
                            400,
                            "InvalidArgument",
                            "The continuation token provided is incorrect",
                        ),
                    },
                }
            }
            ("PUT", false) => {
                let content_type =
                    header(recorded, "content-type").unwrap_or("binary/octet-stream");
                if let Some(cond) = header(recorded, "if-none-match")
                    && cond == "*"
                    && state.objects.contains_key(&(bucket.clone(), key.clone()))
                {
                    return error_reply(
                        412,
                        "PreconditionFailed",
                        "At least one of the pre-conditions you specified did not hold",
                    );
                }
                state.puts += 1;
                let etag = format!("\"opaque-etag-{}\"", state.puts);
                let metadata = recorded
                    .headers
                    .iter()
                    .filter_map(|(n, v)| {
                        n.strip_prefix("x-amz-meta-")
                            .map(|m| (m.to_owned(), v.clone()))
                    })
                    .collect();
                state.objects.insert(
                    (bucket, key),
                    StoredObject {
                        bytes: recorded.body.clone(),
                        content_type: content_type.to_owned(),
                        etag: etag.clone(),
                        last_modified: "Fri, 06 Nov 2026 12:00:00 GMT".to_owned(),
                        metadata,
                        storage_class: header(recorded, "x-amz-storage-class").map(str::to_owned),
                    },
                );
                Reply::new(200)
                    .header("x-amz-request-id", "FAKE-PUT-REQUEST-ID")
                    .header("x-amz-id-2", "fake-host-id")
                    .header("etag", etag)
            }
            (method @ ("GET" | "HEAD"), false) => {
                let head = method == "HEAD";
                let Some(object) = state.objects.get(&(bucket, key)).cloned() else {
                    return if head {
                        Reply::new(404).header("x-amz-request-id", "FAKE-REQUEST-ID")
                    } else {
                        error_reply(404, "NoSuchKey", "The specified key does not exist.")
                    };
                };
                object_reply(recorded, &object, head)
            }
            _ => error_reply(405, "MethodNotAllowed", "unsupported by the fake"),
        }
    }
}

fn validators(reply: Reply, object: &StoredObject) -> Reply {
    reply
        .header("x-amz-request-id", "FAKE-REQUEST-ID")
        .header("x-amz-id-2", "fake-host-id")
        .header("etag", object.etag.clone())
        .header("last-modified", object.last_modified.clone())
}

fn object_reply(recorded: &Recorded, object: &StoredObject, head: bool) -> Reply {
    let modified = httpdate::parse_http_date(&object.last_modified).unwrap();
    let since = |name| header(recorded, name).and_then(|v| httpdate::parse_http_date(v).ok());
    // RFC 9110 §13.2.2 evaluation order, as S3 documents its combinations.
    if let Some(cond) = header(recorded, "if-match") {
        if !etag_matches(cond, &object.etag) {
            return precondition_failed(head);
        }
    } else if let Some(date) = since("if-unmodified-since")
        && modified > date
    {
        return precondition_failed(head);
    }
    if let Some(cond) = header(recorded, "if-none-match") {
        if etag_matches(cond, &object.etag) {
            return validators(Reply::new(304), object);
        }
    } else if let Some(date) = since("if-modified-since")
        && modified <= date
    {
        return validators(Reply::new(304), object);
    }
    let total = object.bytes.len() as u64;
    let mut reply;
    let body: &[u8];
    match header(recorded, "range") {
        Some(range) => {
            let Some((first, last)) = parse_range(range, total) else {
                return if head {
                    Reply::new(416)
                } else {
                    error_reply(
                        416,
                        "InvalidRange",
                        "The requested range is not satisfiable",
                    )
                };
            };
            reply =
                Reply::new(206).header("content-range", format!("bytes {first}-{last}/{total}"));
            body = &object.bytes[first as usize..=last as usize];
        }
        None => {
            reply = Reply::new(200);
            body = &object.bytes;
        }
    }
    reply = validators(reply, object)
        .header("accept-ranges", "bytes")
        .header("content-type", object.content_type.clone());
    if let Some(class) = &object.storage_class {
        reply = reply.header("x-amz-storage-class", class.clone());
    }
    for (name, value) in &object.metadata {
        reply = reply.header(&format!("x-amz-meta-{name}"), value.clone());
    }
    if head {
        head_reply(reply, body)
    } else {
        reply.body(body)
    }
}

/// A `HEAD` answer whose `Content-Length` is the length a `GET` would send.
///
/// The loopback test server always derives `content-length` from the body it writes, and has no
/// way to declare a length without sending the bytes. So the bytes are written and the reply says
/// `connection: close`: the client does not read a body after `HEAD` and does not reuse the
/// connection, so the unread bytes are discarded with it. (A limitation of the test support, not
/// of the SDK.)
pub fn head_reply(reply: Reply, body: &[u8]) -> Reply {
    reply.header("connection", "close").body(body)
}

fn precondition_failed(head: bool) -> Reply {
    if head {
        Reply::new(412).header("x-amz-request-id", "FAKE-REQUEST-ID")
    } else {
        error_reply(
            412,
            "PreconditionFailed",
            "At least one of the pre-conditions you specified did not hold",
        )
    }
}

/// One `bytes=` range against `total` bytes (`None` when unsatisfiable or malformed).
fn parse_range(range: &str, total: u64) -> Option<(u64, u64)> {
    let spec = range.strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (first, last) = spec.split_once('-')?;
    let (first, last) = match (first, last) {
        ("", suffix) => {
            let len: u64 = suffix.parse().ok()?;
            (total.checked_sub(len.min(total))?, total.checked_sub(1)?)
        }
        (first, "") => (first.parse().ok()?, total.checked_sub(1)?),
        (first, last) => (
            first.parse().ok()?,
            last.parse::<u64>().ok()?.min(total.checked_sub(1)?),
        ),
    };
    (first <= last && first < total).then_some((first, last))
}
