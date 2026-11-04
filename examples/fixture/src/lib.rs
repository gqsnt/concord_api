//! Test support for the SDK examples (`examples/sdk`, `examples/app`, `examples/extension`):
//! a tiny HTTP/1.1 loopback server that records each request exactly as received, and
//! deterministic fake Catalog and Ledger services.
//!
//! The server deliberately uses no HTTP server library, so assertions observe what reqwest put on
//! the wire: the raw request target, header lines in order, and the body (a `content-length` body
//! or a decoded `transfer-encoding: chunked` body). It answers with a scripted [`Reply`].
//!
//! The services ([`catalog`], [`ledger`]) are fixed data, not a model of any real API. They answer
//! finite errors for anything they do not understand.

use std::fmt::Write as _;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use percent_encoding::percent_decode_str;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// One request as observed on the wire.
#[derive(Debug, Clone)]
pub struct Recorded {
    /// The request method.
    pub method: String,
    /// The raw request target, e.g. `/catalog/v1/tenants/a%2Fb/items?tag=x&tag=y`.
    pub target: String,
    /// Header lines in wire order, names lowercased.
    pub headers: Vec<(String, String)>,
    /// The body bytes (a chunked body decoded).
    pub body: Vec<u8>,
    /// Whether the body arrived with `transfer-encoding: chunked`.
    pub chunked: bool,
}

impl Recorded {
    /// The raw path, without the query.
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }

    /// The raw query string, if any.
    pub fn query(&self) -> Option<&str> {
        self.target.split_once('?').map(|(_, q)| q)
    }

    /// The path segments, percent-decoded (a `%2F` in a segment stays inside that segment).
    pub fn segments(&self) -> Vec<String> {
        self.path()
            .split('/')
            .skip(1)
            .map(|s| percent_decode_str(s).decode_utf8_lossy().into_owned())
            .collect()
    }

    /// The decoded query pairs, in order (`+` is a space, as in form encoding).
    pub fn query_pairs(&self) -> Vec<(String, String)> {
        let Some(query) = self.query() else {
            return Vec::new();
        };
        query
            .split('&')
            .filter(|p| !p.is_empty())
            .map(|pair| {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                (form_decode(k), form_decode(v))
            })
            .collect()
    }

    /// Every decoded value of one query key, in order.
    pub fn query_all(&self, key: &str) -> Vec<String> {
        self.query_pairs()
            .into_iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v)
            .collect()
    }

    /// All values of a header (case-insensitive name), in wire order.
    pub fn header_all(&self, name: &str) -> Vec<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// The single value of a header; panics if it occurs more than once.
    pub fn header(&self, name: &str) -> Option<&str> {
        let all = self.header_all(name);
        assert!(all.len() <= 1, "header {name} occurs {} times", all.len());
        all.first().copied()
    }
}

fn form_decode(s: &str) -> String {
    percent_decode_str(&s.replace('+', " "))
        .decode_utf8_lossy()
        .into_owned()
}

/// A scripted response.
#[derive(Debug, Clone)]
pub struct Reply {
    /// Status code.
    pub status: u16,
    /// Header lines, in order.
    pub headers: Vec<(String, String)>,
    /// Body bytes (sent with `content-length`).
    pub body: Vec<u8>,
    /// Close the connection after reading the request, without answering.
    pub drop_connection: bool,
}

impl Reply {
    /// An empty response with `status`.
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
            drop_connection: false,
        }
    }

    /// No answer: the connection is closed after the request is read.
    pub fn hang_up() -> Self {
        Self {
            drop_connection: true,
            ..Self::new(0)
        }
    }

    /// A JSON body with `application/json`.
    pub fn json(status: u16, body: &str) -> Self {
        Self::new(status)
            .header("content-type", "application/json")
            .body(body)
    }

    /// A text body with `text/plain; charset=utf-8`.
    pub fn text(status: u16, body: &str) -> Self {
        Self::new(status)
            .header("content-type", "text/plain; charset=utf-8")
            .body(body)
    }

    /// The conventional Catalog error document.
    pub fn problem(status: u16, code: &str, message: &str) -> Self {
        let body = serde_json::json!({ "error": { "code": code, "message": message } });
        Self::json(status, &body.to_string())
    }

    /// Adds a header line.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    /// Replaces the body.
    pub fn body(mut self, body: impl AsRef<[u8]>) -> Self {
        self.body = body.as_ref().to_vec();
        self
    }
}

