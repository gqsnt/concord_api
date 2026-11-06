#![allow(dead_code)] // Shared by two test binaries; each uses part of it.

//! Test support: fixtures, the transport, scripted servers, and a deterministic fake registry
//! with a separate fake token service (two loopback origins).
//!
//! The fake registry is not Distribution: it implements the documented behaviour this SDK relies
//! on (the Bearer challenge, `n`/`last`/`Link` listings, manifests by tag or digest, blobs, and
//! uploads whose `Location` path and opaque `_state` query change after every chunk) and records
//! every request. It routes on the percent-decoded path, as Go's `net/http` does for
//! Distribution.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::reqwest;
use concord_example_testserver::{Body, Recorded, Reply, Script, TestServer};
use concord_sdk_distribution_registry::{Digest, Registry, TokenAuth, concord};

pub const MANIFEST_DOCKER: &[u8] = include_bytes!("../../fixtures/manifest-docker-v2.json");
pub const MANIFEST_OCI: &[u8] = include_bytes!("../../fixtures/manifest-oci.json");
pub const CONFIG: &[u8] = include_bytes!("../../fixtures/config.json");
pub const LAYER: &[u8] = include_bytes!("../../fixtures/layer.bin");
pub const CATALOG_1: &[u8] = include_bytes!("../../fixtures/catalog-page-1.json");
pub const CATALOG_2: &[u8] = include_bytes!("../../fixtures/catalog-page-2.json");
pub const TAGS_1: &[u8] = include_bytes!("../../fixtures/tags-page-1.json");
pub const TAGS_2: &[u8] = include_bytes!("../../fixtures/tags-page-2.json");
pub const TAGS_NULL: &[u8] = include_bytes!("../../fixtures/tags-null.json");
pub const TAGS_OTHER_NAME: &[u8] = include_bytes!("../../fixtures/tags-other-name.json");
pub const UNAUTHORIZED: &[u8] = include_bytes!("../../fixtures/token-spec-unauthorized.json");
pub const TOKEN: &[u8] = include_bytes!("../../fixtures/token.json");
pub const TOKEN_ACCESS: &[u8] = include_bytes!("../../fixtures/token-access-token.json");
pub const TOKEN_MISSING: &[u8] = include_bytes!("../../fixtures/token-missing.json");
pub const MANIFEST_UNKNOWN: &[u8] = include_bytes!("../../fixtures/error-manifest-unknown.json");
pub const BLOB_UNKNOWN: &[u8] = include_bytes!("../../fixtures/error-blob-unknown.json");
pub const MANIFEST_BLOBS_UNKNOWN: &[u8] =
    include_bytes!("../../fixtures/error-manifest-blobs-unknown.json");
pub const DIGEST_INVALID: &[u8] = include_bytes!("../../fixtures/error-digest-invalid.json");
pub const NAME_UNKNOWN: &[u8] = include_bytes!("../../fixtures/error-name-unknown.json");
pub const DENIED: &[u8] = include_bytes!("../../fixtures/error-denied.json");
pub const UNSUPPORTED: &[u8] = include_bytes!("../../fixtures/error-unsupported.json");
pub const PAGINATION_INVALID: &[u8] =
    include_bytes!("../../fixtures/error-pagination-number-invalid.json");
pub const UPLOAD_UNKNOWN: &[u8] = include_bytes!("../../fixtures/error-blob-upload-unknown.json");
pub const RANGE_INVALID: &[u8] = include_bytes!("../../fixtures/error-range-invalid.json");
pub const UNKNOWN_CODE: &[u8] = include_bytes!("../../fixtures/error-unknown-code.json");
pub const PROXY_ERROR: &[u8] = include_bytes!("../../fixtures/proxy-error.html");

/// The token inside `token.json` (synthetic: a readable label, not a JWT, valid nowhere).
pub const TOKEN_VALUE: &str = "synthetic-registry-token-pull-0001";
/// The token inside `token-access-token.json`.
pub const TOKEN_ACCESS_VALUE: &str = "synthetic-registry-token-oauth-0002";
/// Synthetic Basic credentials for the token service.
pub const USERNAME: &str = "fixture-user";
pub const PASSWORD: &str = "synthetic-password-not-a-secret";
/// `Basic base64("fixture-user:synthetic-password-not-a-secret")`.
pub const BASIC: &str = "Basic Zml4dHVyZS11c2VyOnN5bnRoZXRpYy1wYXNzd29yZC1ub3QtYS1zZWNyZXQ=";
/// The `service` the fakes use.
pub const SERVICE: &str = "registry.fake.test";

