//! Hand-written reqwest equivalents of [`crate::apis`]: the same URLs, query strings, headers and
//! bodies, written the way a careful application author would (values stored as owned Rust
//! values, client-level header values validated once into `HeaderValue`s).
//!
//! The S1–S3 equivalents live in `benches/baselines`.

use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderName, HeaderValue, USER_AGENT};
use reqwest::{Client, Method, Request, RequestBuilder, Url};

fn with_segments(base: &Url, segments: &[&str]) -> Url {
    let mut url = base.clone();
    url.path_segments_mut()
        .expect("http(s) base")
        .pop_if_empty()
        .extend(segments);
    url
}

// ---------------------------------------------------------------------------------------------
// Minimal: S1 `GET health` (the S1 baseline only exposes `async` sending)

/// Hand-written minimal request.
#[derive(Clone, Debug)]
pub struct Simple {
    http: Client,
    base: Url,
}

impl Simple {
    /// The client.
    pub fn new(http: Client, base: &str) -> Self {
        Self {
            http,
            base: Url::parse(base).expect("valid base"),
        }
    }

    /// `GET health`, with the `Accept` that Concord's `Json` suggests for a typed JSON output.
    pub fn health(&self) -> reqwest::Result<Request> {
        self.http
            .get(with_segments(&self.base, &["health"]))
            .header(
                reqwest::header::ACCEPT,
                HeaderValue::from_static("application/json"),
            )
            .build()
    }
}

// ---------------------------------------------------------------------------------------------
// Realistic: tracker

/// Hand-written tracker client.
#[derive(Clone, Debug)]
pub struct Tracker {
    http: Client,
    base: Url,
    token: HeaderValue,
    api_version: HeaderValue,
}

impl Tracker {
    /// Validates the client-level header values once.
    pub fn new(http: Client, base: &str, token: &str) -> Self {
        let mut token = HeaderValue::from_str(token).expect("valid token");
        token.set_sensitive(true);
        Self {
            http,
            base: Url::parse(base).expect("valid base"),
            token,
            api_version: HeaderValue::from_static("2024-06-01"),
        }
    }

    /// `orgs/{org}` scope.
    pub fn org(&self, org: impl Into<String>) -> TrackerOrg<'_> {
        TrackerOrg {
            api: self,
            org: org.into(),
        }
    }
}

/// `orgs/{org}`.
#[derive(Clone, Debug)]
pub struct TrackerOrg<'a> {
    api: &'a Tracker,
    org: String,
}

impl<'a> TrackerOrg<'a> {
    /// `projects/{project}` scope.
    pub fn project(&self, project: impl Into<String>) -> TrackerProject<'a> {
        TrackerProject {
            org: self.clone(),
            project: project.into(),
        }
    }
}

/// `orgs/{org}/projects/{project}`.
#[derive(Clone, Debug)]
pub struct TrackerProject<'a> {
    org: TrackerOrg<'a>,
    project: String,
}

impl<'a> TrackerProject<'a> {
    /// `GET milestones/{milestone}/issues`.
    pub fn issues(&self, milestone: u32) -> Issues<'a> {
        Issues {
            project: self.clone(),
            milestone,
            state: None,
            labels: Vec::new(),
            assignee: None,
            since: None,
            sort: None,
            direction: None,
            include_closed: None,
            page: 1,
            per_page: 50,
        }
    }
}

/// The issue-list request value.
#[derive(Clone, Debug)]
#[allow(missing_docs)]
pub struct Issues<'a> {
    project: TrackerProject<'a>,
    milestone: u32,
    pub state: Option<String>,
    pub labels: Vec<String>,
    pub assignee: Option<String>,
    pub since: Option<u64>,
    pub sort: Option<String>,
    pub direction: Option<String>,
    pub include_closed: Option<bool>,
    pub page: u32,
    pub per_page: u32,
}