type Responder = dyn Fn(&Recorded) -> Reply + Send + Sync;

/// A running loopback server. Dropping it stops accepting new connections.
pub struct Server {
    /// The bound loopback address.
    pub addr: SocketAddr,
    requests: Arc<Mutex<Vec<Recorded>>>,
    connections: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    /// Starts a server answering every request with `respond(request)`.
    pub async fn start(respond: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(AtomicUsize::new(0));
        let respond: Arc<Responder> = Arc::new(respond);
        let log = Arc::clone(&requests);
        let accepted = Arc::clone(&connections);
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                accepted.fetch_add(1, Ordering::SeqCst);
                let log = Arc::clone(&log);
                let respond = Arc::clone(&respond);
                tokio::spawn(async move {
                    let _ = serve_connection(stream, log, respond).await;
                });
            }
        });
        Self {
            addr,
            requests,
            connections,
            task,
        }
    }

    /// Starts a server answering every request with the same reply.
    pub async fn reply(reply: Reply) -> Self {
        Self::start(move |_| reply.clone()).await
    }

    /// `http://127.0.0.1:<port>` (no trailing slash).
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Every request received so far, in arrival order.
    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }

    /// How many requests arrived.
    pub fn count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    /// The only request received; panics unless exactly one arrived.
    pub fn single(&self) -> Recorded {
        let all = self.requests();
        assert_eq!(all.len(), 1, "expected exactly one request, got {all:#?}");
        all.into_iter().next().unwrap()
    }

    /// TCP connections accepted so far.
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn read_more(stream: &mut tokio::net::TcpStream, buf: &mut Vec<u8>) -> std::io::Result<bool> {
    let mut chunk = [0u8; 4096];
    let n = stream.read(&mut chunk).await?;
    buf.extend_from_slice(&chunk[..n]);
    Ok(n > 0)
}