pub const OCI: &str = "application/vnd.oci.image.manifest.v1+json";
pub const DOCKER_V2: &str = "application/vnd.docker.distribution.manifest.v2+json";

pub fn digest(bytes: &[u8]) -> Digest {
    Digest::sha256_of(bytes)
}

/// The transport every test uses: no proxy, no redirects, no reqwest retry, a short timeout.
pub fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

/// A client for `server`, trusting `auth` as a realm.
pub fn registry(server: &TestServer, auth: Option<&TestServer>) -> Registry {
    let registry = Registry::new(http(), &server.url()).unwrap();
    match auth {
        Some(auth) => registry.token_auth(TokenAuth::anonymous().trust_realm(&auth.url()).unwrap()),
        None => registry,
    }
}

/// A server answering with `replies` in order.
pub async fn scripted(replies: impl IntoIterator<Item = Reply>) -> TestServer {
    let script = Script::new(replies);
    TestServer::start(move |_| script.next()).await
}

pub fn json(status: u16, body: &[u8]) -> Reply {
    Reply::new(status)
        .header("content-type", "application/json; charset=utf-8")
        .body(body)
}

/// A registry error document reply, with the version header Distribution sends.
pub fn error(status: u16, body: &[u8]) -> Reply {
    json(status, body).header("docker-distribution-api-version", "registry/2.0")
}