impl Issues<'_> {
    /// Builds the native request.
    pub fn build(self) -> reqwest::Result<Request> {
        let p = &self.project;
        let api = p.org.api;
        let milestone = self.milestone.to_string();
        let mut url = with_segments(
            &api.base,
            &[
                "orgs",
                &p.org.org,
                "projects",
                &p.project,
                "milestones",
                &milestone,
                "issues",
            ],
        );
        {
            let mut q = url.query_pairs_mut();
            if let Some(v) = &self.state {
                q.append_pair("state", v);
            }
            for v in &self.labels {
                q.append_pair("label", v);
            }
            if let Some(v) = &self.assignee {
                q.append_pair("assignee", v);
            }
            if let Some(v) = self.since {
                q.append_pair("since", &v.to_string());
            }
            if let Some(v) = &self.sort {
                q.append_pair("sort", v);
            }
            if let Some(v) = &self.direction {
                q.append_pair("direction", v);
            }
            if let Some(v) = self.include_closed {
                q.append_pair("include_closed", if v { "true" } else { "false" });
            }
            q.append_pair("page", &self.page.to_string());
            q.append_pair("per_page", &self.per_page.to_string());
        }
        api.http
            .get(url)
            .header(AUTHORIZATION, api.token.clone())
            .header("x-api-version", api.api_version.clone())
            .header(ACCEPT, HeaderValue::from_static("application/json"))
            .header(USER_AGENT, HeaderValue::from_static("concord-bench/1"))
            .header("x-org", HeaderValue::from_str(&p.org.org).expect("valid"))
            .header("x-request-kind", HeaderValue::from_static("list"))
            .build()
    }
}

// ---------------------------------------------------------------------------------------------
// Headers

/// The header names `x-h00` … `x-h15`.
pub static HEADER_NAMES: [HeaderName; 16] = [
    HeaderName::from_static("x-h00"),
    HeaderName::from_static("x-h01"),
    HeaderName::from_static("x-h02"),
    HeaderName::from_static("x-h03"),
    HeaderName::from_static("x-h04"),
    HeaderName::from_static("x-h05"),
    HeaderName::from_static("x-h06"),
    HeaderName::from_static("x-h07"),
    HeaderName::from_static("x-h08"),
    HeaderName::from_static("x-h09"),
    HeaderName::from_static("x-h10"),
    HeaderName::from_static("x-h11"),
    HeaderName::from_static("x-h12"),
    HeaderName::from_static("x-h13"),
    HeaderName::from_static("x-h14"),
    HeaderName::from_static("x-h15"),
];

/// Text header values `v00-header-value` … (also used, parsed, as the static set).
pub static HEADER_TEXT: [&str; 16] = [
    "v00-header-value",
    "v01-header-value",
    "v02-header-value",
    "v03-header-value",
    "v04-header-value",
    "v05-header-value",
    "v06-header-value",
    "v07-header-value",
    "v08-header-value",
    "v09-header-value",
    "v10-header-value",
    "v11-header-value",
    "v12-header-value",
    "v13-header-value",
    "v14-header-value",
    "v15-header-value",
];

/// Numeric header values.
pub const HEADER_NUMBERS: [u64; 16] = [
    1_700_000_000,
    1_700_000_001,
    1_700_000_002,
    1_700_000_003,
    1_700_000_004,
    1_700_000_005,
    1_700_000_006,
    1_700_000_007,
    1_700_000_008,
    1_700_000_009,
    1_700_000_010,
    1_700_000_011,
    1_700_000_012,
    1_700_000_013,
    1_700_000_014,
    1_700_000_015,
];

/// Hand-written header requests against `https://api.example.com/h`.
#[derive(Clone, Debug)]
pub struct Headers {
    http: Client,
    url: Url,
    /// `HeaderValue`s validated once, cloned per request.
    pub prebuilt: Vec<HeaderValue>,
}

impl Headers {
    /// Prebuilds the reusable header values.
    pub fn new(http: Client) -> Self {
        Self {
            http,
            url: Url::parse("https://api.example.com/h").expect("valid"),
            prebuilt: HEADER_TEXT
                .iter()
                .map(|s| HeaderValue::from_static(s))
                .collect(),
        }
    }

