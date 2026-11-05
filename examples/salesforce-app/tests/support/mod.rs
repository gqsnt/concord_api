//! Shared test support: two synthetic orgs routed to one loopback server, a request router,
//! fixtures, and an independent server-side CSV generator (Salesforce-style, every field
//! quoted) so that the client's own encoder never produces the bytes it is tested against.

#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use concord::reqwest;
use concord_example_salesforce_sdk::OrgContext;
use concord_example_salesforce_sdk::bulk::{BulkDialect, ColumnDelimiter, LineEnding};
use concord_example_salesforce_sdk::ids::{AccessToken, ApiVersion, InstanceUrl};
use concord_example_testserver::{Recorded, Reply, TestServer};

pub const VERSION: &str = "v68.0";

/// A synthetic org: its instance host and token. The tokens are synthetic secrets that the
/// redaction tests look for.
#[derive(Clone, Copy, Debug)]
pub struct Org {
    pub host: &'static str,
    pub token: &'static str,
}

pub const ORG_A: Org = Org {
    host: "org-a.my.salesforce.test",
    token: "00DA0000SECRET!tokenA",
};
pub const ORG_B: Org = Org {
    host: "org-b.my.salesforce.test",
    token: "00DB0000SECRET!tokenB",
};

pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub fn fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name)).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

/// A reqwest client resolving both org hosts to the server.
pub fn client(server: &TestServer) -> reqwest::Client {
    reqwest::Client::builder()
        .resolve(ORG_A.host, server.addr())
        .resolve(ORG_B.host, server.addr())
        .build()
        .unwrap()
}

pub fn instance(org: Org, server: &TestServer) -> String {
    format!("http://{}:{}", org.host, server.addr().port())
}

pub fn ctx(org: Org, server: &TestServer) -> OrgContext {
    OrgContext::new(
        client(server),
        InstanceUrl::parse(&instance(org, server)).unwrap(),
        ApiVersion::PINNED,
        AccessToken::new(org.token).unwrap(),
    )
    .unwrap()
}

/// `/services/data/v68.0/{rest}`.
pub fn api(rest: &str) -> String {
    format!("/services/data/{VERSION}/{rest}")
}

type Factory = Box<dyn FnMut(&Recorded) -> Reply + Send>;
type RouteKey = (String, String, String);
type Routes = HashMap<RouteKey, VecDeque<Factory>>;

/// Routes requests by (host without port, method, raw target). Each route is a queue of reply
/// factories; the last one repeats. An unrouted request gets 599 and is counted.
#[derive(Clone, Default)]
pub struct Router {
    routes: Arc<Mutex<Routes>>,
    unrouted: Arc<AtomicU64>,
}

impl Router {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a reply (queued after earlier ones for the same route).
    pub fn on(
        &self,
        org: Org,
        method: &str,
        target: &str,
        reply: impl FnMut(&Recorded) -> Reply + Send + 'static,
    ) -> &Self {
        self.routes
            .lock()
            .unwrap()
            .entry((org.host.to_owned(), method.to_owned(), target.to_owned()))
            .or_default()
            .push_back(Box::new(reply));
        self
    }

    /// Exact target first, then the path alone (a route registered without a query string
    /// matches any query; its factory inspects it).
    pub fn respond(&self, request: &Recorded) -> Reply {
        let host = request.host.split(':').next().unwrap_or("").to_owned();
        let exact = (host.clone(), request.method.clone(), request.target.clone());
        let by_path = (host, request.method.clone(), request.path().to_owned());
        let mut routes = self.routes.lock().unwrap();
        let key = if routes.contains_key(&exact) {
            exact
        } else {
            by_path
        };
        match routes.get_mut(&key) {
            Some(queue) if queue.len() > 1 => {
                let mut factory = queue.pop_front().unwrap();
                factory(request)
            }
            Some(queue) if queue.len() == 1 => {
                let factory = &mut queue[0];
                factory(request)
            }
            _ => {
                self.unrouted.fetch_add(1, Ordering::SeqCst);
                Reply::text(599, "text/plain", format!("no route for {key:?}"))
            }
        }
    }