async fn serve_connection(
    mut stream: tokio::net::TcpStream,
    log: Arc<Mutex<Vec<Recorded>>>,
    respond: Arc<Responder>,
) -> std::io::Result<()> {
    let mut buf = Vec::new();
    loop {
        let head_end = loop {
            if let Some(pos) = find(&buf, b"\r\n\r\n") {
                break pos;
            }
            if !read_more(&mut stream, &mut buf).await? {
                return Ok(());
            }
        };
        let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
        buf.drain(..head_end + 4);
        let mut lines = head.split("\r\n");
        let request_line = lines.next().unwrap_or_default();
        let mut parts = request_line.splitn(3, ' ');
        let method = parts.next().unwrap_or_default().to_owned();
        let target = parts.next().unwrap_or_default().to_owned();
        let headers: Vec<(String, String)> = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_owned()))
            .collect();
        let chunked = headers
            .iter()
            .any(|(n, v)| n == "transfer-encoding" && v.eq_ignore_ascii_case("chunked"));
        let body = if chunked {
            let mut body = Vec::new();
            loop {
                let line_end = loop {
                    if let Some(pos) = find(&buf, b"\r\n") {
                        break pos;
                    }
                    if !read_more(&mut stream, &mut buf).await? {
                        return Ok(());
                    }
                };
                let size_line = String::from_utf8_lossy(&buf[..line_end]).into_owned();
                buf.drain(..line_end + 2);
                let size =
                    usize::from_str_radix(size_line.split(';').next().unwrap_or("").trim(), 16)
                        .unwrap_or(0);
                while buf.len() < size + 2 {
                    if !read_more(&mut stream, &mut buf).await? {
                        return Ok(());
                    }
                }
                body.extend(buf.drain(..size));
                buf.drain(..2);
                if size == 0 {
                    break;
                }
            }
            body
        } else {
            let len: usize = headers
                .iter()
                .find(|(n, _)| n == "content-length")
                .and_then(|(_, v)| v.parse().ok())
                .unwrap_or(0);
            while buf.len() < len {
                if !read_more(&mut stream, &mut buf).await? {
                    return Ok(());
                }
            }
            buf.drain(..len).collect()
        };
        let recorded = Recorded {
            method,
            target,
            headers,
            body,
            chunked,
        };
        let reply = respond(&recorded);
        log.lock().unwrap().push(recorded);
        if reply.drop_connection {
            return Ok(());
        }
        let mut out = format!("HTTP/1.1 {} X\r\n", reply.status);
        for (n, v) in &reply.headers {
            let _ = write!(out, "{n}: {v}\r\n");
        }
        let _ = write!(out, "content-length: {}\r\n\r\n", reply.body.len());
        stream.write_all(out.as_bytes()).await?;
        stream.write_all(&reply.body).await?;
        stream.flush().await?;
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// The fake services' fixed data.
pub mod data {
    /// One Catalog item: id, name, tags.
    pub type ItemRow = (&'static str, &'static str, &'static [&'static str]);

    /// The items of every tenant (names are prefixed with the tenant for `studio`).
    pub const ITEMS: &[ItemRow] = &[
        ("item-001", "Notebook", &["featured", "paper"]),
        ("item-002", "Pen", &["featured", "office"]),
        ("item-003", "Stapler", &["office"]),
        ("item-004", "Lamp", &["featured", "light"]),
        ("item-005", "Desk", &["featured", "furniture"]),
        ("item-006", "Chair", &["furniture"]),
    ];

    /// Catalog activity events: id, decimal account id, Unix seconds, RFC 3339 text.
    pub const ACTIVITY: &[(&str, &str, i64, &str)] = &[
        ("event-001", "42", 1_768_471_200, "2026-01-15T10:00:00Z"),
        ("event-002", "42", 1_769_904_000, "2026-02-01T00:00:00Z"),
        ("event-003", "42", 1_772_323_200, "2026-03-01T00:00:00Z"),
        ("event-004", "7", 1_768_471_200, "2026-01-15T10:00:00Z"),
    ];

    /// Ledger entries: id, account (`acct-` hex), milliseconds, amount in minor units.
    pub const ENTRIES: &[(&str, &str, u64, i64)] = &[
        ("entry-001", "acct-002a", 1_768_471_200_000, 1200),
        ("entry-002", "acct-002a", 1_768_471_200_125, -300),
        ("entry-003", "acct-002a", 1_769_904_000_000, 4500),
        ("entry-004", "acct-002a", 1_770_000_000_000, 75),
        ("entry-005", "acct-002a", 1_772_323_200_000, -1000),
        ("entry-006", "acct-002a", 1_772_323_200_500, 20),
        ("entry-101", "acct-0007", 1_768_471_200_000, 990),
    ];
}