    fn get(&self) -> RequestBuilder {
        self.http.get(self.url.clone())
    }

    /// Formats each owned `String` with `HeaderValue::from_str`.
    pub fn text<const N: usize>(&self, values: [String; N]) -> reqwest::Result<Request> {
        let mut b = self.get();
        for (name, v) in HEADER_NAMES.iter().zip(values) {
            b = b.header(name, HeaderValue::from_str(&v).expect("valid"));
        }
        b.build()
    }

    /// `HeaderValue::from(u64)`.
    pub fn numeric<const N: usize>(&self, values: [u64; N]) -> reqwest::Result<Request> {
        let mut b = self.get();
        for (name, v) in HEADER_NAMES.iter().zip(values) {
            b = b.header(name, HeaderValue::from(v));
        }
        b.build()
    }

    /// Clones `n` prebuilt `HeaderValue`s (payload reuse: no formatting, no validation).
    pub fn prebuilt(&self, n: usize) -> reqwest::Result<Request> {
        let mut b = self.get();
        for (name, v) in HEADER_NAMES.iter().zip(&self.prebuilt[..n]) {
            b = b.header(name, v.clone());
        }
        b.build()
    }

    /// `HeaderValue::from_static` per request (no allocation for the value).
    pub fn from_static(&self, n: usize) -> reqwest::Result<Request> {
        let mut b = self.get();
        for (name, v) in HEADER_NAMES.iter().zip(&HEADER_TEXT[..n]) {
            b = b.header(name, HeaderValue::from_static(v));
        }
        b.build()
    }
}

// ---------------------------------------------------------------------------------------------
// Deep chain

/// Hand-written eight-level chain.
#[derive(Clone, Debug)]
pub struct Deep {
    http: Client,
    base: Url,
}

static DEEP_NAMES: [HeaderName; 8] = [
    HeaderName::from_static("x-l1"),
    HeaderName::from_static("x-l2"),
    HeaderName::from_static("x-l3"),
    HeaderName::from_static("x-l4"),
    HeaderName::from_static("x-l5"),
    HeaderName::from_static("x-l6"),
    HeaderName::from_static("x-l7"),
    HeaderName::from_static("x-l8"),
];
const DEEP_LEVELS: [&str; 8] = ["l1", "l2", "l3", "l4", "l5", "l6", "l7", "l8"];

impl Deep {
    /// The client.
    pub fn new(http: Client) -> Self {
        Self {
            http,
            base: Url::parse("https://api.example.com").expect("valid"),
        }
    }

    /// The leaf request from eight owned values (navigated per call).
    pub fn leaf(&self, values: [String; 8]) -> reqwest::Result<Request> {
        let mut url = self.base.clone();
        {
            let mut segs = url.path_segments_mut().expect("http(s) base");
            segs.pop_if_empty();
            for (level, v) in DEEP_LEVELS.iter().zip(&values) {
                segs.push(level).push(v);
            }
            segs.push("leaf");
        }
        let mut b = self.http.get(url);
        for (name, v) in DEEP_NAMES.iter().zip(&values) {
            b = b.header(name, HeaderValue::from_str(v).expect("valid"));
        }
        b.build()
    }

    /// A reusable level-8 scope: URL prefix and header map computed once.
    pub fn scope(&self, values: [String; 8]) -> DeepScope {
        let request = self.leaf(values).expect("valid");
        let mut prefix = request.url().clone();
        prefix.path_segments_mut().expect("http(s) base").pop();
        DeepScope {
            http: self.http.clone(),
            prefix,
            headers: request.headers().clone(),
        }
    }
}

/// A hand-written level-8 scope handle caching its URL prefix and headers.
#[derive(Clone, Debug)]
pub struct DeepScope {
    http: Client,
    prefix: Url,
    headers: reqwest::header::HeaderMap,
}

impl DeepScope {
    /// The leaf request: clone the prefix and the header map.
    pub fn leaf(&self) -> reqwest::Result<Request> {
        let url = with_segments(&self.prefix, &["leaf"]);
        self.http.get(url).headers(self.headers.clone()).build()
    }
}

