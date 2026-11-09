//! An independent application consumer; fake answers come from the official protocol docs.
//! See FIRST_USE.md and fixtures/first-use-PROVENANCE.md for predictions and provenance.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_pagination::{Outcome, PagesError};
use concord_sdk_distribution_registry::concord::{self, bytes::Bytes};
use concord_sdk_distribution_registry::{
    AuthProblem, Descriptor, Digest, ErrorCode, ImageManifest, InputError, IntegrityProblem,
    ListOptions, ManifestMediaType, PageSize, ProtocolProblem, Reference, Registry, RegistryError,
    Repository, RepositoryName, TokenAuth,
};

const NAME: &str = "consumer/nested/app";
const DOCKER: &str = "application/vnd.docker.distribution.manifest.v2+json";
const CONFIG: &[u8] = include_bytes!("../fixtures/first-use-config.json");
const LAYER: &[u8] = include_bytes!("../fixtures/first-use-layer.bin");
const MANIFEST: &[u8] = include_bytes!("../fixtures/first-use-manifest.json");
const MISSING: &[u8] = include_bytes!("../fixtures/first-use-missing-manifest.json");

fn registry(server: &TestServer) -> Registry {
    let http = concord::reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(2))
        .redirect(concord::reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .unwrap();
    Registry::new(http, &server.url()).unwrap()
}

fn repository(registry: &Registry) -> Repository {
    registry
        .repository(&RepositoryName::new(NAME).unwrap())
        .unwrap()
}

fn error_reply(status: u16, code: &str, detail: serde_json::Value) -> Reply {
    Reply::json(
        status,
        serde_json::to_vec(&serde_json::json!({
            "errors": [{"code": code, "message": "synthetic provider failure", "detail": detail}]
        }))
        .unwrap(),
    )
}

fn manifest_reply() -> Reply {
    Reply::text(200, DOCKER, MANIFEST)
        .header(
            "docker-content-digest",
            Digest::sha256_of(MANIFEST).to_string(),
        )
        .header("docker-distribution-api-version", "registry/2.0")
}

fn assert_empty(request: &Recorded) {
    assert!(request.body.is_empty());
    assert_eq!(request.body_len, 0);
}

fn assert_accept(request: &Recorded) {
    assert_eq!(
        request
            .header("accept")
            .unwrap()
            .split(',')
            .map(str::trim)
            .collect::<Vec<_>>(),
        [DOCKER, "application/vnd.oci.image.manifest.v1+json"]
    );
}