    pub fn unrouted(&self) -> u64 {
        self.unrouted.load(Ordering::SeqCst)
    }

    pub async fn start(&self) -> TestServer {
        let router = self.clone();
        TestServer::start(move |request| router.respond(request)).await
    }
}

pub fn json(body: impl Into<String>) -> impl FnMut(&Recorded) -> Reply + Send + 'static {
    let body = body.into();
    move |_| Reply::json(200, body.as_bytes())
}

pub fn status_json(
    status: u16,
    body: impl Into<String>,
) -> impl FnMut(&Recorded) -> Reply + Send + 'static {
    let body = body.into();
    move |_| Reply::json(status, body.as_bytes())
}

/// Asserts the org's host and bearer token on every request to it, and that no request carried
/// the other org's token.
pub fn assert_org_isolation(requests: &[Recorded]) {
    for r in requests {
        let host = r.host.split(':').next().unwrap();
        let auth = r.header("authorization").unwrap_or("");
        let expected = if host == ORG_A.host {
            ORG_A.token
        } else if host == ORG_B.host {
            ORG_B.token
        } else {
            panic!("request to an unexpected host {host}");
        };
        assert_eq!(
            auth,
            format!("Bearer {expected}"),
            "{} {}",
            r.method,
            r.target
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Bulk CSV: canonical rows and an independent generator
// ---------------------------------------------------------------------------------------------

/// A canonical export row: Id, FirstName, LastName, Email, AccountId, Loyalty_Tier__c,
/// HasOptedOutOfEmail (empty string for a field Salesforce would leave empty; the boolean is
/// never empty).
pub type Row = [String; 7];

pub const HEADER: [&str; 7] = [
    "Id",
    "FirstName",
    "LastName",
    "Email",
    "AccountId",
    "Loyalty_Tier__c",
    "HasOptedOutOfEmail",
];

/// The canonical rows of part `ordinal` (3 rows per part, with the tricky values spread over
/// the parts: every Bulk delimiter inside a value, quoted LF and CRLF, doubled quotes, UTF-8,
/// empty optional fields).
pub fn part_rows(ordinal: u64) -> Vec<Row> {
    (0..3)
        .map(|i| {
            let n = ordinal * 3 + i;
            let id = format!("003A{n:011}AAA");
            let first = match n % 4 {
                0 => String::new(),
                1 => "Zoë".to_owned(),
                2 => "Ann-Marie".to_owned(),
                _ => "José".to_owned(),
            };
            let last = match n % 5 {
                0 => "all,delims;in|one^val`ue\there".to_owned(),
                1 => "line one\nline two".to_owned(),
                2 => "said \"hi\" twice \"\"".to_owned(),
                3 => "crlf\r\ninside".to_owned(),
                _ => format!("Plain{n}"),
            };
            let email = if n.is_multiple_of(3) {
                String::new()
            } else {
                format!("c{n}@example.test")
            };
            let account = if n.is_multiple_of(2) {
                format!("001A{n:011}AAA")
            } else {
                String::new()
            };
            let tier = match n % 3 {
                0 => "Gold".to_owned(),
                1 => String::new(),
                _ => "Silver, Plus".to_owned(),
            };
            let opted_out = if n % 4 == 1 { "true" } else { "false" }.to_owned();
            [id, first, last, email, account, tier, opted_out]
        })
        .collect()
}

pub fn ending(dialect: BulkDialect) -> &'static str {
    match dialect.line_ending {
        LineEnding::Lf => "\n",
        LineEnding::Crlf => "\r\n",
    }
}

/// Salesforce-style CSV (every field quoted, quotes doubled, the dialect's delimiter and line
/// ending). `empty_unquoted` writes empty fields without quotes, the other plausible form.
pub fn server_csv(
    dialect: BulkDialect,
    header: &[&str],
    rows: &[Row],
    empty_unquoted: bool,
) -> String {
    let d = dialect.delimiter.byte() as char;
    let eol = ending(dialect);
    let field = |v: &str| {
        if v.is_empty() && empty_unquoted {
            String::new()
        } else {
            format!("\"{}\"", v.replace('"', "\"\""))
        }
    };
    let mut out = String::new();
    out.push_str(
        &header
            .iter()
            .map(|h| field(h))
            .collect::<Vec<_>>()
            .join(&d.to_string()),
    );
    out.push_str(eol);
    for row in rows {
        out.push_str(
            &row.iter()
                .map(|v| field(v))
                .collect::<Vec<_>>()
                .join(&d.to_string()),
        );
        out.push_str(eol);
    }
    out
}

pub const QUERY_JOB: &str = "750A00000000001AAA";
pub const INGEST_JOB: &str = "750A00000000002AAA";

pub fn delimiter_name(d: ColumnDelimiter) -> &'static str {
    match d {
        ColumnDelimiter::Backquote => "BACKQUOTE",
        ColumnDelimiter::Caret => "CARET",
        ColumnDelimiter::Comma => "COMMA",
        ColumnDelimiter::Pipe => "PIPE",
        ColumnDelimiter::Semicolon => "SEMICOLON",
        ColumnDelimiter::Tab => "TAB",
    }
}

pub fn ending_name(e: LineEnding) -> &'static str {
    match e {
        LineEnding::Lf => "LF",
        LineEnding::Crlf => "CRLF",
    }
}