/// The fake Catalog service mounted at `prefix` (for example `/catalog/v1`; a test that checks
/// a base path uses a longer one such as `/edge/catalog/v1`).
///
/// | Request | Answer |
/// |---|---|
/// | any request without `x-api-key` | `401` problem |
/// | `GET {prefix}/health` | `200 text/plain` `catalog ready\n` |
/// | `GET {prefix}/accounts/{decimal}/activity[?since=RFC3339]` | `200` `{"events":[..]}`, events at or after `since` |
/// | `GET {prefix}/tenants/{t}/items` with `q`, repeated `tag`, `limit` (1–100, default 20), `cursor` | `200` `{"items":[..],"next_cursor":..}` and `x-result-count` |
/// | `GET {prefix}/tenants/{t}/items/{id}` | `200` item or `404` problem |
/// | `POST {prefix}/tenants/{t}/items` | `201` item (`item-new`) or `422` for an empty name |
/// | `DELETE {prefix}/tenants/{t}/items/{id}` | `204` or `404` |
/// | `PUT {prefix}/tenants/{t}/items/{id}/attachment` | `200` `{"bytes_received":n,"client_note":..}` from `x-client-note` |
/// | `GET {prefix}/tenants/{t}/exports/{export}` (absent from the SDK) | `200 text/csv` with `x-export-id`, or `404` |
///
/// Tenants are decoded path data: any tenant has the same items; `x-tenant-echo` answers with
/// the decoded tenant percent-encoded again (so non-ASCII tenants stay header-safe).
pub fn catalog(prefix: &'static str) -> impl Fn(&Recorded) -> Reply + Send + Sync + 'static {
    move |request| catalog_answer(prefix, request)
}

fn catalog_answer(prefix: &str, request: &Recorded) -> Reply {
    if request.header_all("x-api-key").is_empty() {
        return Reply::problem(401, "UNAUTHENTICATED", "x-api-key is required");
    }
    let prefix_segments = prefix.split('/').filter(|s| !s.is_empty()).count();
    let segments = request.segments();
    if segments.len() < prefix_segments
        || segments[..prefix_segments].join("/") != prefix.trim_matches('/')
    {
        return Reply::problem(404, "NOT_FOUND", "unknown route");
    }
    let rest: Vec<&str> = segments[prefix_segments..]
        .iter()
        .map(String::as_str)
        .collect();
    match (request.method.as_str(), rest.as_slice()) {
        ("GET", ["health"]) => Reply::text(200, "catalog ready\n"),
        ("GET", ["accounts", account, "activity"]) => {
            let since = request.query_all("since");
            let since = match since.as_slice() {
                [] => None,
                [one] => match rfc3339_seconds(one) {
                    Some(seconds) => Some(seconds),
                    None => {
                        return Reply::problem(
                            400,
                            "BAD_SINCE",
                            "since must be whole-second RFC 3339 UTC",
                        );
                    }
                },
                _ => return Reply::problem(400, "BAD_SINCE", "since given twice"),
            };
            if account.is_empty() || !account.bytes().all(|b| b.is_ascii_digit()) {
                return Reply::problem(400, "BAD_ACCOUNT", "account must be decimal");
            }
            let events: Vec<_> = data::ACTIVITY
                .iter()
                .filter(|(_, a, at, _)| a == account && since.is_none_or(|s| *at >= s))
                .map(|(id, a, _, at)| serde_json::json!({ "id": id, "account_id": a, "at": at }))
                .collect();
            Reply::json(200, &serde_json::json!({ "events": events }).to_string())
        }
        (method, ["tenants", tenant, rest @ ..]) => tenant_answer(method, tenant, rest, request)
            .header(
                "x-tenant-echo",
                &percent_encoding::utf8_percent_encode(tenant, percent_encoding::NON_ALPHANUMERIC)
                    .to_string(),
            ),
        _ => Reply::problem(404, "NOT_FOUND", "unknown route"),
    }
}

fn item_json(tenant: &str, (id, name, tags): &data::ItemRow) -> serde_json::Value {
    let name = if tenant == "studio" {
        format!("Studio {name}")
    } else {
        (*name).to_owned()
    };
    serde_json::json!({ "id": id, "name": name, "tags": tags })
}

fn tenant_answer(method: &str, tenant: &str, rest: &[&str], request: &Recorded) -> Reply {
    let find_item = |id: &str| data::ITEMS.iter().find(|(i, _, _)| *i == id);
    match (method, rest) {
        ("GET", ["items"]) => {
            let q = request.query_all("q");
            let tags = request.query_all("tag");
            let limit = match request.query_all("limit").as_slice() {
                [] => 20,
                [one] => match one.parse::<usize>() {
                    Ok(n) if (1..=100).contains(&n) => n,
                    _ => return Reply::problem(400, "BAD_LIMIT", "limit must be 1 through 100"),
                },
                _ => return Reply::problem(400, "BAD_LIMIT", "limit given twice"),
            };
            let start = match request.query_all("cursor").as_slice() {
                [] => 0,
                [one] => match one
                    .strip_prefix("catalog-offset-")
                    .and_then(|n| n.parse().ok())
                {
                    Some(n) => n,
                    None => return Reply::problem(400, "BAD_CURSOR", "unknown cursor"),
                },
                _ => return Reply::problem(400, "BAD_CURSOR", "cursor given twice"),
            };
            let needle = q.first().map(|q| q.to_lowercase()).unwrap_or_default();
            let matching: Vec<_> = data::ITEMS
                .iter()
                .filter(|row| tags.iter().all(|t| row.2.contains(&t.as_str())))
                .filter(|row| row.1.to_lowercase().contains(&needle))
                .collect();
            let page: Vec<_> = matching
                .iter()
                .skip(start)
                .take(limit)
                .map(|row| item_json(tenant, row))
                .collect();
            let next = (start + limit < matching.len())
                .then(|| format!("catalog-offset-{}", start + limit));
            Reply::json(
                200,
                &serde_json::json!({ "items": page, "next_cursor": next }).to_string(),
            )
            .header("x-result-count", &matching.len().to_string())
        }
        ("GET", ["items", id]) => match find_item(id) {
            Some(row) => Reply::json(200, &item_json(tenant, row).to_string()),
            None => Reply::problem(404, "NOT_FOUND", "The requested item does not exist"),
        },
        ("POST", ["items"]) => {
            let Ok(new) = serde_json::from_slice::<serde_json::Value>(&request.body) else {
                return Reply::problem(400, "BAD_BODY", "body must be JSON");
            };
            let name = new["name"].as_str().unwrap_or_default();
            if name.is_empty() {
                return Reply::problem(422, "EMPTY_NAME", "name must not be empty");
            }
            let tags = new.get("tags").cloned().unwrap_or(serde_json::json!([]));
            Reply::json(
                201,
                &serde_json::json!({ "id": "item-new", "name": name, "tags": tags }).to_string(),
            )
        }
        ("DELETE", ["items", id]) => match find_item(id) {
            Some(_) => Reply::new(204),
            None => Reply::problem(404, "NOT_FOUND", "The requested item does not exist"),
        },
        ("PUT", ["items", id, "attachment"]) => match find_item(id) {
            Some(_) => {
                let note = request.header("x-client-note");
                let receipt = serde_json::json!({ "bytes_received": request.body.len(), "client_note": note });
                Reply::json(200, &receipt.to_string())
            }
            None => Reply::problem(404, "NOT_FOUND", "The requested item does not exist"),
        },
        ("GET", ["exports", export]) => match *export {
            "items-2026" => Reply::new(200)
                .header("content-type", "text/csv; charset=utf-8")
                .header("x-export-id", export)
                .body(format!(
                    "id,name\nitem-001,Notebook\nitem-002,Pen\n# tenant {}\n",
                    tenant.len()
                )),
            _ => Reply::problem(404, "NOT_FOUND", "no such export"),
        },
        _ => Reply::problem(404, "NOT_FOUND", "unknown route"),
    }
}

/// Seconds since the Unix epoch of a whole-second `YYYY-MM-DDTHH:MM:SSZ`, if it is one.
pub fn rfc3339_seconds(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return None;
    }
    let num = |r: std::ops::Range<usize>| text.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hh, mm, ss) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 59 {
        return None;
    }
    // Days from civil (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3_600 + mm * 60 + ss)
}

