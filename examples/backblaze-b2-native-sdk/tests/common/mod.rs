//! A local stand-in for the four B2 hosts (account, API, upload, download), each its own
//! loopback server, sharing one in-memory bucket. Responses are built from the fixtures in
//! `fixtures/` (see PROVENANCE.md); the stand-in checks what it receives against fixture values
//! and the documented request rules, never against the SDK's own computations.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_backblaze_b2_native::concord::reqwest;
use concord_sdk_backblaze_b2_native::{ApplicationKey, BucketId, Client, Session};
use serde_json::{Value, json};

pub fn fixture(name: &str) -> String {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

pub fn fixture_json(name: &str) -> Value {
    serde_json::from_str(&fixture(name)).unwrap()
}

pub fn hello() -> Vec<u8> {
    fixture("hello-b2.txt").into_bytes()
}

/// `hello-b2.json`: values computed with coreutils/python, not with the SDK.
pub fn hello_facts() -> Value {
    fixture_json("hello-b2.json")
}

pub fn hello_sha1() -> String {
    hello_facts()["sha1"].as_str().unwrap().to_owned()
}

pub fn credentials() -> Value {
    fixture_json("credentials.json")
}

pub fn key() -> ApplicationKey {
    let c = credentials();
    ApplicationKey::new(
        c["keyId"].as_str().unwrap(),
        c["applicationKey"].as_str().unwrap(),
    )
    .unwrap()
}

/// The account token of the documented `b2_authorize_account` example.
pub const ACCOUNT_TOKEN: &str = "AUTHORIZATION_TOKEN";
/// The bucket of the documented `b2_get_upload_url` example.
pub const BUCKET: &str = "4a48fe8875c6214145260818";

pub fn bucket() -> BucketId {
    BucketId::new(BUCKET).unwrap()
}

pub fn http() -> reqwest::Client {
    concord_sdk_backblaze_b2_native::http_client_builder()
        .no_proxy()
        .build()
        .unwrap()
}

pub fn error_reply(status: u16, code: &str) -> Reply {
    let body = json!({ "status": status, "code": code, "message": format!("synthetic {code}") });
    Reply::json(status, body.to_string())
}

/// Percent-decoding written for the stand-in (independent of the SDK's encoder).
pub fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap();
            out.push(u8::from_str_radix(hex, 16).unwrap());
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// A reply built by a test from the recorded request.
pub type Responder = Box<dyn Fn(&Recorded) -> Reply + Send>;

/// What the upload host does with the next upload request.
pub enum UploadMode {
    /// Checks the request, stores the file and answers `200` with its file version.
    Store,
    /// Stores the file, then closes the connection without answering.
    StoreThenHangUp,
    /// Closes the connection without storing or answering.
    HangUp,
    /// Answers this status and B2 error code; stores nothing.
    Fail(u16, &'static str),
    /// Answers with a reply built by the test (nothing stored).
    Reply(Box<dyn FnOnce(&Recorded) -> Reply + Send>),
}

/// How the download host answers.
pub enum DownloadMode {
    /// The stored bytes with the stored headers.
    Faithful,
    /// Other bytes with the stored `X-Bz-Content-Sha1`.
    Tampered,
    /// A reply built by the test.
    Reply(Responder),
}

pub struct StoredFile {
    pub file_id: String,
    pub encoded_name: String,
    pub content_type: String,
    pub sha1: String,
    pub bytes: Vec<u8>,
    pub last_modified: Option<String>,
}

pub struct State {
    pub upload_modes: VecDeque<UploadMode>,
    pub get_upload_url_replies: VecDeque<Reply>,
    pub download_mode: DownloadMode,
    /// Issue path-form upload URLs (`.../b2_upload_file/<bucket>/<cvt>`) instead of query-form.
    pub path_form: bool,
    /// Upload tokens issued and not retired.
    pub live_tokens: Vec<String>,
    pub issued: usize,
    pub stored: Vec<StoredFile>,
    /// Overrides for the authorize document (JSON pointer-ish top-level patch of storageApi).
    pub storage_api_patch: Option<Value>,
    pub authorize_reply: Option<Responder>,
}

pub struct FakeB2 {
    pub auth: TestServer,
    pub api: TestServer,
    pub upload: TestServer,
    pub download: TestServer,
    pub state: Arc<Mutex<State>>,
}

fn header(r: &Recorded, name: &str) -> Option<String> {
    r.header(name).map(str::to_owned)
}

impl FakeB2 {
    pub async fn start() -> Self {
        let state = Arc::new(Mutex::new(State {
            upload_modes: VecDeque::new(),
            get_upload_url_replies: VecDeque::new(),
            download_mode: DownloadMode::Faithful,
            path_form: false,
            live_tokens: Vec::new(),
            issued: 0,
            stored: Vec::new(),
            storage_api_patch: None,
            authorize_reply: None,
        }));

        let s = Arc::clone(&state);
        let upload = TestServer::start(move |r| upload_reply(&s, r)).await;
        let upload_base = upload.url();

        let s = Arc::clone(&state);
        let base = upload_base.clone();
        let api = TestServer::start(move |r| api_reply(&s, r, &base)).await;

        let s = Arc::clone(&state);
        let download = TestServer::start(move |r| download_reply(&s, r)).await;

        let s = Arc::clone(&state);
        let (api_url, download_url) = (api.url(), download.url());
        let auth = TestServer::start(move |r| {
            if let Some(custom) = &s.lock().unwrap().authorize_reply {
                return custom(r);
            }
            authorize_reply(&s, r, &api_url, &download_url)
        })
        .await;
        Self {
            auth,
            api,
            upload,
            download,
            state,
        }
    }

    pub fn client(&self) -> Client {
        Client::new(http()).with_base_url(&self.auth.url()).unwrap()
    }

    pub async fn session(&self) -> Session {
        self.client().authorize_account(&key()).await.unwrap()
    }

    pub fn push_upload(&self, mode: UploadMode) {
        self.state.lock().unwrap().upload_modes.push_back(mode);
    }

    pub fn push_get_upload_url(&self, reply: Reply) {
        self.state
            .lock()
            .unwrap()
            .get_upload_url_replies
            .push_back(reply);
    }

    pub fn set_download(&self, mode: DownloadMode) {
        self.state.lock().unwrap().download_mode = mode;
    }

    pub fn stored_ids(&self) -> Vec<String> {
        let state = self.state.lock().unwrap();
        state.stored.iter().map(|f| f.file_id.clone()).collect()
    }

    /// Upload requests whose body arrived complete.
    pub fn upload_requests(&self) -> Vec<Recorded> {
        self.upload.requests()
    }

    pub fn get_upload_url_requests(&self) -> usize {
        self.api.requests().len()
    }
}

fn authorize_reply(state: &Mutex<State>, r: &Recorded, api: &str, download: &str) -> Reply {
    let expected = credentials()["authorization"].as_str().unwrap().to_owned();
    if r.method != "GET" || r.path() != "/b2api/v4/b2_authorize_account" {
        return error_reply(400, "bad_request");
    }
    if r.header("authorization") != Some(expected.as_str()) {
        return error_reply(401, "unauthorized");
    }
    // The documented example, with its two issued URLs pointing at the local hosts.
    let mut doc = fixture_json("authorize_account.json");
    doc["apiInfo"]["storageApi"]["apiUrl"] = Value::String(api.to_owned());
    doc["apiInfo"]["storageApi"]["downloadUrl"] = Value::String(download.to_owned());
    if let Some(Value::Object(patch)) = state.lock().unwrap().storage_api_patch.clone() {
        for (k, v) in patch {
            doc["apiInfo"]["storageApi"][k] = v;
        }
    }
    Reply::json(200, doc.to_string())
}

fn api_reply(state: &Mutex<State>, r: &Recorded, upload_base: &str) -> Reply {
    if r.method != "GET" || r.path() != "/b2api/v4/b2_get_upload_url" {
        return error_reply(400, "bad_request");
    }
    if r.header("authorization") != Some(ACCOUNT_TOKEN) {
        return error_reply(401, "bad_auth_token");
    }
    let mut state = state.lock().unwrap();
    if let Some(reply) = state.get_upload_url_replies.pop_front() {
        return reply;
    }
    let Some(bucket) = r.query_all("bucketId").into_iter().next() else {
        return error_reply(400, "bad_request");
    };
    if bucket != BUCKET {
        return error_reply(400, "bad_bucket_id");
    }
    state.issued += 1;
    let n = state.issued;
    let token = format!("2_upload_token_{n}_upld");
    state.live_tokens.push(token.clone());
    let cvt = format!("c001_v0001005_t{n:04}");
    let url = if state.path_form {
        format!("{upload_base}/b2api/v4/b2_upload_file/{bucket}/{cvt}")
    } else {
        format!("{upload_base}/b2api/v4/b2_upload_file?cvt={cvt}&bucket={bucket}")
    };
    let mut doc = fixture_json("get_upload_url.json");
    doc["bucketId"] = Value::String(bucket);
    doc["uploadUrl"] = Value::String(url);
    doc["authorizationToken"] = Value::String(token);
    Reply::json(200, doc.to_string())
}

/// The documented request rules the stand-in enforces: POST, a live upload token, the name, a
/// media type, a SHA-1, `Content-Length` equal to the body, no chunked transfer.
fn check_upload(state: &State, r: &Recorded) -> Result<(), Reply> {
    if r.method != "POST" {
        return Err(error_reply(405, "method_not_allowed"));
    }
    let token = r.header("authorization").unwrap_or_default();
    if !state.live_tokens.iter().any(|t| t == token) {
        return Err(error_reply(401, "bad_auth_token"));
    }
    if r.chunked || r.header("content-length").is_none() {
        return Err(error_reply(411, "length_required"));
    }
    if r.header("content-length") != Some(r.body_len.to_string().as_str()) {
        return Err(error_reply(400, "bad_request"));
    }
    for required in ["x-bz-file-name", "x-bz-content-sha1", "content-type"] {
        if r.header(required).is_none() {
            return Err(error_reply(400, "bad_request"));
        }
    }
    Ok(())
}

fn store(state: &mut State, r: &Recorded) -> Value {
    let n = state.stored.len() + 1;
    let file_id =
        format!("4_h{BUCKET}_f00000000000047{n:02}_d20140104_m032022_c001_v0000123_t0104");
    let encoded_name = header(r, "x-bz-file-name").unwrap();
    let name = String::from_utf8(percent_decode(&encoded_name)).unwrap();
    let sha1 = header(r, "x-bz-content-sha1").unwrap();
    let content_type = header(r, "content-type").unwrap();
    state.stored.push(StoredFile {
        file_id: file_id.clone(),
        encoded_name,
        content_type: content_type.clone(),
        sha1: sha1.clone(),
        bytes: r.body.clone(),
        last_modified: header(r, "x-bz-info-src_last_modified_millis"),
    });
    let bucket = r
        .query_all("bucket")
        .into_iter()
        .next()
        .unwrap_or_else(|| r.path().split('/').nth(4).unwrap_or_default().to_owned());
    // The assembled documented file version, with what was received.
    let mut doc = fixture_json("upload_file.json");
    doc["fileId"] = Value::String(file_id);
    doc["fileName"] = Value::String(name);
    doc["bucketId"] = Value::String(bucket);
    doc["contentLength"] = Value::String(r.body_len.to_string());
    doc["contentSha1"] = Value::String(sha1);
    doc["contentType"] = Value::String(content_type);
    doc
}

fn upload_reply(state: &Mutex<State>, r: &Recorded) -> Reply {
    let mut state = state.lock().unwrap();
    let mode = state.upload_modes.pop_front().unwrap_or(UploadMode::Store);
    let token = r.header("authorization").unwrap_or_default().to_owned();
    if let UploadMode::Reply(build) = mode {
        state.live_tokens.retain(|t| *t != token);
        return build(r);
    }
    if let Err(reply) = check_upload(&state, r) {
        state.live_tokens.retain(|t| *t != token);
        return reply;
    }
    match mode {
        UploadMode::Store => Reply::json(200, store(&mut state, r).to_string()),
        UploadMode::StoreThenHangUp => {
            store(&mut state, r);
            state.live_tokens.retain(|t| *t != token);
            Reply::hang_up()
        }
        UploadMode::HangUp => {
            state.live_tokens.retain(|t| *t != token);
            Reply::hang_up()
        }
        UploadMode::Fail(status, code) => {
            // B2: the URL and token are valid "until the endpoint rejects an upload".
            state.live_tokens.retain(|t| *t != token);
            error_reply(status, code)
        }
        UploadMode::Reply(_) => unreachable!(),
    }
}

fn download_reply(state: &Mutex<State>, r: &Recorded) -> Reply {
    let state = state.lock().unwrap();
    if let DownloadMode::Reply(build) = &state.download_mode {
        return build(r);
    }
    if r.method != "GET" || r.path() != "/b2api/v4/b2_download_file_by_id" {
        return error_reply(400, "bad_request");
    }
    if r.header("authorization") != Some(ACCOUNT_TOKEN) {
        return error_reply(401, "bad_auth_token");
    }
    let id = r.query_all("fileId").into_iter().next().unwrap_or_default();
    let Some(file) = state.stored.iter().find(|f| f.file_id == id) else {
        return error_reply(404, "not_found");
    };
    let body = match state.download_mode {
        DownloadMode::Tampered => hello_facts()["tamperedBody"]
            .as_str()
            .unwrap()
            .as_bytes()
            .to_vec(),
        _ => file.bytes.clone(),
    };
    let mut reply = Reply::new(200)
        .header("content-type", file.content_type.clone())
        .header("x-bz-file-id", file.file_id.clone())
        .header("x-bz-file-name", file.encoded_name.clone())
        .header("x-bz-content-sha1", file.sha1.clone())
        .header("x-bz-upload-timestamp", "1559585435000");
    if let Some(millis) = &file.last_modified {
        reply = reply.header("x-bz-info-src_last_modified_millis", millis.clone());
    }
    reply.body(body)
}