/// A query job document (the "Get Information About a Query Job" shape) in `state`.
pub fn query_job(state: &str, dialect: BulkDialect) -> String {
    serde_json::json!({
        "id": QUERY_JOB,
        "operation": "query",
        "object": "Contact",
        "createdById": "005A00000000001AAA",
        "createdDate": "2026-11-04T10:00:00.000+0000",
        "systemModstamp": "2026-11-04T10:00:05.000+0000",
        "state": state,
        "concurrencyMode": "Parallel",
        "contentType": "CSV",
        "apiVersion": 68.0,
        "jobType": "V2Query",
        "lineEnding": ending_name(dialect.line_ending),
        "columnDelimiter": delimiter_name(dialect.delimiter),
        "numberRecordsProcessed": 21,
        "retries": 0,
        "totalProcessingTime": 334,
        "isPkChunkingSupported": true
    })
    .to_string()
}

/// A result-pages manifest listing `ordinals` (locator `loc{n}`), with the next manifest's
/// locator under `next_field` (`nextRecordsUrl` or `nextRecordUrl`).
pub fn result_pages(
    ordinals: std::ops::Range<u64>,
    next: Option<&str>,
    next_field: &str,
) -> String {
    let pages: Vec<_> = ordinals
        .map(|n| {
            serde_json::json!({
                "resultUrl": api(&format!("jobs/query/{QUERY_JOB}/results?locator=loc{n}"))
            })
        })
        .collect();
    let mut doc = serde_json::json!({ "resultPages": pages, "done": next.is_none() });
    doc[next_field] = match next {
        Some(locator) => serde_json::Value::String(api(&format!(
            "jobs/query/{QUERY_JOB}/resultpages?locator={locator}"
        ))),
        None => serde_json::Value::Null,
    };
    doc.to_string()
}

pub fn result_target(ordinal: u64) -> String {
    api(&format!(
        "jobs/query/{QUERY_JOB}/results?locator=loc{ordinal}"
    ))
}

/// A result part reply: the canonical rows of `ordinal` in `dialect`, with
/// `Sforce-NumberOfRecords` and `Sforce-Locator: null` (unused by the parallel path).
pub fn part_reply(dialect: BulkDialect, ordinal: u64) -> Reply {
    let rows = part_rows(ordinal);
    let body = server_csv(dialect, &HEADER, &rows, ordinal % 2 == 1);
    Reply::text(200, "text/csv; charset=UTF-8", body)
        .header("sforce-numberofrecords", rows.len().to_string())
        .header("sforce-locator", "null")
}

/// Every canonical row of parts `0..parts`, in order.
pub fn all_rows(parts: u64) -> Vec<Row> {
    (0..parts).flat_map(part_rows).collect()
}

/// A fresh temporary directory under the target directory (not /tmp, which may be small),
/// removed by the guard.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let base = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("sf-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        Self(base)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