/// The fake Ledger service mounted at `prefix` (for example `/ledger/v1`).
///
/// | Request | Answer |
/// |---|---|
/// | any request without `authorization` | `401` |
/// | `GET {prefix}/entries?account=acct-XXXX[&from_ms=n][&page_size=1..50][&page_token=..]` | `200 application/vnd.demo.ledger+json` `{"data":[..],"paging":{"next":..}}` and `x-record-count` |
/// | a malformed account, `from_ms`, `page_size` or token | `400` |
pub fn ledger(prefix: &'static str) -> impl Fn(&Recorded) -> Reply + Send + Sync + 'static {
    move |request| ledger_answer(prefix, request)
}

fn ledger_answer(prefix: &str, request: &Recorded) -> Reply {
    if request.header_all("authorization").is_empty() {
        return Reply::problem(401, "UNAUTHENTICATED", "authorization is required");
    }
    if request.method != "GET"
        || request.path() != format!("{}/entries", prefix.trim_end_matches('/'))
    {
        return Reply::problem(404, "NOT_FOUND", "unknown route");
    }
    let one = |key: &str| -> Result<Option<String>, Reply> {
        match request.query_all(key).as_slice() {
            [] => Ok(None),
            [v] => Ok(Some(v.clone())),
            _ => Err(Reply::problem(400, "REPEATED", "a query key is repeated")),
        }
    };
    let result = (|| {
        let account = one("account")?
            .ok_or_else(|| Reply::problem(400, "BAD_ACCOUNT", "account is required"))?;
        let valid_account = account.strip_prefix("acct-").is_some_and(|hex| {
            hex.len() >= 4
                && hex
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        });
        if !valid_account {
            return Err(Reply::problem(
                400,
                "BAD_ACCOUNT",
                "account must be acct- and lowercase hex",
            ));
        }
        let from_ms = match one("from_ms")? {
            None => 0,
            Some(v) => v.parse::<u64>().map_err(|_| {
                Reply::problem(400, "BAD_FROM", "from_ms must be nonnegative milliseconds")
            })?,
        };
        let page_size = match one("page_size")? {
            None => 10,
            Some(v) => match v.parse::<usize>() {
                Ok(n) if (1..=50).contains(&n) => n,
                _ => {
                    return Err(Reply::problem(
                        400,
                        "BAD_PAGE_SIZE",
                        "page_size must be 1 through 50",
                    ));
                }
            },
        };
        let start = match one("page_token")? {
            None => 0,
            Some(t) => t
                .strip_prefix("ledger-from-")
                .and_then(|n| n.parse::<usize>().ok())
                .ok_or_else(|| Reply::problem(400, "BAD_TOKEN", "unknown page token"))?,
        };
        Ok((account, from_ms, page_size, start))
    })();
    let (account, from_ms, page_size, start) = match result {
        Ok(parsed) => parsed,
        Err(reply) => return reply,
    };
    let matching: Vec<_> = data::ENTRIES
        .iter()
        .filter(|(_, a, ms, _)| *a == account && *ms >= from_ms)
        .collect();
    let page: Vec<_> = matching
        .iter()
        .skip(start)
        .take(page_size)
        .map(|(id, a, ms, amount)| serde_json::json!({ "id": id, "account": a, "at_ms": ms, "amount_minor": amount }))
        .collect();
    let next =
        (start + page_size < matching.len()).then(|| format!("ledger-from-{}", start + page_size));
    Reply::new(200)
        .header("content-type", "application/vnd.demo.ledger+json")
        .header("x-record-count", &matching.len().to_string())
        .body(serde_json::json!({ "data": page, "paging": { "next": next } }).to_string())
}

/// Starts a server that answers `/catalog/v1/...` as [`catalog`] and `/ledger/v1/...` as
/// [`ledger`]; anything else is a `404` problem.
pub async fn services() -> Server {
    let catalog = catalog("/catalog/v1");
    let ledger = ledger("/ledger/v1");
    Server::start(move |request| {
        if request.path().starts_with("/ledger/") {
            ledger(request)
        } else if request.path().starts_with("/catalog/") {
            catalog(request)
        } else {
            Reply::problem(404, "NOT_FOUND", "unknown service")
        }
    })
    .await
}