// ---------------------------------------------------------------------------------------------
// Inherited header operations

/// Hand-written equivalent of `apis::compose`.
#[derive(Clone, Debug)]
pub struct Compose {
    http: Client,
    base: Url,
    a: HeaderValue,
    b: HeaderValue,
    c: HeaderValue,
}

static X_A: HeaderName = HeaderName::from_static("x-a");
static X_B: HeaderName = HeaderName::from_static("x-b");
static X_C: HeaderName = HeaderName::from_static("x-c");

impl Compose {
    /// Client defaults validated once.
    pub fn new(http: Client) -> Self {
        Self {
            http,
            base: Url::parse("https://api.example.com").expect("valid"),
            a: HeaderValue::from_static("client-a"),
            b: HeaderValue::from_static("client-b"),
            c: HeaderValue::from_static("client-c"),
        }
    }

    /// The `teams/{team}` scope; its header value is validated once.
    pub fn team(&self, team: &str) -> Team<'_> {
        Team {
            api: self,
            team: team.to_owned(),
            team_value: HeaderValue::from_str(team).expect("valid"),
        }
    }
}

/// A reusable team scope.
#[derive(Clone, Debug)]
pub struct Team<'a> {
    api: &'a Compose,
    team: String,
    team_value: HeaderValue,
}

/// Which inherited-header operation an endpoint applies.
#[derive(Clone, Debug)]
pub enum Op {
    /// Inherit everything.
    Inherit,
    /// Replace `x-a` when `Some`, inherit when `None`.
    Override(Option<String>),
    /// Add one `x-b` after the inherited ones.
    Append(String),
    /// Drop `x-c`.
    Remove,
}

impl Team<'_> {
    /// Builds the request for `op`.
    pub fn request(&self, op: Op) -> reqwest::Result<Request> {
        let api = self.api;
        let last = match op {
            Op::Inherit => "inherit",
            Op::Override(_) => "override",
            Op::Append(_) => "append",
            Op::Remove => "remove",
        };
        let url = with_segments(&api.base, &["teams", &self.team, last]);
        let mut b = api.http.get(url);
        b = match &op {
            Op::Override(Some(v)) => b.header(&X_A, HeaderValue::from_str(v).expect("valid")),
            _ => b.header(&X_A, api.a.clone()),
        };
        b = b
            .header(&X_B, api.b.clone())
            .header(&X_B, self.team_value.clone());
        if let Op::Append(w) = &op {
            b = b.header(&X_B, HeaderValue::from_str(w).expect("valid"));
        }
        if !matches!(op, Op::Remove) {
            b = b.header(&X_C, api.c.clone());
        }
        b.header(ACCEPT, HeaderValue::from_static("application/json"))
            .build()
    }
}

// ---------------------------------------------------------------------------------------------
// Bodies

/// Hand-written body requests and response decoding.
#[derive(Clone, Debug)]
pub struct Bodies {
    http: Client,
    base: Url,
}

impl Bodies {
    /// The client.
    pub fn new(http: Client, base: &str) -> Self {
        Self {
            http,
            base: Url::parse(base).expect("valid base"),
        }
    }

    /// `POST json` with `RequestBuilder::json`.
    pub fn post_json(&self, body: &crate::payload::Payload) -> reqwest::Result<Request> {
        self.http
            .request(Method::POST, with_segments(&self.base, &["json"]))
            .json(body)
            .build()
    }

    /// `POST text`.
    pub fn post_text(&self, body: String) -> reqwest::Result<Request> {
        self.http
            .request(Method::POST, with_segments(&self.base, &["text"]))
            .header(
                reqwest::header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            )
            .body(body)
            .build()
    }

    /// `POST bytes`.
    pub fn post_bytes(&self, body: bytes::Bytes) -> reqwest::Result<Request> {
        self.http
            .request(Method::POST, with_segments(&self.base, &["bytes"]))
            .header(
                reqwest::header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            )
            .body(body)
            .build()
    }

