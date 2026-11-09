//! Secrets: BasicCredentials username + password (src/auth.rs:158; Basic to the token service,
//! the #[sensitive] TokenService credentials, src/api.rs:112); the pull token (`token`) and push
//! token (`access_token`) the token service returns (src/model.rs:27), sent as the #[sensitive]
//! Bearer authorization (src/api.rs:36,55); a `refresh_token` in the same answer, documented as
//! ignored; the upload `Location` `_state` (an upload capability, withheld by BlobUpload and
//! ResponseMeta Debug, src/upload.rs:45, src/meta.rs:20) and the `rel="next"` link (NextLink
//! withholds its URL, src/pages.rs:40). Each has its own canary.
//! Scenarios: tag listing and manifest GET (reads) and manifest PUT (write) across 2xx, 4xx
//! error document, 503, malformed, wrong Content-Type, truncated and refused; the PUT has no
//! response body, so malformed/wrong-type bodies are drained and it succeeds there. Case 0 also
//! drives ping, a verified blob read, page 2 and an upload session (POST, PATCH, status, PUT).
//! A second phase breaks the token service (500, malformed, wrong type, truncated, an unsendable
//! token, refused) so a response-only token reaches the decoder. Echoes go only into withheld
//! places: response headers, unrecognized bodies and undecodable bodies.
//! Wire exemptions: the refresh token (ignored by design, never sent) and the broken-token
//! canary (never accepted). Not covered: native reqwest values, `Prepared` (the declarations are
//! private; no public `prepare()`), and documented clear values: `BlobUpload::location`,
//! `NextLink::url`, `ResponseMeta::headers`, error-document messages/details, challenges.

use concord::bytes::Bytes;
use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_distribution_registry::{
    AuthProblem, BasicCredentials, Descriptor, Digest, ErrorBody, ErrorCode, ListOptions,
    ManifestMediaType, ManifestUpload, ProtocolProblem, Reference, Registry, RegistryError,
    RepositoryName, TokenAuth,
};
use std::time::Duration;

const MANIFEST: &[u8] = include_bytes!("../fixtures/manifest-oci.json");
const UNAUTHORIZED: &[u8] = include_bytes!("../fixtures/token-spec-unauthorized.json");
const NAME_UNKNOWN: &[u8] = include_bytes!("../fixtures/error-name-unknown.json");
const MANIFEST_UNKNOWN: &[u8] = include_bytes!("../fixtures/error-manifest-unknown.json");
const BLOBS_UNKNOWN: &[u8] = include_bytes!("../fixtures/error-manifest-blobs-unknown.json");
const OCI: &str = "application/vnd.oci.image.manifest.v1+json";
const BLOB: &[u8] = b"synthetic canary layer";
const UPLOAD: &str = "/v2/acme/app/blobs/uploads/u1";

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    concord_example_testserver::refused_url()
}