/// A `401` with a Bearer challenge.
pub fn challenge(realm: &str, scope: Option<&str>) -> Reply {
    let mut value = format!(r#"Bearer realm="{realm}",service="{SERVICE}""#);
    if let Some(scope) = scope {
        value.push_str(&format!(r#",scope="{scope}""#));
    }
    error(401, UNAUTHORIZED).header("www-authenticate", value)
}

/// `Link: <url>; rel="next"`.
pub fn next(reply: Reply, url: &str) -> Reply {
    reply.header("link", format!("<{url}>; rel=\"next\""))
}

pub fn empty(status: u16) -> Reply {
    Reply::new(status).header("docker-distribution-api-version", "registry/2.0")
}

/// `Debug` and `Display` of a value contain none of `secrets`.
pub fn assert_hides<T: std::fmt::Debug + std::fmt::Display>(value: &T, secrets: &[&str]) {
    let shown = format!("{value} {value:?}");
    for secret in secrets {
        assert!(!shown.contains(secret), "{secret} leaked in {shown}");
    }
}

// ---------------------------------------------------------------------------------------------
// The fake registry and token service.

/// One repository of the fake.
#[derive(Default)]
pub struct Repo {
    pub tags: BTreeMap<String, String>,
    pub manifests: HashMap<String, (String, Vec<u8>)>,
    pub blobs: HashMap<String, Vec<u8>>,
}

pub struct Upload {
    pub repo: String,
    pub data: Vec<u8>,
    pub generation: u64,
}

/// What a test can perturb.
#[derive(Default)]
pub struct Perturb {
    /// The n-th `PATCH` (1-based) gets no answer; `true` stores its bytes first.
    pub hang_up_patch: Option<(usize, bool)>,
    /// Blob `GET` answers flip one byte (the length stays).
    pub corrupt_blobs: bool,
    /// The token service grants only `pull`, whatever was asked.
    pub pull_only: bool,
}

pub struct State {
    pub realm: String,
    pub repos: BTreeMap<String, Repo>,
    pub uploads: HashMap<u64, Upload>,
    pub next_upload: u64,
    /// Issued tokens → the scopes they grant.
    pub tokens: HashMap<String, Vec<String>>,
    /// Token requests: (service, scopes, authorization header).
    pub token_requests: Vec<(Option<String>, Vec<String>, Option<String>)>,
    pub require_basic: bool,
    pub perturb: Perturb,
    pub patches: usize,
}

pub struct Fake {
    pub registry: TestServer,
    pub auth: TestServer,
    pub state: Arc<Mutex<State>>,
}

impl Fake {
    pub async fn start() -> Self {
        let state = Arc::new(Mutex::new(State {
            realm: String::new(),
            repos: BTreeMap::new(),
            uploads: HashMap::new(),
            next_upload: 1,
            tokens: HashMap::new(),
            token_requests: Vec::new(),
            require_basic: false,
            perturb: Perturb::default(),
            patches: 0,
        }));
        let for_auth = Arc::clone(&state);
        let auth = TestServer::start(move |request| token_service(&for_auth, request)).await;
        state.lock().unwrap().realm = format!("{}/token", auth.url());
        let for_registry = Arc::clone(&state);
        let registry = TestServer::start(move |request| serve(&for_registry, request)).await;
        Self {
            registry,
            auth,
            state,
        }
    }

    pub fn client(&self) -> Registry {
        registry(&self.registry, Some(&self.auth))
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        f(&mut self.state.lock().unwrap())
    }

    /// Seeds `repo` with the fixture image under `tag`, as `media_type` (`OCI` or `DOCKER_V2`).
    pub fn seed(&self, repo: &str, tag: &str, media_type: &str) {
        let bytes = if media_type == OCI {
            MANIFEST_OCI
        } else {
            MANIFEST_DOCKER
        };
        let digest = digest(bytes).to_string();
        self.with(|state| {
            let repo = state.repos.entry(repo.to_owned()).or_default();
            repo.blobs.insert(digest_text(CONFIG), CONFIG.to_vec());
            repo.blobs.insert(digest_text(LAYER), LAYER.to_vec());
            repo.manifests
                .insert(digest.clone(), (media_type.to_owned(), bytes.to_vec()));
            repo.tags.insert(tag.to_owned(), digest);
        });
    }

    pub fn add_repo(&self, repo: &str) {
        self.with(|state| {
            state.repos.entry(repo.to_owned()).or_default();
        });
    }
}

fn digest_text(bytes: &[u8]) -> String {
    digest(bytes).to_string()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 3 <= bytes.len()
            && let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn encode_query(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn token_service(state: &Mutex<State>, request: &Recorded) -> Reply {
    let mut state = state.lock().unwrap();
    let service = request.query_all("service").into_iter().next();
    let scopes = request.query_all("scope");
    let authorization = request.header("authorization").map(str::to_owned);
    state
        .token_requests
        .push((service.clone(), scopes.clone(), authorization.clone()));
    if request.method != "GET" || request.path() != "/token" {
        return Reply::new(404);
    }
    if service.as_deref() != Some(SERVICE) {
        return json(400, br#"{"details":"unknown service"}"#);
    }
    if state.require_basic && authorization.as_deref() != Some(BASIC) {
        return json(401, br#"{"details":"incorrect username or password"}"#);
    }
    let granted: Vec<String> = if state.perturb.pull_only {
        scopes
            .iter()
            .map(|scope| scope.replace(":pull,push", ":pull"))
            .collect()
    } else {
        scopes
    };
    let token = format!("synthetic-fake-token-{}", state.tokens.len() + 1);
    state.tokens.insert(token.clone(), granted);
    json(
        200,
        format!(r#"{{"token":"{token}","expires_in":300,"issued_at":"2026-11-06T00:00:00Z"}}"#)
            .as_bytes(),
    )
}

/// The scope a request needs.
fn needed_scope(method: &str, path: &str) -> Option<String> {
    if path == "/v2/" {
        return None;
    }
    if path == "/v2/_catalog" {
        return Some("registry:catalog:*".to_owned());
    }
    let rest = path.strip_prefix("/v2/")?;
    let name = ["/tags/", "/manifests/", "/blobs/"]
        .iter()
        .find_map(|marker| rest.find(marker).map(|at| &rest[..at]))?;
    let push = !matches!(method, "GET" | "HEAD") || rest.contains("/blobs/uploads/");
    Some(if push {
        format!("repository:{name}:pull,push")
    } else {
        format!("repository:{name}:pull")
    })
}

fn granted(state: &State, request: &Recorded, needed: &Option<String>) -> bool {
    let Some(token) = request
        .header("authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return false;
    };
    let Some(scopes) = state.tokens.get(token) else {
        return false;
    };
    match needed {
        None => true,
        Some(needed) => scopes.iter().any(|scope| {
            scope == needed || (needed.ends_with(":pull") && *scope == format!("{needed},push"))
        }),
    }
}

fn upload_location(name: &str, id: u64, generation: u64) -> String {
    format!("/v2/{name}/blobs/uploads/p{id}-g{generation}?_state=s{id}.{generation}%2Bopaque%3D%3D")
}

fn upload_reply(status: u16, name: &str, id: u64, upload: &Upload) -> Reply {
    let end = upload.data.len().saturating_sub(1);
    empty(status)
        .header("location", upload_location(name, id, upload.generation))
        .header("range", format!("0-{end}"))
        .header("docker-upload-uuid", format!("uuid-{id:04}-not-the-path"))
}

fn serve(state: &Mutex<State>, request: &Recorded) -> Reply {
    let mut state = state.lock().unwrap();
    let path = percent_decode(request.path());
    let method = request.method.as_str();
    let needed = needed_scope(method, &path);
    if !granted(&state, request, &needed) {
        return challenge(&state.realm, needed.as_deref());
    }
    if path == "/v2/" {
        return empty(200)
            .header("content-type", "application/json")
            .body("{}");
    }
    if path == "/v2/_catalog" {
        let names: Vec<String> = state.repos.keys().cloned().collect();
        return listing(
            request,
            "/v2/_catalog",
            names,
            |page| serde_json::json!({ "repositories": page }),
        );
    }
    let rest = path.strip_prefix("/v2/").unwrap_or_default().to_owned();
    if let Some(name) = rest.strip_suffix("/tags/list") {
        let Some(repo) = state.repos.get(name) else {
            return error(404, NAME_UNKNOWN);
        };
        let tags: Vec<String> = repo.tags.keys().cloned().collect();
        let name = name.to_owned();
        return listing(
            request,
            &format!("/v2/{name}/tags/list"),
            tags,
            |page| serde_json::json!({ "name": name, "tags": page }),
        );
    }
    if let Some(at) = rest.find("/blobs/uploads/") {
        let name = rest[..at].to_owned();
        let tail = rest[at + "/blobs/uploads/".len()..].to_owned();
        return upload(&mut state, request, &name, &tail);
    }
    if let Some(at) = rest.find("/manifests/") {
        let name = rest[..at].to_owned();
        let reference = rest[at + "/manifests/".len()..].to_owned();
        return manifest(&mut state, request, &name, &reference);
    }
    if let Some(at) = rest.find("/blobs/") {
        let name = &rest[..at];
        let digest = &rest[at + "/blobs/".len()..];
        let corrupt = state.perturb.corrupt_blobs;
        let Some(bytes) = state.repos.get(name).and_then(|r| r.blobs.get(digest)) else {
            return if method == "HEAD" {
                empty(404)
            } else {
                error(404, BLOB_UNKNOWN)
            };
        };
        let reply = empty(200)
            .header("content-type", "application/octet-stream")
            .header("docker-content-digest", digest);
        if method == "HEAD" {
            return reply.with_body(Body::HeadOnly(bytes.len() as u64));
        }
        let mut bytes = bytes.clone();
        if corrupt && let Some(first) = bytes.first_mut() {
            *first ^= 0x20;
        }
        return reply.body(bytes);
    }
    Reply::new(404)
}

fn listing(
    request: &Recorded,
    collection: &str,
    entries: Vec<String>,
    body: impl Fn(Vec<String>) -> serde_json::Value,
) -> Reply {
    let n: Option<usize> = match request.query_all("n").first() {
        Some(n) => match n.parse() {
            Ok(n) if n > 0 => Some(n),
            _ => return error(400, PAGINATION_INVALID),
        },
        None => None,
    };
    let last = request.query_all("last").into_iter().next();
    let after: Vec<String> = entries
        .into_iter()
        .filter(|entry| last.as_ref().is_none_or(|last| entry > last))
        .collect();
    let take = n.unwrap_or(after.len());
    let more = after.len() > take;
    let page: Vec<String> = after.into_iter().take(take).collect();
    let mut reply = json(200, body(page.clone()).to_string().as_bytes())
        .header("docker-distribution-api-version", "registry/2.0");
    if more && let (Some(n), Some(last)) = (n, page.last()) {
        reply = next(
            reply,
            &format!("{collection}?last={}&n={n}", encode_query(last)),
        );
    }
    reply
}

fn manifest(state: &mut State, request: &Recorded, name: &str, reference: &str) -> Reply {
    match request.method.as_str() {
        "GET" | "HEAD" => {
            let Some(repo) = state.repos.get(name) else {
                return error(404, NAME_UNKNOWN);
            };
            let digest = if reference.contains(':') {
                Some(reference.to_owned())
            } else {
                repo.tags.get(reference).cloned()
            };
            let Some((media_type, bytes)) = digest.as_ref().and_then(|d| repo.manifests.get(d))
            else {
                return error(404, MANIFEST_UNKNOWN);
            };
            let accepted = request
                .header_all("accept")
                .iter()
                .flat_map(|value| value.split(','))
                .any(|item| item.trim() == media_type);
            if !accepted {
                return error(404, MANIFEST_UNKNOWN);
            }
            empty(200)
                .header("content-type", media_type.clone())
                .header("docker-content-digest", digest.unwrap())
                .body(bytes.clone())
        }
        "PUT" => {
            let media_type = request
                .header("content-type")
                .unwrap_or_default()
                .to_owned();
            if media_type != OCI && media_type != DOCKER_V2 {
                return error(
                    400,
                    br#"{"errors":[{"code":"MANIFEST_INVALID","message":"manifest invalid"}]}"#,
                );
            }
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&request.body) else {
                return error(
                    400,
                    br#"{"errors":[{"code":"MANIFEST_INVALID","message":"manifest invalid"}]}"#,
                );
            };
            let repo = state.repos.entry(name.to_owned()).or_default();
            let mut referenced = vec![value["config"]["digest"].as_str().unwrap_or("").to_owned()];
            if let Some(layers) = value["layers"].as_array() {
                referenced.extend(
                    layers
                        .iter()
                        .map(|l| l["digest"].as_str().unwrap_or("").to_owned()),
                );
            }
            let missing: Vec<serde_json::Value> = referenced
                .iter()
                .filter(|digest| !repo.blobs.contains_key(*digest))
                .map(|digest| {
                    serde_json::json!({"code": "BLOB_UNKNOWN", "message": "blob unknown to registry", "detail": {"digest": digest}})
                })
                .collect();
            if !missing.is_empty() {
                let body = serde_json::json!({ "errors": missing }).to_string();
                return error(400, body.as_bytes());
            }
            let digest = digest_text(&request.body);
            if reference.contains(':') && reference != digest {
                return error(400, DIGEST_INVALID);
            }
            repo.manifests
                .insert(digest.clone(), (media_type, request.body.clone()));
            if !reference.contains(':') {
                repo.tags.insert(reference.to_owned(), digest.clone());
            }
            empty(201)
                .header("location", format!("/v2/{name}/manifests/{digest}"))
                .header("docker-content-digest", digest)
        }
        _ => Reply::new(405),
    }
}

fn upload(state: &mut State, request: &Recorded, name: &str, tail: &str) -> Reply {
    if request.method == "POST" && tail.is_empty() {
        let id = state.next_upload;
        state.next_upload += 1;
        state.repos.entry(name.to_owned()).or_default();
        let upload = Upload {
            repo: name.to_owned(),
            data: Vec::new(),
            generation: 0,
        };
        let reply = upload_reply(202, name, id, &upload);
        state.uploads.insert(id, upload);
        return reply;
    }
    // `p<id>-g<generation>`, and the opaque state of that same generation, or the session is
    // unknown: a stale or rebuilt URL fails like an unknown upload.
    let parsed = tail
        .strip_prefix('p')
        .and_then(|t| t.split_once("-g"))
        .and_then(|(id, generation)| {
            Some((id.parse::<u64>().ok()?, generation.parse::<u64>().ok()?))
        });
    let Some((id, generation)) = parsed else {
        return error(404, UPLOAD_UNKNOWN);
    };
    let expected = upload_location(name, id, generation);
    // A status query may come from an earlier `Location` (the answer to the chunk that moved the
    // session was lost); chunks and the completion must use the latest one.
    let current = state.uploads.get(&id).is_some_and(|u| {
        u.repo == name
            && (u.generation == generation
                || (request.method == "GET" && generation < u.generation))
    });
    let state_ok =
        request.target == expected || request.target.starts_with(&format!("{expected}&"));
    if !current || !state_ok {
        return error(404, UPLOAD_UNKNOWN);
    }
    match request.method.as_str() {
        "PATCH" => {
            state.patches += 1;
            let hang_up = state
                .perturb
                .hang_up_patch
                .filter(|(n, _)| *n == state.patches);
            let upload = state.uploads.get_mut(&id).unwrap();
            let start = upload.data.len();
            let expected_range = format!("{start}-{}", start + request.body.len() - 1);
            if request.header("content-range") != Some(expected_range.as_str()) {
                let reply = upload_reply(416, name, id, upload);
                return reply
                    .header("content-type", "application/json")
                    .body(RANGE_INVALID);
            }
            if let Some((_, store)) = hang_up {
                if store {
                    upload.data.extend_from_slice(&request.body);
                    upload.generation += 1;
                }
                return Reply::hang_up();
            }
            upload.data.extend_from_slice(&request.body);
            upload.generation += 1;
            upload_reply(202, name, id, upload)
        }
        "GET" => upload_reply(204, name, id, &state.uploads[&id]),
        "PUT" => {
            let digest = request
                .query_all("digest")
                .into_iter()
                .next()
                .unwrap_or_default();
            let mut upload = state.uploads.remove(&id).unwrap();
            upload.data.extend_from_slice(&request.body);
            if digest_text(&upload.data) != digest {
                state.uploads.insert(id, upload);
                return error(400, DIGEST_INVALID);
            }
            let repo = state.repos.entry(name.to_owned()).or_default();
            repo.blobs.insert(digest.clone(), upload.data);
            empty(201)
                .header("location", format!("/v2/{name}/blobs/{digest}"))
                .header("docker-content-digest", digest)
        }
        _ => Reply::new(405),
    }
}