#[tokio::test]
async fn discover_catalog_and_tags_pull_manifest_and_verify_all_blobs() {
    let server = TestServer::start(|request| {
        assert_empty(request);
        assert_eq!(request.header("authorization"), None);
        match (request.method.as_str(), request.path()) {
            ("GET", "/v2/") => {
                Reply::new(200).header("docker-distribution-api-version", "registry/2.0")
            }
            ("GET", "/v2/_catalog") => {
                if request.query_all("last").is_empty() {
                    assert_eq!(request.target, "/v2/_catalog?n=2");
                    Reply::json(200, format!(r#"{{"repositories":["{NAME}"]}}"#)).header(
                        "link",
                        "</v2/_catalog?last=consumer%2Fnested%2Fapp&n=1>; rel=\"next\"".to_string(),
                    )
                } else {
                    assert_eq!(
                        request.target,
                        "/v2/_catalog?last=consumer%2Fnested%2Fapp&n=1"
                    );
                    Reply::json(200, br#"{"repositories":["zeta/service"]}"#)
                }
            }
            ("GET", "/v2/consumer/nested/app/tags/list") => {
                if request.query_all("last").is_empty() {
                    assert_eq!(request.target, "/v2/consumer/nested/app/tags/list?n=1");
                    Reply::json(200, format!(r#"{{"name":"{NAME}","tags":["stable"]}}"#)).header(
                        "link",
                        "</v2/consumer/nested/app/tags/list?last=stable&n=3>; rel=\"next\"",
                    )
                } else {
                    assert_eq!(
                        request.target,
                        "/v2/consumer/nested/app/tags/list?last=stable&n=3"
                    );
                    Reply::json(200, format!(r#"{{"name":"{NAME}","tags":["v2","v3"]}}"#))
                }
            }
            ("GET", path) if path.starts_with("/v2/consumer/nested/app/manifests/") => {
                assert_accept(request);
                assert!(
                    path == "/v2/consumer/nested/app/manifests/stable"
                        || path == format!("/v2/{NAME}/manifests/{}", Digest::sha256_of(MANIFEST))
                );
                manifest_reply()
            }
            (method @ ("GET" | "HEAD"), path) => {
                for bytes in [CONFIG, LAYER] {
                    let digest = Digest::sha256_of(bytes);
                    if path == format!("/v2/{NAME}/blobs/{digest}") {
                        let reply =
                            Reply::new(200).header("docker-content-digest", digest.to_string());
                        return if method == "HEAD" {
                            reply.with_body(Body::HeadOnly(bytes.len() as u64))
                        } else {
                            reply
                                .header("content-type", "application/octet-stream")
                                .body(bytes)
                        };
                    }
                }
                assert_eq!(method, "HEAD");
                assert_eq!(
                    path,
                    format!("/v2/{NAME}/blobs/{}", Digest::sha256_of(b"absent"))
                );
                Reply::new(404).with_body(Body::HeadOnly(0))
            }
            _ => panic!("unexpected request: {request:?}"),
        }
    })
    .await;
    let registry = registry(&server);
    assert_eq!(
        registry.ping().await.unwrap().version(),
        Some("registry/2.0")
    );
    let mut catalog = registry.catalog(ListOptions::default().page_size(PageSize::new(2).unwrap()));
    assert_eq!(
        server.requests().len(),
        1,
        "constructing pages sends nothing"
    );
    let first = catalog.next_page().await.unwrap().unwrap();
    assert_eq!(first.items, [NAME]);
    assert!(first.next.is_some());
    let last = catalog.next_page().await.unwrap().unwrap();
    assert_eq!(last.items, ["zeta/service"]);
    assert!(last.next.is_none());
    assert!(catalog.next_page().await.unwrap().is_none());
    let repo = repository(&registry);
    let tags = repo
        .tags(ListOptions::default().page_size(PageSize::new(1).unwrap()))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(tags, ["stable", "v2", "v3"]);
    let accept = [ManifestMediaType::DockerV2, ManifestMediaType::OciV1];
    let manifest = repo
        .manifest(&Reference::tag(&tags[0]).unwrap(), &accept)
        .await
        .unwrap();
    assert_eq!(manifest.bytes.as_ref(), MANIFEST);
    assert_eq!(manifest.digest, Digest::sha256_of(MANIFEST));
    assert_eq!(manifest.media_type, ManifestMediaType::DockerV2);
    assert_eq!(manifest.meta.api_version(), Some("registry/2.0"));
    assert!(manifest.meta.operation().is_some());
    let by_digest = repo
        .manifest(&Reference::digest(manifest.digest.clone()), &accept)
        .await
        .unwrap();
    assert_eq!(by_digest.bytes, manifest.bytes);
    for (descriptor, expected) in manifest.image.blobs().zip([CONFIG, LAYER]) {
        assert_eq!(descriptor.size, expected.len() as u64);
        let info = repo.blob_info(&descriptor.digest).await.unwrap().unwrap();
        assert_eq!(info.size, Some(descriptor.size));
        assert_eq!(info.digest.as_ref(), Some(&descriptor.digest));
        let mut reader = repo.blob_for(descriptor).await.unwrap();
        let mut staged = Vec::new();
        while let Some(unverified) = reader.next_chunk().await.unwrap() {
            staged.extend_from_slice(&unverified);
        }
        // Only the successful EOF above authorizes publishing these staged bytes.
        assert_eq!(reader.bytes_read(), descriptor.size);
        assert_eq!(staged, expected);
        assert!(matches!(
            reader.next_chunk().await,
            Err(RegistryError::Input(InputError::ReaderEnded))
        ));
    }
    assert!(
        repo.blob_info(&Digest::sha256_of(b"absent"))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(server.requests().len(), 12);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn failed_catalog_preserves_prefix_and_can_resume_at_the_failed_page() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let observed = attempts.clone();
    let server = TestServer::start(move |request| {
        assert_eq!(request.method, "GET");
        assert_empty(request);
        match request.target.as_str() {
            "/v2/_catalog" => Reply::json(200, br#"{"repositories":["consumer/nested/app"]}"#)
                .header(
                    "link",
                    "</v2/_catalog?n=4&last=consumer%2Fnested%2Fapp>; rel=\"next\"",
                ),
            "/v2/_catalog?n=4&last=consumer%2Fnested%2Fapp" => {
                if observed.fetch_add(1, Ordering::SeqCst) == 0 {
                    error_reply(
                        403,
                        "DENIED",
                        serde_json::json!({"reason":"synthetic policy"}),
                    )
                } else {
                    Reply::json(200, br#"{"repositories":["zeta/service"]}"#)
                }
            }
            _ => panic!("unexpected target {}", request.target),
        }
    })
    .await;
    let registry = registry(&server);
    let items = registry.catalog(ListOptions::default()).items();
    let incomplete = items.collect_complete().await.unwrap_err();
    assert_eq!(incomplete.collected, [NAME]);
    assert_eq!(incomplete.error.outcome(), Outcome::Failed);
    let resume = incomplete.error.resume().unwrap().clone();
    match &incomplete.error {
        PagesError::Provider { error, .. } => {
            assert_eq!(error.status().unwrap().as_u16(), 403);
            assert!(error.has_code(&ErrorCode::Denied));
            assert!(error.meta().unwrap().operation().is_some());
        }
        other => panic!("wrong failure: {other:?}"),
    }
    assert_eq!(
        server.requests().len(),
        2,
        "the SDK does not retry a refused page"
    );
    let rest = registry
        .catalog(ListOptions::default())
        .starting_at(resume)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest, ["zeta/service"]);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(
        server.requests().len(),
        3,
        "resume must not replay page one"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn null_tags_exhaust_but_a_link_to_another_collection_fails() {
    let server = TestServer::start(|request| {
        assert_eq!(request.method, "GET");
        assert_empty(request);
        assert_eq!(request.path(), "/v2/consumer/nested/app/tags/list");
        let reply = Reply::json(200, format!(r#"{{"name":"{NAME}","tags":null}}"#));
        match request.query() {
            None => reply,
            Some("last=broken") => reply.header("link", "</v2/_catalog?last=x>; rel=\"next\""),
            _ => panic!("unexpected query"),
        }
    })
    .await;
    let repo = repository(&registry(&server));
    assert!(
        repo.tags(ListOptions::default())
            .items()
            .collect_complete()
            .await
            .unwrap()
            .is_empty()
    );
    let mut pages = repo.tags(ListOptions::default().after("broken"));
    match pages.next_page().await.unwrap_err() {
        PagesError::Provider { error, .. } => {
            assert!(matches!(
                error.protocol_problem(),
                Some(ProtocolProblem::OtherCollection)
            ));
        }
        other => panic!("wrong failure: {other:?}"),
    }
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(
        server.requests().len(),
        2,
        "the foreign collection was never requested"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn missing_manifest_keeps_typed_provider_document_and_redacted_diagnostics() {
    let server = TestServer::start(|request| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.target, "/v2/consumer/nested/app/manifests/missing");
        assert_eq!(request.header("accept"), Some(DOCKER));
        assert_empty(request);
        Reply::json(404, MISSING)
    })
    .await;
    let error = repository(&registry(&server))
        .manifest(
            &Reference::tag("missing").unwrap(),
            &[ManifestMediaType::DockerV2],
        )
        .await
        .unwrap_err();
    assert_eq!(error.status().unwrap().as_u16(), 404);
    assert!(error.has_code(&ErrorCode::ManifestUnknown));
    assert!(error.has_code(&ErrorCode::Unknown("FIRST_USE_EXTENSION".into())));
    let api = error.api().unwrap();
    assert_eq!(
        api.errors()[0].detail.as_ref().unwrap()["reference"],
        "missing"
    );
    assert!(api.operation.is_some());
    assert!(
        !error
            .to_string()
            .contains("synthetic missing manifest detail")
    );
    assert!(!error.to_string().contains(&server.url()));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

async fn broken_blob(body: Body) -> RegistryError {
    let digest = Digest::sha256_of(LAYER);
    let expected = digest.clone();
    let body = Mutex::new(Some(body));
    let server = TestServer::start(move |request| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.target, format!("/v2/{NAME}/blobs/{expected}"));
        assert_empty(request);
        Reply::new(200)
            .header("content-type", "application/octet-stream")
            .with_body(body.lock().unwrap().take().expect("one blob request"))
    })
    .await;
    let mut reader = repository(&registry(&server))
        .blob(&digest, Some(LAYER.len() as u64))
        .await
        .unwrap();
    let error = loop {
        match reader.next_chunk().await {
            Ok(Some(_)) => {}
            Ok(None) => panic!("damaged blob was accepted as verified"),
            Err(error) => break error,
        }
    };
    assert!(error.meta().unwrap().operation().is_some());
    assert!(matches!(
        reader.next_chunk().await,
        Err(RegistryError::Input(InputError::ReaderEnded))
    ));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
    error
}

#[tokio::test]
async fn clean_short_blob_is_not_successful_eof() {
    let error = broken_blob(Body::Chunks(vec![(
        Duration::ZERO,
        Bytes::from_static(&LAYER[..8]),
    )]))
    .await;
    assert!(
        matches!(error.integrity_problem(), Some(IntegrityProblem::SizeMismatch { expected, actual: 8 })
        if *expected == LAYER.len() as u64)
    );
}

#[tokio::test]
async fn broken_transport_does_not_turn_a_prefix_into_a_verified_blob() {
    let error = broken_blob(Body::FailAfter(vec![Bytes::from_static(&LAYER[..8])])).await;
    assert_eq!(error.request_kind(), Some(concord::ErrorKind::Transport));
    assert_eq!(error.meta().unwrap().status().as_u16(), 200);
}

#[tokio::test]
async fn same_length_corruption_fails_digest_verification() {
    let mut damaged = LAYER.to_vec();
    damaged[0] ^= 1;
    let error = broken_blob(Body::Chunks(vec![(Duration::ZERO, Bytes::from(damaged))])).await;
    assert!(
        matches!(error.integrity_problem(), Some(IntegrityProblem::DigestMismatch { expected, .. })
        if *expected == Digest::sha256_of(LAYER))
    );
}

#[tokio::test]
async fn unaccepted_manifest_media_type_is_rejected_before_a_stalled_body() {
    let server = TestServer::start(|request| {
        assert_eq!(request.target, "/v2/consumer/nested/app/manifests/stable");
        assert_eq!(request.header("accept"), Some(DOCKER));
        Reply::new(200)
            .header("content-type", "text/html")
            .with_body(Body::Stall)
    })
    .await;
    let error = repository(&registry(&server))
        .manifest(
            &Reference::tag("stable").unwrap(),
            &[ManifestMediaType::DockerV2],
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error.protocol_problem(),
        Some(ProtocolProblem::UnexpectedMediaType)
    ));
    assert_eq!(error.meta().unwrap().status().as_u16(), 200);
    assert_eq!(server.shutdown().await.open_after, 0);
}

const PULL_TOKEN: &str = "synthetic-first-use-pull-token";
const PUSH_TOKEN: &str = "synthetic-first-use-push-token";
const SERVICE: &str = "synthetic-registry.example.test";

// Independent fake-provider hashing: do not call the SDK's digest implementation here.
fn stored_digest(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let hex = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

fn challenge(realm: &str, scope: &str) -> Reply {
    error_reply(401, "UNAUTHORIZED", serde_json::Value::Null).header(
        "www-authenticate",
        format!("Bearer realm=\"{realm}/token\",service=\"{SERVICE}\",scope=\"{scope}\""),
    )
}

#[derive(Default)]
struct FakeUpload {
    bytes: Vec<u8>,
    location: String,
    patches: usize,
    statuses: usize,
}

#[derive(Default)]
struct FakeRegistry {
    uploads: BTreeMap<String, FakeUpload>,
    blobs: BTreeMap<String, Vec<u8>>,
    published: Option<Vec<u8>>,
    serial: usize,
    lost_reply: bool,
}

fn upload_location(id: usize, stage: &str) -> String {
    format!("/v2/{NAME}/blobs/uploads/session-{id}?keep=%2f&_state={stage}%2Bopaque")
}

impl FakeRegistry {
    fn respond(&mut self, request: &Recorded) -> Reply {
        let start = format!("/v2/{NAME}/blobs/uploads/");
        if request.method == "POST" && request.path() == start {
            assert_empty(request);
            self.serial += 1;
            let path = format!("{start}session-{}", self.serial);
            let location = upload_location(self.serial, "start");
            self.uploads.insert(
                path,
                FakeUpload {
                    location: location.clone(),
                    ..FakeUpload::default()
                },
            );
            return Reply::new(202)
                .header("location", location)
                .header("range", "0-0")
                .header(
                    "docker-upload-uuid",
                    "synthetic-correlation-not-the-session-path",
                );
        }
        if request.path().starts_with(&start) {
            let session = self
                .uploads
                .get_mut(request.path())
                .expect("known upload path");
            match request.method.as_str() {
                "PATCH" => {
                    assert_eq!(
                        request.target, session.location,
                        "use the latest opaque Location"
                    );
                    assert_eq!(
                        request.header("content-type"),
                        Some("application/octet-stream")
                    );
                    assert_eq!(
                        request
                            .header("content-length")
                            .unwrap()
                            .parse::<usize>()
                            .unwrap(),
                        request.body.len()
                    );
                    let expected_range = format!(
                        "{}-{}",
                        session.bytes.len(),
                        session.bytes.len() + request.body.len() - 1
                    );
                    assert_eq!(
                        request.header("content-range"),
                        Some(expected_range.as_str())
                    );
                    session.patches += 1;
                    if !self.lost_reply && self.serial == 1 && session.patches == 2 {
                        // Received the entire HTTP request, committed only five bytes, then lost the reply.
                        session.bytes.extend_from_slice(&request.body[..5]);
                        self.lost_reply = true;
                        return Reply::hang_up();
                    }
                    session.bytes.extend_from_slice(&request.body);
                    session.location =
                        upload_location(self.serial, &format!("patch{}", session.patches));
                    Reply::new(202)
                        .header("location", session.location.clone())
                        .header("range", format!("bytes=0-{}", session.bytes.len() - 1))
                }
                "GET" => {
                    assert_eq!(request.target, session.location);
                    assert_empty(request);
                    session.statuses += 1;
                    assert_eq!(session.bytes.len(), 15, "authoritative committed prefix");
                    session.location = upload_location(self.serial, "recovered");
                    Reply::new(204)
                        .header("location", session.location.clone())
                        .header("range", format!("0-{}", session.bytes.len() - 1))
                }
                "PUT" => {
                    assert!(
                        request
                            .target
                            .starts_with(&format!("{}&digest=", session.location))
                    );
                    assert_eq!(request.query_pairs().len(), 3);
                    assert_eq!(
                        request.header("content-type"),
                        Some("application/octet-stream")
                    );
                    session.bytes.extend_from_slice(&request.body);
                    let digest = stored_digest(&session.bytes);
                    if request.query_all("digest") != [digest.clone()] {
                        return error_reply(
                            400,
                            "DIGEST_INVALID",
                            serde_json::json!({"digest": request.query_all("digest")[0]}),
                        );
                    }
                    let bytes = session.bytes.clone();
                    self.blobs.insert(digest.clone(), bytes);
                    self.uploads.remove(request.path());
                    Reply::new(201)
                        .header("docker-content-digest", digest.clone())
                        .header("location", format!("/v2/{NAME}/blobs/{digest}"))
                }
                _ => panic!("unexpected upload method"),
            }
        } else if request.path() == format!("/v2/{NAME}/manifests/released") {
            match request.method.as_str() {
                "PUT" => {
                    assert_eq!(request.header("content-type"), Some(DOCKER));
                    let image: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                    assert_eq!(image["schemaVersion"], 2);
                    assert_eq!(image["mediaType"], DOCKER);
                    let descriptors = std::iter::once(&image["config"])
                        .chain(image["layers"].as_array().unwrap());
                    for descriptor in descriptors {
                        let digest = descriptor["digest"].as_str().unwrap();
                        match self.blobs.get(digest) {
                            Some(bytes) => {
                                assert_eq!(descriptor["size"].as_u64(), Some(bytes.len() as u64))
                            }
                            None => {
                                return error_reply(
                                    400,
                                    "BLOB_UNKNOWN",
                                    serde_json::json!({"digest":digest}),
                                );
                            }
                        }
                    }
                    self.published = Some(request.body.clone());
                    Reply::new(201)
                        .header("docker-content-digest", stored_digest(&request.body))
                        .header(
                            "location",
                            format!("/v2/{NAME}/manifests/{}", stored_digest(&request.body)),
                        )
                }
                "GET" => {
                    assert_empty(request);
                    assert_eq!(request.header("accept"), Some(DOCKER));
                    let bytes = self.published.as_ref().expect("published first");
                    Reply::text(200, DOCKER, bytes)
                        .header("docker-content-digest", stored_digest(bytes))
                }
                _ => panic!("unexpected manifest method"),
            }
        } else if let Some(digest) = request.path().strip_prefix(&format!("/v2/{NAME}/blobs/")) {
            assert_empty(request);
            match (request.method.as_str(), self.blobs.get(digest)) {
                ("HEAD", None) => Reply::new(404).with_body(Body::HeadOnly(0)),
                ("HEAD", Some(bytes)) => Reply::new(200)
                    .with_body(Body::HeadOnly(bytes.len() as u64))
                    .header("docker-content-digest", digest),
                ("GET", Some(bytes)) => Reply::new(200)
                    .body(bytes)
                    .header("docker-content-digest", digest),
                _ => panic!("unexpected blob read"),
            }
        } else {
            panic!("unexpected request {request:?}");
        }
    }
}

#[tokio::test]
async fn resume_uncertain_chunk_then_publish_and_read_back_with_scoped_auth() {
    let auth = TestServer::start(|request| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path(), "/token");
        assert_eq!(request.query_all("service"), [SERVICE]);
        assert_eq!(request.query_pairs().len(), 2);
        assert_eq!(
            request.header("authorization"),
            None,
            "anonymous token exchange"
        );
        assert_empty(request);
        match request.query_all("scope").as_slice() {
            [scope] if scope == &format!("repository:{NAME}:pull") => Reply::json(
                200,
                format!(r#"{{"access_token":"{PULL_TOKEN}","expires_in":3600}}"#),
            ),
            [scope] if scope == &format!("repository:{NAME}:pull,push") => Reply::json(
                200,
                format!(r#"{{"token":"{PUSH_TOKEN}","expires_in":3600}}"#),
            ),
            scopes => panic!("wrong token scope: {scopes:?}"),
        }
    })
    .await;
    let state = Arc::new(Mutex::new(FakeRegistry::default()));
    let shared = state.clone();
    let realm = auth.url();
    let server = TestServer::start(move |request| {
        let pull = matches!(request.method.as_str(), "GET" | "HEAD")
            && !request.path().contains("/uploads/");
        let (scope, token) = if pull {
            (format!("repository:{NAME}:pull"), PULL_TOKEN)
        } else {
            (format!("repository:{NAME}:pull,push"), PUSH_TOKEN)
        };
        if request.header("authorization") != Some(format!("Bearer {token}").as_str()) {
            assert_eq!(
                request.header("authorization"),
                None,
                "a cached token for another access must not cross scopes"
            );
            return challenge(&realm, &scope);
        }
        shared.lock().unwrap().respond(request)
    })
    .await;
    let registry =
        registry(&server).token_auth(TokenAuth::anonymous().trust_realm(&auth.url()).unwrap());
    let repo = repository(&registry);
    let layer = Bytes::from_static(LAYER);
    let digest = Digest::sha256_of(&layer);
    assert!(repo.blob_info(&digest).await.unwrap().is_none()); // obtains a pull token
    let mut upload = repo.start_upload().await.unwrap(); // must obtain a separate push token
    assert_eq!(upload.offset(), 0);
    assert_eq!(
        upload.uuid(),
        Some("synthetic-correlation-not-the-session-path")
    );
    assert!(!upload.needs_status());
    let mut recovered = false;
    let mut attempts = 0;
    while upload.offset() < layer.len() as u64 {
        attempts += 1;
        assert!(attempts <= 10, "application recovery budget");
        let start = upload.offset() as usize;
        let end = layer.len().min(start + 10);
        match upload.patch(layer.slice(start..end)).await {
            Ok(progress) => {
                assert_eq!(progress.offset, end as u64);
                assert_eq!(progress.meta.status().as_u16(), 202);
            }
            Err(error) => {
                assert_eq!(error.request_kind(), Some(concord::ErrorKind::Transport));
                assert!(upload.needs_status());
                assert_eq!(upload.offset(), 10);
                let sent = server.requests().len();
                assert!(matches!(
                    upload.patch(Bytes::from_static(b"blocked")).await,
                    Err(RegistryError::Input(InputError::StatusRequired))
                ));
                assert!(matches!(
                    upload.complete(&digest, None).await,
                    Err(RegistryError::Input(InputError::StatusRequired))
                ));
                assert_eq!(
                    server.requests().len(),
                    sent,
                    "uncertain-state guards send nothing"
                );
                let progress = upload.status().await.unwrap();
                assert_eq!(progress.offset, 15);
                assert_eq!(progress.meta.status().as_u16(), 204);
                assert!(!upload.needs_status());
                recovered = true;
            }
        }
    }
    assert!(recovered);
    let pushed = upload.complete(&digest, None).await.unwrap();
    assert_eq!(pushed.size, LAYER.len() as u64);
    assert_eq!(pushed.digest.to_string(), stored_digest(LAYER));
    assert_eq!(pushed.meta.status().as_u16(), 201);
    assert!(upload.is_finished());
    let sent = server.requests().len();
    assert!(matches!(
        upload.patch(Bytes::from_static(b"late")).await,
        Err(RegistryError::Input(InputError::UploadFinished))
    ));
    assert!(matches!(
        upload.status().await,
        Err(RegistryError::Input(InputError::UploadFinished))
    ));
    assert!(matches!(
        upload.complete(&digest, None).await,
        Err(RegistryError::Input(InputError::UploadFinished))
    ));
    assert_eq!(server.requests().len(), sent);
    let config = repo
        .push_blob(Bytes::from_static(CONFIG), NonZeroUsize::new(128).unwrap())
        .await
        .unwrap();
    assert_eq!(config.size, CONFIG.len() as u64);
    let image = ImageManifest::new(
        ManifestMediaType::DockerV2,
        Descriptor::new(
            "application/vnd.docker.container.image.v1+json",
            config.size,
            config.digest,
        ),
        vec![Descriptor::new(
            "application/vnd.docker.image.rootfs.diff.tar.gzip",
            pushed.size,
            pushed.digest,
        )],
    );
    let manifest = image.to_upload().unwrap();
    let reference = Reference::tag("released").unwrap();
    let published = repo.put_manifest(&reference, &manifest).await.unwrap();
    assert_eq!(&published.digest, manifest.digest());
    let pulled = repo
        .manifest(&reference, &[ManifestMediaType::DockerV2])
        .await
        .unwrap();
    assert_eq!(&pulled.bytes, manifest.bytes());
    assert_eq!(pulled.digest, published.digest);
    for descriptor in pulled.image.blobs() {
        let bytes = repo
            .blob_for(descriptor)
            .await
            .unwrap()
            .read_to_end(descriptor.size)
            .await
            .unwrap();
        assert!(bytes.as_ref() == CONFIG || bytes.as_ref() == LAYER);
    }
    {
        let state = state.lock().unwrap();
        assert!(state.uploads.is_empty());
        assert_eq!(state.blobs.get(&stored_digest(LAYER)).unwrap(), LAYER);
        assert_eq!(state.blobs.get(&stored_digest(CONFIG)).unwrap(), CONFIG);
        assert_eq!(state.published.as_deref(), Some(manifest.bytes().as_ref()));
    }
    assert_eq!(
        auth.requests().len(),
        2,
        "one token per access, cached across calls"
    );
    let requests = server.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "GET" && r.path().contains("/uploads/"))
            .count(),
        1
    );
    assert_eq!(requests.iter().filter(|r| r.method == "PATCH").count(), 6);
    assert_eq!(
        requests.iter().filter(|r| r.method == "POST").count(),
        3,
        "challenge retries only the start"
    );
    for diagnostic in [format!("{registry:?}"), format!("{upload:?}")] {
        assert!(!diagnostic.contains(PULL_TOKEN));
        assert!(!diagnostic.contains(PUSH_TOKEN));
        assert!(!diagnostic.contains("_state="));
    }
    assert_eq!(server.shutdown().await.open_after, 0);
    assert_eq!(auth.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn range_refusal_moves_the_session_and_one_byte_first_patch_is_local() {
    let sequence = Arc::new(AtomicUsize::new(0));
    let observed = sequence.clone();
    let first = "/v2/consumer/nested/app/blobs/uploads/range-test?state=first%2Bopaque";
    let second = "/v2/consumer/nested/app/blobs/uploads/range-test?state=after%2frefusal";
    let third = "/v2/consumer/nested/app/blobs/uploads/range-test?state=after%2Bpatch";
    let server = TestServer::start(
        move |request| match observed.fetch_add(1, Ordering::SeqCst) {
            0 => {
                assert_eq!(request.method, "POST");
                assert_eq!(request.target, "/v2/consumer/nested/app/blobs/uploads/");
                assert_empty(request);
                Reply::new(202)
                    .header("location", first)
                    .header("range", "0-0")
            }
            1 => {
                assert_eq!(request.method, "PATCH");
                assert_eq!(request.target, first);
                assert_eq!(request.header("content-range"), Some("0-4"));
                assert_eq!(request.body, b"abcde");
                error_reply(416, "RANGE_INVALID", serde_json::Value::Null)
                    .header("location", second)
                    .header("range", "0-1")
            }
            2 => {
                assert_eq!(request.method, "PATCH");
                assert_eq!(request.target, second);
                assert_eq!(request.header("content-range"), Some("2-4"));
                assert_eq!(request.body, b"cde");
                Reply::new(202)
                    .header("location", third)
                    .header("range", "0-4")
            }
            3 => {
                assert_eq!(request.method, "PUT");
                assert!(request.target.starts_with(&format!("{third}&digest=")));
                assert_eq!(request.query_all("digest"), [stored_digest(b"abcde")]);
                assert_empty(request);
                Reply::new(201).header("docker-content-digest", stored_digest(b"abcde"))
            }
            _ => panic!("unexpected retry"),
        },
    )
    .await;
    let repo = repository(&registry(&server));
    let mut upload = repo.start_upload().await.unwrap();
    let sent = server.requests().len();
    assert!(matches!(
        upload.patch(Bytes::from_static(b"a")).await,
        Err(RegistryError::Input(InputError::OneByteFirstChunk))
    ));
    assert_eq!(server.requests().len(), sent);
    assert_eq!(upload.offset(), 0);
    assert!(!upload.needs_status());
    let error = upload
        .patch(Bytes::from_static(b"abcde"))
        .await
        .unwrap_err();
    assert_eq!(error.status().unwrap().as_u16(), 416);
    assert!(error.has_code(&ErrorCode::RangeInvalid));
    assert_eq!(upload.offset(), 2);
    assert!(
        !upload.needs_status(),
        "416 is already an authoritative status answer"
    );
    upload.patch(Bytes::from_static(b"cde")).await.unwrap();
    assert_eq!(upload.offset(), 5);
    let done = upload
        .complete(&Digest::sha256_of(b"abcde"), None)
        .await
        .unwrap();
    assert_eq!(done.size, 5);
    assert!(upload.is_finished());
    assert_eq!(sequence.load(Ordering::SeqCst), 4);
    server.shutdown().await;
}

#[tokio::test]
async fn digest_invalid_retains_provider_detail_and_application_restarts() {
    let state = Arc::new(Mutex::new(FakeRegistry::default()));
    let shared = state.clone();
    let server = TestServer::start(move |request| shared.lock().unwrap().respond(request)).await;
    let repo = repository(&registry(&server));
    let digest = Digest::sha256_of(LAYER);
    let mut failed = repo.start_upload().await.unwrap();
    let error = failed
        .complete(&digest, Some(Bytes::from_static(b"incorrect content")))
        .await
        .unwrap_err();
    assert_eq!(error.status().unwrap().as_u16(), 400);
    assert!(error.has_code(&ErrorCode::DigestInvalid));
    assert_eq!(
        error.api().unwrap().errors()[0].detail_digest(),
        Some(digest.clone())
    );
    assert!(failed.needs_status());
    assert!(!failed.is_finished());
    assert!(state.lock().unwrap().blobs.is_empty());
    // Per spec the application restarts, rather than trying to publish the failed session.
    let mut fresh = repo.start_upload().await.unwrap();
    let done = fresh
        .complete(&digest, Some(Bytes::from_static(LAYER)))
        .await
        .unwrap();
    assert_eq!(done.size, LAYER.len() as u64);
    assert!(fresh.is_finished());
    assert_eq!(state.lock().unwrap().serial, 2);
    assert_eq!(
        state
            .lock()
            .unwrap()
            .blobs
            .get(&stored_digest(LAYER))
            .unwrap(),
        LAYER
    );
    assert_eq!(server.requests().len(), 4);
    server.shutdown().await;
}

#[tokio::test]
async fn publishing_a_manifest_with_an_absent_blob_keeps_the_missing_digest() {
    let state = Arc::new(Mutex::new(FakeRegistry::default()));
    let shared = state.clone();
    let server = TestServer::start(move |request| shared.lock().unwrap().respond(request)).await;
    let repo = repository(&registry(&server));
    let image = ImageManifest::new(
        ManifestMediaType::DockerV2,
        Descriptor::of("application/vnd.docker.container.image.v1+json", CONFIG),
        vec![Descriptor::of(
            "application/vnd.docker.image.rootfs.diff.tar.gzip",
            LAYER,
        )],
    );
    let manifest = image.to_upload().unwrap();
    let error = repo
        .put_manifest(&Reference::tag("released").unwrap(), &manifest)
        .await
        .unwrap_err();
    assert_eq!(error.status().unwrap().as_u16(), 400);
    assert!(error.has_code(&ErrorCode::BlobUnknown));
    assert_eq!(
        error.api().unwrap().errors()[0].detail_digest(),
        Some(Digest::sha256_of(CONFIG))
    );
    assert!(state.lock().unwrap().published.is_none());
    assert_eq!(server.single().body, manifest.bytes().as_ref());
    server.shutdown().await;
}

#[tokio::test]
async fn a_one_byte_blob_uses_the_completing_put_instead_of_an_ambiguous_patch() {
    let state = Arc::new(Mutex::new(FakeRegistry::default()));
    let shared = state.clone();
    let server = TestServer::start(move |request| shared.lock().unwrap().respond(request)).await;
    let done = repository(&registry(&server))
        .push_blob(Bytes::from_static(b"x"), NonZeroUsize::new(8).unwrap())
        .await
        .unwrap();
    assert_eq!(done.size, 1);
    assert_eq!(done.digest.to_string(), stored_digest(b"x"));
    assert_eq!(
        server
            .requests()
            .iter()
            .map(|r| r.method.as_str())
            .collect::<Vec<_>>(),
        ["POST", "PUT"]
    );
    assert_eq!(
        state
            .lock()
            .unwrap()
            .blobs
            .get(&stored_digest(b"x"))
            .unwrap(),
        b"x"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn untrusted_token_realm_is_refused_before_contacting_it() {
    let auth = TestServer::start(|_| panic!("untrusted realm must not be contacted")).await;
    let realm = auth.url();
    let server = TestServer::start(move |request| {
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/v2/consumer/nested/app/blobs/uploads/");
        assert_eq!(request.header("authorization"), None);
        challenge(&realm, &format!("repository:{NAME}:pull,push"))
    })
    .await;
    let registry = registry(&server).token_auth(TokenAuth::anonymous());
    let error = repository(&registry).start_upload().await.unwrap_err();
    assert!(matches!(
        error.auth_problem(),
        Some(AuthProblem::UntrustedRealm)
    ));
    assert_eq!(server.requests().len(), 1);
    assert!(auth.requests().is_empty());
    server.shutdown().await;
    auth.shutdown().await;
}

#[tokio::test]
async fn a_second_unauthorized_response_is_final_and_does_not_fetch_another_token() {
    let auth = TestServer::start(|request| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path(), "/token");
        assert_eq!(request.query_all("service"), [SERVICE]);
        assert_eq!(
            request.query_all("scope"),
            [format!("repository:{NAME}:pull,push")]
        );
        Reply::json(200, format!(r#"{{"token":"{PUSH_TOKEN}"}}"#))
    })
    .await;
    let realm = auth.url();
    let attempts = Arc::new(AtomicUsize::new(0));
    let observed = attempts.clone();
    let server = TestServer::start(move |request| {
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/v2/consumer/nested/app/blobs/uploads/");
        assert_empty(request);
        let prior = observed.fetch_add(1, Ordering::SeqCst);
        match prior {
            0 => assert_eq!(request.header("authorization"), None),
            1 => assert_eq!(
                request.header("authorization"),
                Some(format!("Bearer {PUSH_TOKEN}").as_str())
            ),
            _ => panic!("challenge loop must stop after one answer"),
        }
        challenge(&realm, &format!("repository:{NAME}:pull,push"))
    })
    .await;
    let registry =
        registry(&server).token_auth(TokenAuth::anonymous().trust_realm(&auth.url()).unwrap());
    let error = repository(&registry).start_upload().await.unwrap_err();
    assert_eq!(error.status().unwrap().as_u16(), 401);
    assert!(error.has_code(&ErrorCode::Unauthorized));
    assert!(error.api().unwrap().challenge.is_some());
    for diagnostic in [error.to_string(), format!("{error:?}")] {
        assert!(!diagnostic.contains(PUSH_TOKEN));
    }
    assert!(!error.to_string().contains(&auth.url()));
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(auth.requests().len(), 1);
    server.shutdown().await;
    auth.shutdown().await;
}