fn challenge(realm: &str, echo: &str) -> Reply {
    let value = format!(r#"Bearer realm="{realm}/token",service="registry.canary.test""#);
    Reply::json(401, UNAUTHORIZED)
        .header("www-authenticate", value)
        .header("x-private", echo)
}

fn reply(
    case: usize,
    ok: u16,
    success: Reply,
    error: (u16, &[u8]),
    media: &str,
    echo: &str,
) -> Reply {
    let response = match case {
        0 | 6 => success,
        1 => {
            Reply::json(error.0, error.1).header("docker-distribution-api-version", "registry/2.0")
        }
        2 => Reply::text(503, "text/html", format!("<html>{echo}</html>")),
        3 => Reply::text(ok, media, format!("<{{invalid {echo}")),
        4 => Reply::text(ok, "text/html", echo),
        5 => Reply::text(ok, media, "").with_body(Body::FailAfter(vec![
            format!("{{\"incomplete\":\"{echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn registry_reply(
    case: usize,
    r: &Recorded,
    realm: &str,
    state: &str,
    link: &str,
    echo: &str,
) -> Reply {
    if !r
        .header("authorization")
        .is_some_and(|v| v.starts_with("Bearer "))
    {
        return challenge(realm, echo);
    }
    let path = r.path();
    let location = format!("{UPLOAD}?_state={state}");
    if path.ends_with("/tags/list") {
        if r.query().is_some_and(|q| q.contains(link)) {
            return Reply::json(200, r#"{"name":"acme/app","tags":["v2"]}"#);
        }
        let next = format!(r#"</v2/acme/app/tags/list?n=1&last={link}>; rel="next""#);
        let page = Reply::json(200, r#"{"name":"acme/app","tags":["v1"]}"#).header("link", next);
        reply(
            case,
            200,
            page,
            (404, NAME_UNKNOWN),
            "application/json",
            echo,
        )
    } else if path.contains("/manifests/") && r.method == "PUT" {
        let created = Reply::new(201).header("location", "/v2/acme/app/manifests/v2");
        reply(case, 201, created, (400, BLOBS_UNKNOWN), OCI, echo)
    } else if path.contains("/manifests/") {
        reply(
            case,
            200,
            Reply::text(200, OCI, MANIFEST),
            (404, MANIFEST_UNKNOWN),
            OCI,
            echo,
        )
    } else if path.starts_with(UPLOAD) || path.ends_with("/blobs/uploads/") {
        match r.method.as_str() {
            "POST" => Reply::new(202)
                .header("location", location)
                .header("docker-upload-uuid", "u1"),
            "PATCH" => Reply::new(202)
                .header("location", location)
                .header("range", "0-3"),
            "GET" => Reply::new(204).header("range", "0-3"),
            _ => Reply::new(201).header("location", "/v2/acme/app/blobs/sha256:x"),
        }
        .header("x-private", echo)
    } else if path.contains("/blobs/") {
        Reply::new(200).body(BLOB).header("x-private", echo)
    } else {
        Reply::new(200).header("docker-distribution-api-version", "registry/2.0")
    }
}

fn concord_error(c: &mut Canaries, e: &concord::Error) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
}

fn error(c: &mut Canaries, e: &RegistryError) {
    c.assert_debug_absent("RegistryError", e);
    c.assert_error_absent("RegistryError/source chain", e);
    c.assert_debug_absent("error metadata", &e.meta());
    c.assert_debug_absent("error status", &e.status());
    c.assert_debug_absent("auth problem", &e.auth_problem());
    c.assert_debug_absent("protocol problem", &e.protocol_problem());
    c.assert_debug_absent("request kind", &e.request_kind());
    match e {
        RegistryError::Request { error, meta } => {
            concord_error(c, error);
            c.assert_debug_absent("request metadata", meta);
        }
        RegistryError::Api(api) => {
            c.assert_debug_absent("ApiError", api);
            c.assert_display_absent("ApiError", api);
            c.assert_debug_absent("ApiError failure", &api.failure);
            c.assert_debug_absent("ApiError document state", api.failure.state());
            c.assert_debug_absent("ApiError entries", api.errors());
            c.assert_debug_absent("ApiError metadata", &api.meta);
            if let ErrorBody::Broken(e) = api.failure.state() {
                concord_error(c, e);
            }
        }
        RegistryError::Auth(auth) => {
            c.assert_debug_absent("AuthError", auth);
            c.assert_display_absent("AuthError", auth);
            c.assert_debug_absent("AuthError metadata", &auth.meta);
            if let Some(e) = auth.source_error() {
                concord_error(c, e);
            }
        }
        RegistryError::Protocol(p) => {
            c.assert_debug_absent("ProtocolError", p);
            c.assert_display_absent("ProtocolError", p);
        }
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("distribution-registry");
    let username = c.issue();
    let password = c.issue();
    let pull_token = c.issue();
    let push_token = c.issue();
    let refresh = c.issue();
    let state = c.issue();
    let link = c.issue();
    let credentials = BasicCredentials::new(&username, &password).unwrap();
    c.assert_debug_absent("BasicCredentials", &credentials);
    let name = RepositoryName::new("acme/app").unwrap();
    let upload = ManifestUpload::new(ManifestMediaType::OciV1, MANIFEST).unwrap();
    for case in 0..7 {
        let (pull, push, rt) = (pull_token.clone(), push_token.clone(), refresh.clone());
        let tokens = TestServer::start(move |r: &Recorded| {
            let body = if r.query_all("scope").iter().any(|s| s.contains("push")) {
                format!(r#"{{"access_token":"{push}","expires_in":3600,"refresh_token":"{rt}"}}"#)
            } else {
                format!(r#"{{"token":"{pull}","expires_in":3600,"refresh_token":"{rt}"}}"#)
            };
            Reply::json(200, body).header("x-private", &rt)
        })
        .await;
        let (realm, st, ln, echo) = (
            tokens.url(),
            state.clone(),
            link.clone(),
            pull_token.clone(),
        );
        let server =
            TestServer::start(move |r: &Recorded| registry_reply(case, r, &realm, &st, &ln, &echo))
                .await;
        let auth = TokenAuth::basic(credentials.clone())
            .trust_realm(&tokens.url())
            .unwrap()
            .client_id("canary-audit-label");
        c.assert_debug_absent("TokenAuth", &auth);
        let base = if case == 6 { refused() } else { server.url() };
        let registry = Registry::new(http(), &base).unwrap().token_auth(auth);
        let repo = registry.repository(&name).unwrap();
        c.assert_debug_absent("Registry", &registry);
        c.assert_debug_absent("Repository", &repo);
        c.assert_debug_absent("ListOptions", &ListOptions::default());

        let mut walk = repo.tags(ListOptions::default());
        c.assert_debug_absent("tag traversal", &walk);
        let page = walk.next_page().await;
        c.assert_debug_absent("tag page Result", &page);
        assert_eq!(page.is_ok(), case == 0);
        match page {
            Ok(page) => {
                let page = page.unwrap();
                c.assert_debug_absent("tag page metadata", &page.meta);
                c.assert_debug_absent("NextLink", &page.next);
                assert!(page.next.is_some());
                let second = walk.next_page().await;
                c.assert_debug_absent("tag page 2 Result", &second);
                assert!(second.unwrap().unwrap().next.is_none());
            }
            Err(e) => {
                c.assert_debug_absent("PagesError", &e);
                c.assert_error_absent("PagesError", &e);
                error(&mut c, e.provider_error().unwrap());
            }
        }
        c.assert_debug_absent("tag checkpoint", &walk.checkpoint());
        c.assert_debug_absent("tag progress", &walk.progress());
        c.assert_debug_absent("tag report", &walk.finish());

        let read = repo
            .manifest(&Reference::tag("v1").unwrap(), &[ManifestMediaType::OciV1])
            .await;
        c.assert_debug_absent("manifest Result", &read);
        assert_eq!(read.is_ok(), case == 0);
        match read {
            Ok(m) => c.assert_debug_absent("manifest metadata", &m.meta),
            Err(e) => {
                error(&mut c, &e);
                if case == 1 {
                    // Safe diagnosis survives: operation, status and the error code.
                    let shown = e.to_string();
                    assert_eq!(e.status().unwrap().as_u16(), 404);
                    assert!(e.has_code(&ErrorCode::ManifestUnknown));
                    assert!(shown.contains("Repository.manifest") && shown.contains("404"));
                    assert!(shown.contains("MANIFEST_UNKNOWN"), "{shown}");
                }
                if case == 4 {
                    let problem = e.protocol_problem();
                    assert_eq!(problem, Some(&ProtocolProblem::UnexpectedMediaType));
                }
            }
        }

        let write = repo
            .put_manifest(&Reference::tag("v2").unwrap(), &upload)
            .await;
        c.assert_debug_absent("manifest PUT Result", &write);
        assert_eq!(write.is_ok(), matches!(case, 0 | 3 | 4));
        if let Err(e) = &write {
            error(&mut c, e);
        }

        if case == 0 {
            let version = registry.ping().await.unwrap();
            c.assert_debug_absent("ApiVersion", &version);
            assert_eq!(version.version(), Some("registry/2.0"));
            let descriptor = Descriptor::of("application/octet-stream", BLOB);
            let reader = repo.blob_for(&descriptor).await.unwrap();
            c.assert_debug_absent("BlobReader", &reader);
            assert_eq!(reader.read_to_end(descriptor.size).await.unwrap(), BLOB);
            let mut session = repo.start_upload().await.unwrap();
            c.assert_debug_absent("BlobUpload", &session);
            let progress = session.patch(Bytes::from_static(&BLOB[..4])).await.unwrap();
            c.assert_debug_absent("UploadProgress (PATCH)", &progress);
            let progress = session.status().await.unwrap();
            c.assert_debug_absent("UploadProgress (status)", &progress);
            let rest = Bytes::from_static(&BLOB[4..]);
            let pushed = session.complete(&Digest::sha256_of(BLOB), Some(rest)).await;
            c.assert_debug_absent("BlobPushed Result", &pushed);
            assert_eq!(pushed.unwrap().size, BLOB.len() as u64);
            c.assert_debug_absent("finished BlobUpload", &session);

            let mut requests = server.requests();
            requests.extend(tokens.requests());
            c.assert_on_wire_except(&requests, &[&refresh]);
            // Tokens stay with their access: pull reads carry the pull token, writes the push one.
            for r in server
                .requests()
                .iter()
                .filter(|r| r.header("authorization").is_some())
            {
                let push = r.method != "GET" || r.path().starts_with(UPLOAD);
                let token = if push { &push_token } else { &pull_token };
                assert_eq!(
                    r.header("authorization"),
                    Some(format!("Bearer {token}").as_str())
                );
            }
        }
        if case == 6 {
            assert!(server.requests().is_empty() && tokens.requests().is_empty());
        }
        server.shutdown().await;
        tokens.shutdown().await;
    }

    // A response-only token that never becomes usable must not print either.
    let bad_token = c.issue();
    for mode in 0..6 {
        let (tok, rt) = (bad_token.clone(), refresh.clone());
        let tokens = TestServer::start(move |_: &Recorded| match mode {
            0 => Reply::text(500, "text/html", format!("<p>{tok}</p>")),
            1 => Reply::json(200, format!(r#"{{"token":"{tok}"#)),
            2 => Reply::text(200, "text/html", format!(r#"{{"token":"{tok}"}}"#)),
            3 => Reply::json(200, "")
                .with_body(Body::FailAfter(vec![format!(r#"{{"token":"{tok}"#).into()])),
            _ => Reply::json(
                200,
                format!(r#"{{"token":"{tok}\u0007","refresh_token":"{rt}"}}"#),
            ),
        })
        .await;
        let realm = if mode == 5 { refused() } else { tokens.url() };
        let (challenge_realm, echo) = (realm.clone(), bad_token.clone());
        let server =
            TestServer::start(move |_: &Recorded| challenge(&challenge_realm, &echo)).await;
        let auth = TokenAuth::basic(credentials.clone())
            .trust_realm(&realm)
            .unwrap();
        let registry = Registry::new(http(), &server.url())
            .unwrap()
            .token_auth(auth);
        let e = registry
            .repository(&name)
            .unwrap()
            .manifest(&Reference::tag("v1").unwrap(), &[ManifestMediaType::OciV1])
            .await
            .unwrap_err();
        error(&mut c, &e);
        let expected = match mode {
            0 => AuthProblem::TokenRefused,
            5 => AuthProblem::TokenTransport,
            _ => AuthProblem::TokenResponse,
        };
        assert_eq!(e.auth_problem(), Some(expected), "mode {mode}");
        if mode == 0 {
            // Safe diagnosis: the token service's status stays visible.
            assert!(e.to_string().contains("HTTP 500"), "{e}");
        }
        server.shutdown().await;
        tokens.shutdown().await;
    }

    let e = BasicCredentials::new(&format!("{username}:"), &password).unwrap_err();
    c.assert_debug_absent("credential input error", &e);
    c.assert_error_absent("credential input error", &e);
    assert!(c.checked() >= 300, "{}", c.checked());
    println!(
        "distribution-registry: {} renderings; positive control passed (refresh token exempt)",
        c.checked()
    );
}