    /// `GET json` over the network.
    pub async fn get_json(&self) -> reqwest::Result<crate::payload::Payload> {
        let response = self
            .http
            .get(with_segments(&self.base, &["json"]))
            .send()
            .await?;
        response.error_for_status()?.json().await
    }
}

/// Hand-written decoding of an already-received response.
pub mod decode {
    /// `error_for_status` + `Response::json`.
    pub async fn json(response: reqwest::Response) -> reqwest::Result<crate::payload::Payload> {
        response.error_for_status()?.json().await
    }

    /// `error_for_status` + `Response::text`.
    pub async fn text(response: reqwest::Response) -> reqwest::Result<String> {
        response.error_for_status()?.text().await
    }

    /// `error_for_status` + `Response::bytes`.
    pub async fn bytes(response: reqwest::Response) -> reqwest::Result<bytes::Bytes> {
        response.error_for_status()?.bytes().await
    }
}

// ---------------------------------------------------------------------------------------------
// Native header names: six inherited headers, a replaced one, and a Riot-shaped request.

static KEY_A: HeaderName = HeaderName::from_static("x-a");
static KEY_B: HeaderName = HeaderName::from_static("x-b");
static KEY_C: HeaderName = HeaderName::from_static("x-c");
static X_RIOT_TOKEN: HeaderName = HeaderName::from_static("x-riot-token");

/// Hand-written six-header client: client values validated once.
#[derive(Clone, Debug)]
pub struct Keys {
    http: Client,
    base: Url,
    token: HeaderValue,
    a: HeaderValue,
}

impl Keys {
    /// Validates the client values once.
    pub fn new(http: Client, token: &str, a: &str) -> Self {
        let mut token = HeaderValue::from_str(token).expect("valid");
        token.set_sensitive(true);
        Self {
            http,
            base: Url::parse("https://api.example.com").expect("valid"),
            token,
            a: HeaderValue::from_str(a).expect("valid"),
        }
    }

    /// `GET teams/{team}/items`, with `x-a` replaced when `v` is given.
    pub fn items(&self, team: &str, v: Option<&str>) -> reqwest::Result<Request> {
        let a = match v {
            Some(v) => HeaderValue::from_str(v).expect("valid"),
            None => self.a.clone(),
        };
        self.http
            .get(with_segments(&self.base, &["teams", team, "items"]))
            .header(AUTHORIZATION, self.token.clone())
            .header(KEY_A.clone(), a)
            .header(ACCEPT, HeaderValue::from_static("application/json"))
            .header(USER_AGENT, HeaderValue::from_static("concord-bench/1"))
            .header(KEY_B.clone(), HeaderValue::from_static("b"))
            .header(KEY_C.clone(), HeaderValue::from_static("c"))
            .build()
    }
}

/// Hand-written Riot-shaped client.
#[derive(Clone, Debug)]
pub struct Riot {
    http: Client,
    key: HeaderValue,
}

impl Riot {
    /// Validates the key once.
    pub fn new(http: Client, key: &str) -> Self {
        let mut key = HeaderValue::from_str(key).expect("valid");
        key.set_sensitive(true);
        Self { http, key }
    }

    /// `GET https://{route}.api.riotgames.com/lol/match/v5/matches/by-puuid/{puuid}/ids?count=..`.
    pub fn ids(&self, route: &str, puuid: &str, count: Option<u32>) -> reqwest::Result<Request> {
        let mut url = Url::parse(&format!("https://{route}.api.riotgames.com")).expect("valid");
        url.path_segments_mut()
            .expect("http(s) base")
            .pop_if_empty()
            .extend(["lol", "match", "v5", "matches", "by-puuid", puuid, "ids"]);
        if let Some(count) = count {
            url.query_pairs_mut()
                .append_pair("count", &count.to_string());
        }
        RequestBuilder::from_parts(self.http.clone(), Request::new(Method::GET, url))
            .header(X_RIOT_TOKEN.clone(), self.key.clone())
            .header(ACCEPT, HeaderValue::from_static("application/json"))
            .build()
    }
}
