//! Wire-level tests: what goes out (method, target, headers, body bytes) and how each documented
//! answer — and each meaningful violation — is read. Scripted loopback servers; the stateful fake
//! of `support` where an exchange needs state.

mod support;

use std::num::NonZeroUsize;
use std::sync::{Arc, OnceLock};

use concord_example_testserver::{Body, Reply, Script, TestServer};
use concord_pagination::PagesError;
use concord_sdk_distribution_registry::concord::bytes::Bytes;
use concord_sdk_distribution_registry::{
    AuthProblem, BasicCredentials, Digest, ErrorBody, ErrorCode, ImageManifest, InputError,
    IntegrityProblem, ListOptions, ManifestMediaType, ManifestUpload, PageSize, ProtocolProblem,
    Reference, Registry, RegistryError, Repository, RepositoryName, TokenAuth,
};
use support::*;

fn repo(registry: &Registry, name: &str) -> Repository {
    registry
        .repository(&RepositoryName::new(name).unwrap())
        .unwrap()
}

/// The first page's provider error of a listing.
async fn first_error(mut pages: concord_sdk_distribution_registry::TagPages) -> RegistryError {
    match pages.next_page().await {
        Err(PagesError::Provider { error, .. }) => error,
        other => panic!("expected a provider error, got {other:?}"),
    }
}

/// A server whose replies are built after it started, from its own URL.
async fn self_aware(build: impl Fn(&str) -> Vec<Reply> + Send + Sync + 'static) -> TestServer {
    let url: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
    let script: Arc<OnceLock<Script>> = Arc::new(OnceLock::new());
    let (u, s) = (Arc::clone(&url), Arc::clone(&script));
    let server = TestServer::start(move |_| {
        s.get_or_init(|| Script::new(build(u.get().unwrap())))
            .next()
    })
    .await;
    url.set(server.url()).unwrap();
    server
}

// ---------------------------------------------------------------------------------------------
// GET /v2/

#[tokio::test]
async fn ping_is_get_v2_and_reads_the_version_header() {
    let server = scripted([empty(200)
        .header("content-type", "application/json")
        .body("{}")])
    .await;
    let version = registry(&server, None).ping().await.unwrap();
    assert_eq!(version.version(), Some("registry/2.0"));
    let request = server.single();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("GET", "/v2/")
    );
    assert_eq!(request.header("authorization"), None);
    assert_eq!(request.header("content-type"), None);
    assert!(request.body.is_empty());
}

#[tokio::test]
async fn ping_404_means_v2_is_not_implemented() {
    let server = scripted([Reply::new(404)]).await;
    let error = registry(&server, None).ping().await.unwrap_err();
    assert_eq!(error.status().map(|s| s.as_u16()), Some(404));
    assert!(matches!(error.api().unwrap().body, ErrorBody::Empty));
}

#[tokio::test]
async fn a_registry_base_path_prefix_is_kept() {
    let server = scripted([empty(200), json(200, TAGS_NULL)]).await;
    let registry = Registry::new(http(), &format!("{}/mirror/", server.url())).unwrap();
    registry.ping().await.unwrap();
    let page = repo(&registry, "acme/app")
        .tags(ListOptions::default())
        .next_page()
        .await
        .unwrap()
        .unwrap();
    assert!(page.items.is_empty() && page.next.is_none());
    let targets: Vec<String> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(targets, ["/mirror/v2/", "/mirror/v2/acme/app/tags/list"]);
}

#[test]
fn registry_urls_are_validated() {
    for bad in [
        "registry.example.test",
        "ftp://registry.example.test",
        "https://user:pw@registry.example.test",
        "https://registry.example.test/?x=1",
        "https://registry.example.test/#f",
    ] {
        let error = Registry::new(http(), bad).unwrap_err();
        assert!(
            matches!(error, RegistryError::Input(InputError::RegistryUrl)),
            "{bad}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The Bearer challenge.

#[tokio::test]
async fn a_challenge_is_answered_once_with_service_and_scope_then_the_token_is_reused() {
    let auth = scripted([json(200, TOKEN)]).await;
    let realm = format!("{}/token", auth.url());
    let server = scripted([
        challenge(&realm, Some("repository:acme/app:pull")),
        json(200, TAGS_NULL),
        json(200, TAGS_NULL),
    ])
    .await;
    let registry = registry(&server, Some(&auth));
    let repository = repo(&registry, "acme/app");
    for _ in 0..2 {
        let page = repository
            .tags(ListOptions::default())
            .next_page()
            .await
            .unwrap();
        assert!(page.unwrap().items.is_empty());
    }

    let token = auth.single();
    assert_eq!(token.method, "GET");
    assert_eq!(
        token.target,
        "/token?service=registry.fake.test&scope=repository%3Aacme%2Fapp%3Apull"
    );
    assert_eq!(
        token.header("authorization"),
        None,
        "anonymous token request"
    );
    assert_eq!(token.header("accept"), Some("application/json"));

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].header("authorization"), None);
    let bearer = format!("Bearer {TOKEN_VALUE}");
    assert_eq!(requests[1].header("authorization"), Some(bearer.as_str()));
    // The cached pull token is sent pre-emptively: no second challenge, no second token request.
    assert_eq!(requests[2].header("authorization"), Some(bearer.as_str()));
    assert!(
        requests
            .iter()
            .all(|r| r.target == "/v2/acme/app/tags/list")
    );
}

#[tokio::test]
async fn several_challenge_scopes_become_repeated_scope_parameters_and_client_id_is_sent() {
    let auth = scripted([json(200, TOKEN_ACCESS)]).await;
    let realm = format!("{}/token", auth.url());
    let server = scripted([
        challenge(&realm, Some("repository:acme/app:pull registry:catalog:*")),
        json(200, CATALOG_2),
    ])
    .await;
    let registry = Registry::new(http(), &server.url()).unwrap().token_auth(
        TokenAuth::anonymous()
            .trust_realm(&auth.url())
            .unwrap()
            .client_id("concord-sdk-tests"),
    );
    let page = registry
        .catalog(ListOptions::default())
        .next_page()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(page.items, ["library/base"]);
    let token = auth.single();
    assert_eq!(
        token.query_all("scope"),
        ["repository:acme/app:pull", "registry:catalog:*"]
    );
    assert_eq!(token.query_all("client_id"), ["concord-sdk-tests"]);
    // `access_token` is accepted as the OAuth 2.0 alias of `token`.
    let bearer = format!("Bearer {TOKEN_ACCESS_VALUE}");
    assert_eq!(
        server.requests()[1].header("authorization"),
        Some(bearer.as_str())
    );
}

#[tokio::test]
async fn a_challenge_without_scope_asks_for_the_operations_scope() {
    let auth = scripted([json(200, TOKEN)]).await;
    let realm = format!("{}/token", auth.url());
    let server = scripted([challenge(&realm, None), json(200, CATALOG_2)]).await;
    let registry = registry(&server, Some(&auth));
    registry
        .catalog(ListOptions::default())
        .next_page()
        .await
        .unwrap();
    assert_eq!(auth.single().query_all("scope"), ["registry:catalog:*"]);
}

#[tokio::test]
async fn basic_credentials_go_to_the_trusted_realm_only() {
    let auth = scripted([json(200, TOKEN)]).await;
    let realm = format!("{}/token", auth.url());
    let server = scripted([challenge(&realm, None), empty(200)]).await;
    let credentials = BasicCredentials::new(USERNAME, PASSWORD).unwrap();
    let registry = Registry::new(http(), &server.url())
        .unwrap()
        .token_auth(TokenAuth::basic(credentials).trust_realm(&realm).unwrap());
    registry.ping().await.unwrap();

    let token = auth.single();
    assert_eq!(token.header("authorization"), Some(BASIC));
    assert_eq!(
        token.target, "/token?service=registry.fake.test",
        "no scope for /v2/"
    );
    for request in server.requests() {
        assert!(
            !request
                .header("authorization")
                .unwrap_or("")
                .starts_with("Basic"),
            "the registry never sees the Basic credentials"
        );
    }
    let shown = format!("{registry:?}");
    assert!(
        !shown.contains(PASSWORD) && !shown.contains(BASIC),
        "{shown}"
    );
}

#[tokio::test]
async fn an_untrusted_realm_is_never_contacted() {
    let rogue = scripted([json(200, TOKEN)]).await;
    let trusted = scripted([]).await;
    let realm = format!("{}/token", rogue.url());
    let server = scripted([challenge(&realm, Some("repository:acme/app:pull"))]).await;
    let credentials = BasicCredentials::new(USERNAME, PASSWORD).unwrap();
    let registry = Registry::new(http(), &server.url()).unwrap().token_auth(
        TokenAuth::basic(credentials)
            .trust_realm(&trusted.url())
            .unwrap(),
    );
    let error = first_error(repo(&registry, "acme/app").tags(ListOptions::default())).await;
    assert_eq!(error.auth_problem(), Some(AuthProblem::UntrustedRealm));
    assert_eq!(error.meta().unwrap().status().as_u16(), 401);
    assert!(
        rogue.requests().is_empty(),
        "nothing was sent to the untrusted realm"
    );
    assert!(trusted.requests().is_empty());
    assert_eq!(server.requests().len(), 1);
    assert_hides(&error, &[PASSWORD, BASIC]);
}

#[tokio::test]
async fn a_realm_with_a_query_or_user_information_is_refused() {
    let auth = scripted([]).await;
    for realm in [
        format!("{}/token?tenant=x", auth.url()),
        format!("http://user@{}/token", auth.addr()),
        "not a url".to_owned(),
    ] {
        let server = scripted([challenge(&realm, None)]).await;
        let error = registry(&server, Some(&auth)).ping().await.unwrap_err();
        let problem = error.auth_problem().unwrap();
        assert!(
            matches!(
                problem,
                AuthProblem::InvalidRealm | AuthProblem::UntrustedRealm
            ),
            "{realm}: {problem:?}"
        );
    }
    assert!(auth.requests().is_empty());
}

#[tokio::test]
async fn token_service_failures_are_auth_errors_without_the_token() {
    let auth = scripted([
        json(401, br#"{"details":"incorrect username or password"}"#),
        json(200, TOKEN_MISSING),
        Reply::text(200, "text/html", PROXY_ERROR),
    ])
    .await;
    let realm = format!("{}/token", auth.url());
    let server = scripted([
        challenge(&realm, None),
        challenge(&realm, None),
        challenge(&realm, None),
    ])
    .await;
    let registry = registry(&server, Some(&auth));

    let refused = registry.ping().await.unwrap_err();
    assert_eq!(refused.auth_problem(), Some(AuthProblem::TokenRefused));
    let RegistryError::Auth(auth_error) = &refused else {
        unreachable!()
    };
    assert_eq!(auth_error.status.map(|s| s.as_u16()), Some(401));

    let missing = registry.ping().await.unwrap_err();
    assert_eq!(missing.auth_problem(), Some(AuthProblem::TokenResponse));

    let html = registry.ping().await.unwrap_err();
    assert_eq!(html.auth_problem(), Some(AuthProblem::TokenResponse));
    let RegistryError::Auth(auth_error) = &html else {
        unreachable!()
    };
    assert_eq!(
        auth_error.source_error().map(|e| e.kind()),
        Some(concord_sdk_distribution_registry::concord::ErrorKind::Metadata)
    );
    assert_eq!(server.requests().len(), 3, "no retry without a token");
}

#[tokio::test]
async fn a_second_401_is_the_registrys_answer_not_another_round() {
    let auth = scripted([json(200, TOKEN)]).await;
    let realm = format!("{}/token", auth.url());
    let server = scripted([
        challenge(&realm, Some("repository:acme/app:pull")),
        challenge(&realm, Some("repository:acme/app:pull")).header("x-attempt", "2"),
    ])
    .await;
    let registry = registry(&server, Some(&auth));
    let error = first_error(repo(&registry, "acme/app").tags(ListOptions::default())).await;
    assert_eq!(error.status().map(|s| s.as_u16()), Some(401));
    assert!(error.has_code(&ErrorCode::Unauthorized));
    let challenge = error.api().unwrap().challenge.as_ref().unwrap();
    assert_eq!(challenge.scopes, ["repository:acme/app:pull"]);
    assert_eq!(server.requests().len(), 2);
    assert_eq!(auth.requests().len(), 1);
    assert_hides(&error, &[TOKEN_VALUE]);
}

#[tokio::test]
async fn without_token_auth_a_challenge_is_reported_with_its_parameters() {
    let server = scripted([
        challenge(
            "https://auth.example.test/token",
            Some("repository:acme/app:pull"),
        ),
        error(401, UNAUTHORIZED).header("www-authenticate", r#"Basic realm="registry""#),
    ])
    .await;
    let registry = registry(&server, None);
    let error = first_error(repo(&registry, "acme/app").tags(ListOptions::default())).await;
    let api = error.api().unwrap();
    let challenge = api.challenge.as_ref().unwrap();
    assert_eq!(challenge.realm, "https://auth.example.test/token");
    assert_eq!(challenge.service.as_deref(), Some(SERVICE));
    assert_eq!(api.errors()[0].code, ErrorCode::Unauthorized);
    assert_eq!(
        api.errors()[0].message.as_deref(),
        Some("access to the requested resource is not authorized")
    );

    // A Basic challenge is not a Bearer one: reported, nothing else sent.
    let auth = scripted([]).await;
    let basic = Registry::new(http(), &server.url())
        .unwrap()
        .token_auth(TokenAuth::anonymous().trust_realm(&auth.url()).unwrap());
    let error = first_error(repo(&basic, "acme/app").tags(ListOptions::default())).await;
    assert_eq!(error.status().map(|s| s.as_u16()), Some(401));
    assert!(error.api().unwrap().challenge.is_none());
    assert!(auth.requests().is_empty());
}

#[tokio::test]
async fn a_malformed_bearer_challenge_is_an_auth_error() {
    let auth = scripted([]).await;
    let server = scripted([
        error(401, UNAUTHORIZED).header("www-authenticate", r#"Bearer realm="unterminated"#)
    ])
    .await;
    let error = registry(&server, Some(&auth)).ping().await.unwrap_err();
    assert_eq!(error.auth_problem(), Some(AuthProblem::MalformedChallenge));
    assert!(auth.requests().is_empty());
}

#[tokio::test]
async fn push_and_pull_tokens_are_requested_and_kept_apart() {
    let fake = Fake::start().await;
    fake.seed("acme/app", "v1", OCI);
    let registry = fake.client();
    let repository = repo(&registry, "acme/app");
    repository
        .manifest(&Reference::tag("v1").unwrap(), &[ManifestMediaType::OciV1])
        .await
        .unwrap();
    let mut upload = repository.start_upload().await.unwrap();
    upload
        .patch(Bytes::from_static(b"0123456789"))
        .await
        .unwrap();
    repository
        .manifest(&Reference::tag("v1").unwrap(), &[ManifestMediaType::OciV1])
        .await
        .unwrap();

    let token_scopes: Vec<Vec<String>> = fake.with(|s| {
        s.token_requests
            .iter()
            .map(|(_, scopes, _)| scopes.clone())
            .collect()
    });
    assert_eq!(
        token_scopes,
        [
            vec!["repository:acme/app:pull".to_owned()],
            vec!["repository:acme/app:pull,push".to_owned()]
        ]
    );
    let requests = fake.registry.requests();
    let auth_of = |i: usize| requests[i].header("authorization").map(str::to_owned);
    // GET (401), GET (pull token), POST (no token: none cached for push), POST (push token),
    // PATCH (push token, pre-emptive), GET (pull token, pre-emptive).
    let methods: Vec<&str> = requests.iter().map(|r| r.method.as_str()).collect();
    assert_eq!(methods, ["GET", "GET", "POST", "POST", "PATCH", "GET"]);
    let pull = auth_of(1).unwrap();
    let push = auth_of(3).unwrap();
    assert_ne!(pull, push);
    assert_eq!(
        auth_of(2),
        None,
        "a pull token is never sent with a push request"
    );
    assert_eq!(auth_of(4).as_deref(), Some(push.as_str()));
    assert_eq!(auth_of(5).as_deref(), Some(pull.as_str()));
}

#[tokio::test]
async fn a_token_without_push_is_refused_by_the_registry_once() {
    let fake = Fake::start().await;
    fake.with(|s| s.perturb.pull_only = true);
    let repository = repo(&fake.client(), "acme/app");
    let error = repository.start_upload().await.unwrap_err();
    assert_eq!(error.status().map(|s| s.as_u16()), Some(401));
    assert_eq!(fake.registry.requests().len(), 2);
    assert_eq!(fake.with(|s| s.token_requests.len()), 1);
}

// ---------------------------------------------------------------------------------------------
// Catalog and tags.

#[tokio::test]
async fn the_catalog_follows_the_link_exactly() {
    let server = scripted([
        next(
            json(200, CATALOG_1),
            "/v2/_catalog?last=acme%2Ftools%2Fcli&n=2",
        ),
        json(200, CATALOG_2),
    ])
    .await;
    let registry = registry(&server, None);
    let options = ListOptions::default().page_size(PageSize::new(2).unwrap());
    let names = registry
        .catalog(options)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(names, ["acme/app", "acme/tools/cli", "library/base"]);
    let requests = server.requests();
    assert_eq!(requests[0].target, "/v2/_catalog?n=2");
    assert_eq!(
        requests[1].target,
        "/v2/_catalog?last=acme%2Ftools%2Fcli&n=2"
    );
    assert!(
        requests
            .iter()
            .all(|r| r.header("accept") == Some("application/json"))
    );
}

#[tokio::test]
async fn a_changed_n_and_an_absolute_link_are_followed_as_sent() {
    let server = self_aware(|url| {
        vec![
            next(
                json(200, CATALOG_1),
                &format!("{url}/v2/_catalog?n=50&last=acme%2Ftools%2Fcli"),
            ),
            json(200, CATALOG_2),
        ]
    })
    .await;
    let registry = registry(&server, None);
    let options = ListOptions::default()
        .page_size(PageSize::new(2).unwrap())
        .after("a");
    let names = registry
        .catalog(options)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(names.len(), 3);
    let targets: Vec<String> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(
        targets,
        [
            "/v2/_catalog?n=2&last=a",
            "/v2/_catalog?n=50&last=acme%2Ftools%2Fcli"
        ]
    );
}

#[tokio::test]
async fn links_that_leave_the_listing_are_refused_before_any_request() {
    let rogue = scripted([]).await;
    let cases = [
        (
            format!("{}/v2/_catalog?last=x", rogue.url()),
            ProtocolProblem::ForeignLink,
        ),
        (
            "/v2/acme/app/tags/list?last=x".to_owned(),
            ProtocolProblem::OtherCollection,
        ),
        ("/v2/_catalog?n=2".to_owned(), ProtocolProblem::NotAdvancing),
    ];
    for (link, problem) in cases {
        let server = scripted([next(json(200, CATALOG_1), &link)]).await;
        let mut pages = registry(&server, None)
            .catalog(ListOptions::default().page_size(PageSize::new(2).unwrap()));
        let Err(PagesError::Provider { error, .. }) = pages.next_page().await else {
            panic!("{link}: expected a refusal");
        };
        assert_eq!(error.protocol_problem(), Some(&problem), "{link}");
        assert_eq!(server.requests().len(), 1, "{link}");
        // Not an end: the traversal is over with an error.
        assert!(matches!(
            pages.next_page().await,
            Err(PagesError::Ended { .. })
        ));
    }
    assert!(rogue.requests().is_empty());

    let server =
        scripted([json(200, CATALOG_1).header("link", r#"</v2/_catalog?last=b; rel="next""#)])
            .await;
    let mut pages = registry(&server, None).catalog(ListOptions::default());
    let Err(PagesError::Provider { error, .. }) = pages.next_page().await else {
        panic!()
    };
    assert_eq!(
        error.protocol_problem(),
        Some(&ProtocolProblem::MalformedLink)
    );
}

#[tokio::test]
async fn a_restricted_or_unsupported_catalog_is_the_registrys_answer() {
    let server = scripted([
        error(404, UNSUPPORTED),
        error(403, DENIED),
        error(400, PAGINATION_INVALID),
        error(429, UNKNOWN_CODE).header("retry-after", "30"),
    ])
    .await;
    let registry = registry(&server, None);
    let mut codes = Vec::new();
    for _ in 0..4 {
        let mut pages = registry.catalog(ListOptions::default());
        let Err(PagesError::Provider { error, .. }) = pages.next_page().await else {
            panic!()
        };
        codes.push((
            error.status().unwrap().as_u16(),
            error.api().unwrap().errors()[0].code.clone(),
        ));
    }
    assert_eq!(
        codes,
        [
            (404, ErrorCode::Unsupported),
            (403, ErrorCode::Denied),
            (400, ErrorCode::PaginationNumberInvalid),
            (429, ErrorCode::TooManyRequests),
        ]
    );
}

#[tokio::test]
async fn tags_use_the_multi_segment_name_verbatim() {
    let server = scripted([
        next(
            json(200, TAGS_1),
            "/v2/acme/tools/app/tags/list?last=v2&n=2",
        ),
        json(200, TAGS_2),
    ])
    .await;
    // The fixtures name `acme/app`; this listing is for `acme/tools/app`: the name check fires.
    let registry = registry(&server, None);
    let error = first_error(
        repo(&registry, "acme/tools/app")
            .tags(ListOptions::default().page_size(PageSize::new(2).unwrap())),
    )
    .await;
    assert_eq!(
        error.protocol_problem(),
        Some(&ProtocolProblem::NameMismatch)
    );
    assert_eq!(
        server.requests()[0].target,
        "/v2/acme/tools/app/tags/list?n=2"
    );

    let server = scripted([
        next(json(200, TAGS_1), "/v2/acme/app/tags/list?last=v2&n=2"),
        json(200, TAGS_2),
    ])
    .await;
    let tags = repo(&support::registry(&server, None), "acme/app")
        .tags(ListOptions::default().page_size(PageSize::new(2).unwrap()))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(tags, ["v1", "v2", "v3"]);
    assert_eq!(
        server.requests()[1].target,
        "/v2/acme/app/tags/list?last=v2&n=2"
    );
}

#[tokio::test]
async fn tag_list_errors() {
    let server = scripted([error(404, NAME_UNKNOWN), json(200, TAGS_OTHER_NAME)]).await;
    let registry = registry(&server, None);
    let unknown = first_error(repo(&registry, "acme/app").tags(ListOptions::default())).await;
    assert!(unknown.has_code(&ErrorCode::NameUnknown));
    let other = first_error(repo(&registry, "acme/app").tags(ListOptions::default())).await;
    assert_eq!(
        other.protocol_problem(),
        Some(&ProtocolProblem::NameMismatch)
    );
}

// ---------------------------------------------------------------------------------------------
// Manifests.

#[tokio::test]
async fn a_manifest_by_tag_sends_the_explicit_accept_and_is_verified() {
    let manifest_digest = digest(MANIFEST_OCI);
    let server = scripted([empty(200)
        .header("content-type", OCI)
        .header("docker-content-digest", manifest_digest.as_str())
        .body(MANIFEST_OCI)])
    .await;
    let manifest = repo(&registry(&server, None), "acme/app")
        .manifest(
            &Reference::tag("v1").unwrap(),
            &[ManifestMediaType::OciV1, ManifestMediaType::DockerV2],
        )
        .await
        .unwrap();
    assert_eq!(manifest.media_type, ManifestMediaType::OciV1);
    assert_eq!(manifest.digest, manifest_digest);
    assert_eq!(manifest.bytes.as_ref(), MANIFEST_OCI);
    assert_eq!(manifest.image.layers.len(), 1);
    assert_eq!(manifest.image.config.digest, digest(CONFIG));
    let request = server.single();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("GET", "/v2/acme/app/manifests/v1")
    );
    assert_eq!(
        request.header("accept"),
        Some(format!("{OCI}, {DOCKER_V2}").as_str())
    );
}

#[tokio::test]
async fn a_manifest_by_digest_uses_the_digest_verbatim_in_the_path() {
    let manifest_digest = digest(MANIFEST_DOCKER);
    let server = scripted([empty(200)
        .header("content-type", DOCKER_V2)
        .body(MANIFEST_DOCKER)])
    .await;
    let manifest = repo(&registry(&server, None), "acme/app")
        .manifest(
            &Reference::digest(manifest_digest.clone()),
            &[ManifestMediaType::DockerV2],
        )
        .await
        .unwrap();
    assert_eq!(manifest.digest, manifest_digest);
    assert_eq!(
        server.single().target,
        format!("/v2/acme/app/manifests/{manifest_digest}"),
        "`sha256:` with a literal colon, as the spec's URLs show it"
    );
}

#[tokio::test]
async fn manifest_answers_that_break_the_contract() {
    let oci = digest(MANIFEST_OCI);
    let other = digest(b"other bytes");
    let server = scripted([
        // Labelled with a type the call did not accept.
        empty(200)
            .header("content-type", DOCKER_V2)
            .body(MANIFEST_DOCKER),
        // A header digest (same algorithm) naming other bytes.
        empty(200)
            .header("content-type", OCI)
            .header("docker-content-digest", other.as_str())
            .body(MANIFEST_OCI),
        // Fetched by digest, other bytes.
        empty(200).header("content-type", OCI).body(MANIFEST_OCI),
        // The body's `mediaType` contradicts the Content-Type.
        empty(200).header("content-type", OCI).body(MANIFEST_DOCKER),
        // Not a manifest.
        empty(200)
            .header("content-type", OCI)
            .body(br#"{"schemaVersion":1}"#),
        error(404, MANIFEST_UNKNOWN),
        // A header with another algorithm is ignored, as the spec allows.
        empty(200)
            .header("content-type", OCI)
            .header(
                "docker-content-digest",
                "sha512:".to_owned() + &"0".repeat(128),
            )
            .body(MANIFEST_OCI),
    ])
    .await;
    let repository = repo(&registry(&server, None), "acme/app");
    let tag = Reference::tag("v1").unwrap();
    let oci_only = [ManifestMediaType::OciV1];

    let error = repository.manifest(&tag, &oci_only).await.unwrap_err();
    assert_eq!(
        error.protocol_problem(),
        Some(&ProtocolProblem::UnexpectedMediaType)
    );
    let error = repository.manifest(&tag, &oci_only).await.unwrap_err();
    assert_eq!(
        error.integrity_problem(),
        Some(&IntegrityProblem::HeaderDigestMismatch {
            expected: oci.clone(),
            header: other.clone()
        })
    );
    let error = repository
        .manifest(&Reference::digest(other.clone()), &oci_only)
        .await
        .unwrap_err();
    assert_eq!(
        error.integrity_problem(),
        Some(&IntegrityProblem::DigestMismatch {
            expected: other,
            actual: oci.clone()
        })
    );
    let error = repository.manifest(&tag, &oci_only).await.unwrap_err();
    assert_eq!(
        error.protocol_problem(),
        Some(&ProtocolProblem::MediaTypeMismatch)
    );
    let error = repository.manifest(&tag, &oci_only).await.unwrap_err();
    assert_eq!(
        error.protocol_problem(),
        Some(&ProtocolProblem::MalformedManifest)
    );
    let error = repository.manifest(&tag, &oci_only).await.unwrap_err();
    assert!(error.has_code(&ErrorCode::ManifestUnknown));
    assert_eq!(
        error.to_string(),
        "GET Repository.manifest: registry answered HTTP 404 Not Found (MANIFEST_UNKNOWN)"
    );
    let manifest = repository.manifest(&tag, &oci_only).await.unwrap();
    assert_eq!(manifest.digest, oci);
    assert_eq!(server.requests().len(), 7);
}

#[tokio::test]
async fn manifest_inputs_refused_before_sending() {
    let server = scripted([]).await;
    let repository = repo(&registry(&server, None), "acme/app");
    let error = repository
        .manifest(&Reference::tag("v1").unwrap(), &[])
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        RegistryError::Input(InputError::NoMediaType)
    ));
    let blake = Reference::digest(Digest::parse("blake3:00ff").unwrap());
    let error = repository
        .manifest(&blake, &[ManifestMediaType::OciV1])
        .await
        .unwrap_err();
    assert_eq!(
        error.integrity_problem(),
        Some(&IntegrityProblem::UnsupportedAlgorithm)
    );
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn a_manifest_push_sends_the_exact_bytes_with_their_media_type() {
    let upload = ManifestUpload::new(ManifestMediaType::OciV1, MANIFEST_OCI).unwrap();
    let pushed_digest = digest(MANIFEST_OCI);
    let server = scripted([
        empty(201)
            .header(
                "location",
                format!("/v2/acme/app/manifests/{pushed_digest}"),
            )
            .header("docker-content-digest", pushed_digest.as_str()),
        empty(201).header("docker-content-digest", pushed_digest.as_str()),
    ])
    .await;
    let repository = repo(&registry(&server, None), "acme/app");
    let pushed = repository
        .put_manifest(&Reference::tag("v2").unwrap(), &upload)
        .await
        .unwrap();
    assert_eq!(pushed.digest, pushed_digest);
    repository
        .put_manifest(&Reference::digest(pushed_digest.clone()), &upload)
        .await
        .unwrap();
    let requests = server.requests();
    assert_eq!(
        (requests[0].method.as_str(), requests[0].target.as_str()),
        ("PUT", "/v2/acme/app/manifests/v2")
    );
    assert_eq!(requests[0].header("content-type"), Some(OCI));
    assert_eq!(requests[0].body, MANIFEST_OCI);
    assert_eq!(
        requests[0].header("content-length"),
        Some(MANIFEST_OCI.len().to_string().as_str())
    );
    assert_eq!(
        requests[1].target,
        format!("/v2/acme/app/manifests/{pushed_digest}")
    );
}

#[tokio::test]
async fn manifest_push_refusals() {
    let upload = ManifestUpload::new(ManifestMediaType::OciV1, MANIFEST_OCI).unwrap();
    let server = scripted([
        error(400, MANIFEST_BLOBS_UNKNOWN),
        empty(201).header("docker-content-digest", digest(b"x").as_str()),
        empty(202),
    ])
    .await;
    let repository = repo(&registry(&server, None), "acme/app");
    let tag = Reference::tag("v2").unwrap();

    let error = repository.put_manifest(&tag, &upload).await.unwrap_err();
    let missing: Vec<Digest> = error
        .api()
        .unwrap()
        .errors()
        .iter()
        .filter_map(|e| e.detail_digest())
        .collect();
    assert_eq!(missing, [digest(CONFIG), digest(LAYER)]);
    assert!(error.has_code(&ErrorCode::BlobUnknown));

    let error = repository.put_manifest(&tag, &upload).await.unwrap_err();
    assert!(matches!(
        error.integrity_problem(),
        Some(IntegrityProblem::HeaderDigestMismatch { .. })
    ));
    let error = repository.put_manifest(&tag, &upload).await.unwrap_err();
    assert!(matches!(
        error.protocol_problem(),
        Some(ProtocolProblem::UnexpectedStatus(status)) if status.as_u16() == 202
    ));

    let wrong = Reference::digest(digest(b"not the manifest"));
    let error = repository.put_manifest(&wrong, &upload).await.unwrap_err();
    assert!(matches!(
        error,
        RegistryError::Input(InputError::ReferenceDigest)
    ));
    assert_eq!(server.requests().len(), 3);

    assert!(
        ManifestUpload::new(ManifestMediaType::DockerV2, MANIFEST_OCI).is_err(),
        "mediaType contradicts"
    );
    assert!(ManifestUpload::new(ManifestMediaType::OciV1, &b"{}"[..]).is_err());
    let rebuilt: ImageManifest = serde_json::from_slice(MANIFEST_OCI).unwrap();
    assert_eq!(
        rebuilt.to_upload().unwrap().media_type(),
        ManifestMediaType::OciV1
    );
}

// ---------------------------------------------------------------------------------------------
// Blobs.

#[tokio::test]
async fn blob_head_reports_length_and_digest() {
    let layer = digest(LAYER);
    let server = scripted([
        empty(200)
            .header("docker-content-digest", layer.as_str())
            .header("content-type", "application/octet-stream")
            .with_body(Body::HeadOnly(LAYER.len() as u64)),
        empty(404),
    ])
    .await;
    let repository = repo(&registry(&server, None), "acme/app");
    let info = repository.blob_info(&layer).await.unwrap().unwrap();
    assert_eq!(info.size, Some(LAYER.len() as u64));
    assert_eq!(info.digest, Some(layer.clone()));
    assert!(repository.blob_info(&layer).await.unwrap().is_none());
    let requests = server.requests();
    assert_eq!(requests[0].method, "HEAD");
    assert_eq!(requests[0].target, format!("/v2/acme/app/blobs/{layer}"));
}

#[tokio::test]
async fn a_blob_is_verified_against_its_digest_and_size() {
    let layer = digest(LAYER);
    let mut corrupted = LAYER.to_vec();
    corrupted[0] ^= 0x20;
    let mut longer = LAYER.to_vec();
    longer.push(b'!');
    let ok = || empty(200).header("content-type", "application/octet-stream");
    let server = scripted([
        ok().header("docker-content-digest", layer.as_str())
            .body(LAYER),
        ok().body(corrupted.clone()),
        // `Content-Length` contradicts the descriptor: refused before the body is read.
        ok().body(longer.clone()),
        // No length announced: the first byte too many fails the read.
        ok().with_body(Body::Chunks(vec![
            (
                std::time::Duration::ZERO,
                Bytes::copy_from_slice(&longer[..40]),
            ),
            (
                std::time::Duration::from_millis(20),
                Bytes::copy_from_slice(&longer[40..]),
            ),
        ])),
        ok().header("docker-content-digest", digest(b"x").as_str())
            .body(LAYER),
        error(404, BLOB_UNKNOWN),
    ])
    .await;
    let repository = repo(&registry(&server, None), "acme/app");
    let size = Some(LAYER.len() as u64);
    let limit = 1024;

    let bytes = repository
        .blob(&layer, size)
        .await
        .unwrap()
        .read_to_end(limit)
        .await
        .unwrap();
    assert_eq!(bytes.as_ref(), LAYER);

    let error = repository
        .blob(&layer, size)
        .await
        .unwrap()
        .read_to_end(limit)
        .await
        .unwrap_err();
    assert_eq!(
        error.integrity_problem(),
        Some(&IntegrityProblem::DigestMismatch {
            expected: layer.clone(),
            actual: digest(&corrupted)
        })
    );
    let error = repository.blob(&layer, size).await.unwrap_err();
    assert_eq!(
        error.integrity_problem(),
        Some(&IntegrityProblem::SizeMismatch {
            expected: 73,
            actual: 74
        })
    );
    let mut reader = repository.blob(&layer, size).await.unwrap();
    assert_eq!(
        reader.next_chunk().await.unwrap().unwrap().len(),
        40,
        "unverified bytes"
    );
    let error = reader.next_chunk().await.unwrap_err();
    assert_eq!(
        error.integrity_problem(),
        Some(&IntegrityProblem::SizeMismatch {
            expected: 73,
            actual: 74
        })
    );
    assert!(matches!(
        reader.next_chunk().await,
        Err(RegistryError::Input(InputError::ReaderEnded))
    ));

    let error = repository.blob(&layer, size).await.unwrap_err();
    assert!(matches!(
        error.integrity_problem(),
        Some(IntegrityProblem::HeaderDigestMismatch { .. })
    ));
    let error = repository.blob(&layer, size).await.unwrap_err();
    assert!(error.has_code(&ErrorCode::BlobUnknown));
    let requests = server.requests();
    assert!(
        requests
            .iter()
            .all(|r| r.method == "GET" && r.target == format!("/v2/acme/app/blobs/{layer}"))
    );
}

// ---------------------------------------------------------------------------------------------
// Uploads.

#[tokio::test]
async fn an_upload_follows_the_latest_location_with_inclusive_ranges() {
    let blob = b"0123456789";
    let blob_digest = digest(blob);
    let complete_digest = blob_digest.clone();
    let server = self_aware(move |url| {
        vec![
            empty(202)
                .header(
                    "location",
                    "/v2/acme/app/blobs/uploads/u1?_state=AAA%2Bb%3D%3D&x=1",
                )
                .header("range", "0-0")
                .header("docker-upload-uuid", "6d8a3b1e-not-in-any-url"),
            // An absolute Location on the registry's origin, new path and state.
            empty(202)
                .header(
                    "location",
                    format!("{url}/v2/acme/app/blobs/uploads/u1-r2?_state=BBB"),
                )
                .header("range", "0-4"),
            // The status query moves the session again.
            empty(204)
                .header("location", "/v2/acme/app/blobs/uploads/u1-r3?_state=CCC%2F")
                .header("range", "bytes=0-4"),
            empty(202)
                .header("location", "/v2/acme/app/blobs/uploads/u1-r4?_state=DDD")
                .header("range", "0-9"),
            empty(201)
                .header("location", format!("/v2/acme/app/blobs/{complete_digest}"))
                .header("docker-content-digest", complete_digest.as_str()),
        ]
    })
    .await;
    let repository = repo(&registry(&server, None), "acme/app");
    let mut upload = repository.start_upload().await.unwrap();
    assert_eq!(upload.uuid(), Some("6d8a3b1e-not-in-any-url"));
    assert_eq!(upload.offset(), 0);
    assert_eq!(
        upload
            .patch(Bytes::from_static(&blob[..5]))
            .await
            .unwrap()
            .offset,
        5
    );
    assert_eq!(upload.status().await.unwrap().offset, 5);
    assert_eq!(
        upload
            .patch(Bytes::from_static(&blob[5..]))
            .await
            .unwrap()
            .offset,
        10
    );
    let pushed = upload.complete(&blob_digest, None).await.unwrap();
    assert_eq!(
        (pushed.digest.clone(), pushed.size),
        (blob_digest.clone(), 10)
    );
    assert!(upload.is_finished());

    let requests = server.requests();
    let line = |i: usize| (requests[i].method.as_str(), requests[i].target.as_str());
    assert_eq!(line(0), ("POST", "/v2/acme/app/blobs/uploads/"));
    assert_eq!(requests[0].header("content-length"), Some("0"));
    assert!(requests[0].body.is_empty());
    assert_eq!(
        line(1),
        (
            "PATCH",
            "/v2/acme/app/blobs/uploads/u1?_state=AAA%2Bb%3D%3D&x=1"
        )
    );
    assert_eq!(requests[1].header("content-range"), Some("0-4"));
    assert_eq!(
        requests[1].header("content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(requests[1].header("content-length"), Some("5"));
    assert_eq!(requests[1].body, b"01234");
    assert_eq!(
        line(2),
        ("GET", "/v2/acme/app/blobs/uploads/u1-r2?_state=BBB")
    );
    assert_eq!(
        line(3),
        ("PATCH", "/v2/acme/app/blobs/uploads/u1-r3?_state=CCC%2F")
    );
    assert_eq!(requests[3].header("content-range"), Some("5-9"));
    assert_eq!(requests[3].body, b"56789");
    assert_eq!(
        line(4),
        (
            "PUT",
            format!(
                "/v2/acme/app/blobs/uploads/u1-r4?_state=DDD&digest=sha256%3A{}",
                blob_digest.hex()
            )
            .as_str()
        )
    );
    assert_eq!(requests[4].header("content-length"), Some("0"));
    assert!(requests[4].body.is_empty());
    assert!(
        requests.iter().all(|r| !r.target.contains("6d8a3b1e")),
        "never built from the UUID"
    );
    let shown = format!("{upload:?}");
    assert!(
        !shown.contains("_state") && !shown.contains("u1-r4"),
        "{shown}"
    );
}

#[tokio::test]
async fn a_location_that_leaves_the_registry_is_never_requested() {
    let rogue = scripted([]).await;
    let foreign = format!("{}/v2/acme/app/blobs/uploads/u1?_state=A", rogue.url());
    let server = scripted([
        empty(202)
            .header("location", foreign.clone())
            .header("range", "0-0"),
        empty(202).header("range", "0-0"),
        empty(202)
            .header("location", "/v2/acme/app/blobs/uploads/u2")
            .header("range", "0-0"),
        empty(202)
            .header("location", foreign)
            .header("range", "0-4"),
    ])
    .await;
    let repository = repo(&registry(&server, None), "acme/app");
    let error = repository.start_upload().await.unwrap_err();
    assert_eq!(
        error.protocol_problem(),
        Some(&ProtocolProblem::ForeignLocation)
    );
    let error = repository.start_upload().await.unwrap_err();
    assert_eq!(
        error.protocol_problem(),
        Some(&ProtocolProblem::MissingLocation)
    );

    let mut upload = repository.start_upload().await.unwrap();
    let error = upload
        .patch(Bytes::from_static(b"01234"))
        .await
        .unwrap_err();
    assert_eq!(
        error.protocol_problem(),
        Some(&ProtocolProblem::ForeignLocation)
    );
    assert!(upload.needs_status(), "the chunk's fate is uncertain");
    assert_eq!(upload.location().path(), "/v2/acme/app/blobs/uploads/u2");
    assert!(rogue.requests().is_empty());
    assert_eq!(server.requests().len(), 4);
}

#[tokio::test]
async fn an_interrupted_chunk_requires_a_status_query_then_resumes_from_the_reported_range() {
    let server = scripted([
        empty(202)
            .header("location", "/v2/acme/app/blobs/uploads/u1?_state=A")
            .header("range", "0-0"),
        Reply::hang_up(),
        empty(204)
            .header("location", "/v2/acme/app/blobs/uploads/u1?_state=B")
            .header("range", "0-2"),
        empty(202)
            .header("location", "/v2/acme/app/blobs/uploads/u1?_state=C")
            .header("range", "0-5"),
    ])
    .await;
    let repository = repo(&registry(&server, None), "acme/app");
    let mut upload = repository.start_upload().await.unwrap();
    let error = upload
        .patch(Bytes::from_static(b"012345"))
        .await
        .unwrap_err();
    assert_eq!(
        error.request_kind(),
        Some(concord_sdk_distribution_registry::concord::ErrorKind::Transport)
    );
    assert!(upload.needs_status());
    let refused = upload
        .patch(Bytes::from_static(b"012345"))
        .await
        .unwrap_err();
    assert!(matches!(
        refused,
        RegistryError::Input(InputError::StatusRequired)
    ));
    let refused = upload.complete(&digest(b"012345"), None).await.unwrap_err();
    assert!(matches!(
        refused,
        RegistryError::Input(InputError::StatusRequired)
    ));
    assert_eq!(server.requests().len(), 2, "refusals send nothing");

    // The registry kept three bytes: the caller re-slices its source at the reported offset.
    assert_eq!(upload.status().await.unwrap().offset, 3);
    upload.patch(Bytes::from_static(b"345")).await.unwrap();
    assert_eq!(upload.offset(), 6);
    let requests = server.requests();
    assert_eq!(requests[2].target, "/v2/acme/app/blobs/uploads/u1?_state=A");
    assert_eq!(requests[3].target, "/v2/acme/app/blobs/uploads/u1?_state=B");
    assert_eq!(requests[3].header("content-range"), Some("3-5"));
}

#[tokio::test]
async fn a_416_moves_the_session_to_the_last_valid_range() {
    let server = scripted([
        empty(202)
            .header("location", "/v2/acme/app/blobs/uploads/u1?_state=A")
            .header("range", "0-0"),
        empty(202)
            .header("location", "/v2/acme/app/blobs/uploads/u1?_state=B")
            .header("range", "0-3"),
        error(416, RANGE_INVALID)
            .header("location", "/v2/acme/app/blobs/uploads/u1?_state=C")
            .header("range", "0-1"),
        empty(202)
            .header("location", "/v2/acme/app/blobs/uploads/u1?_state=D")
            .header("range", "0-5"),
    ])
    .await;
    let repository = repo(&registry(&server, None), "acme/app");
    let mut upload = repository.start_upload().await.unwrap();
    upload.patch(Bytes::from_static(b"0123")).await.unwrap();
    let error = upload.patch(Bytes::from_static(b"45")).await.unwrap_err();
    assert_eq!(error.status().map(|s| s.as_u16()), Some(416));
    assert!(error.has_code(&ErrorCode::RangeInvalid));
    assert!(!upload.needs_status());
    assert_eq!(upload.offset(), 2, "the registry's last valid range");
    upload.patch(Bytes::from_static(b"2345")).await.unwrap();
    let requests = server.requests();
    assert_eq!(requests[3].target, "/v2/acme/app/blobs/uploads/u1?_state=C");
    assert_eq!(requests[3].header("content-range"), Some("2-5"));
}

#[tokio::test]
async fn upload_answers_that_break_the_contract() {
    let server = scripted([
        empty(202)
            .header("location", "/v2/acme/app/blobs/uploads/u1")
            .header("range", "0-0"),
        // Acknowledges less than the chunk.
        empty(202)
            .header("location", "/v2/acme/app/blobs/uploads/u1")
            .header("range", "0-2"),
        // A status claiming bytes never sent.
        empty(204).header("range", "0-99"),
        empty(204).header("range", "0-4"),
        error(400, DIGEST_INVALID),
        empty(204).header("range", "0-4"),
        empty(201).header("docker-content-digest", digest(b"other").as_str()),
        empty(202)
            .header("location", "/v2/acme/app/blobs/uploads/u2")
            .header("range", "0-7"),
    ])
    .await;
    let repository = repo(&registry(&server, None), "acme/app");
    let mut upload = repository.start_upload().await.unwrap();
    let error = upload
        .patch(Bytes::from_static(b"01234"))
        .await
        .unwrap_err();
    assert_eq!(
        error.protocol_problem(),
        Some(&ProtocolProblem::UnexpectedRange {
            expected: Some(4),
            reported: 3
        })
    );
    assert!(upload.needs_status());
    let error = upload.status().await.unwrap_err();
    assert!(matches!(
        error.protocol_problem(),
        Some(ProtocolProblem::UnexpectedRange { reported: 100, .. })
    ));
    assert_eq!(upload.status().await.unwrap().offset, 5);

    let error = upload.complete(&digest(b"wrong"), None).await.unwrap_err();
    assert!(error.has_code(&ErrorCode::DigestInvalid));
    assert!(upload.needs_status());
    upload.status().await.unwrap();
    let blob = digest(b"01234");
    let error = upload.complete(&blob, None).await.unwrap_err();
    assert!(matches!(
        error.integrity_problem(),
        Some(IntegrityProblem::HeaderDigestMismatch { .. })
    ));

    let error = repository.start_upload().await.unwrap_err();
    assert!(matches!(
        error.protocol_problem(),
        Some(ProtocolProblem::UnexpectedRange {
            expected: None,
            reported: 8
        })
    ));
}

#[tokio::test]
async fn chunk_inputs_refused_before_sending() {
    let server = scripted([empty(202)
        .header("location", "/v2/acme/app/blobs/uploads/u1")
        .header("range", "0-0")])
    .await;
    let repository = repo(&registry(&server, None), "acme/app");
    let mut upload = repository.start_upload().await.unwrap();
    let empty_chunk = upload.patch(Bytes::new()).await.unwrap_err();
    assert!(matches!(
        empty_chunk,
        RegistryError::Input(InputError::EmptyChunk)
    ));
    let one = upload.patch(Bytes::from_static(b"x")).await.unwrap_err();
    assert!(matches!(
        one,
        RegistryError::Input(InputError::OneByteFirstChunk)
    ));
    let error = repository
        .push_blob(Bytes::from_static(b"abc"), NonZeroUsize::new(1).unwrap())
        .await
        .unwrap_err();
    assert!(matches!(error, RegistryError::Input(InputError::ChunkSize)));
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn a_one_byte_blob_goes_with_the_completing_put() {
    let fake = Fake::start().await;
    let repository = repo(&fake.client(), "acme/app");
    let pushed = repository
        .push_blob(Bytes::from_static(b"x"), NonZeroUsize::new(4).unwrap())
        .await
        .unwrap();
    assert_eq!(pushed.digest, digest(b"x"));
    let methods: Vec<String> = fake
        .registry
        .requests()
        .into_iter()
        .filter(|r| r.header("authorization").is_some())
        .map(|r| r.method)
        .collect();
    assert_eq!(methods, ["POST", "PUT"]);
}

// ---------------------------------------------------------------------------------------------
// Error documents and redaction.

#[tokio::test]
async fn error_bodies_unknown_codes_html_and_oversize() {
    let large = vec![b' '; concord_sdk_distribution_registry::ERROR_BODY_LIMIT + 1];
    let server = scripted([
        error(429, UNKNOWN_CODE),
        Reply::text(502, "text/html", PROXY_ERROR),
        json(500, &large),
    ])
    .await;
    let registry = registry(&server, None);
    let error = registry.ping().await.unwrap_err();
    let codes: Vec<ErrorCode> = error
        .api()
        .unwrap()
        .errors()
        .iter()
        .map(|e| e.code.clone())
        .collect();
    assert_eq!(
        codes,
        [
            ErrorCode::TooManyRequests,
            ErrorCode::Unknown("VENDOR_SPECIFIC".into())
        ]
    );
    assert_eq!(codes[1].label(), "UNKNOWN");
    assert_eq!(
        error.to_string(),
        "GET Registry.ping: registry answered HTTP 429 Too Many Requests (TOOMANYREQUESTS, UNKNOWN)"
    );
    let error = registry.ping().await.unwrap_err();
    assert!(
        matches!(&error.api().unwrap().body, ErrorBody::Unrecognized(bytes) if bytes.as_ref() == PROXY_ERROR)
    );
    let error = registry.ping().await.unwrap_err();
    assert!(matches!(
        error.api().unwrap().body,
        ErrorBody::TooLarge { .. }
    ));
}

#[tokio::test]
async fn tokens_credentials_and_upload_urls_stay_out_of_debug_and_display() {
    let fake = Fake::start().await;
    fake.with(|s| s.require_basic = true);
    let credentials = BasicCredentials::new(USERNAME, PASSWORD).unwrap();
    let registry = Registry::new(http(), &fake.registry.url())
        .unwrap()
        .token_auth(
            TokenAuth::basic(credentials)
                .trust_realm(&fake.auth.url())
                .unwrap(),
        );
    let repository = repo(&registry, "acme/app");
    let mut upload = repository.start_upload().await.unwrap();
    upload.patch(Bytes::from_static(b"0123")).await.unwrap();
    let error = upload.complete(&digest(b"not it"), None).await.unwrap_err();

    let tokens: Vec<String> = fake.with(|s| s.tokens.keys().cloned().collect());
    assert_eq!(tokens.len(), 1);
    let mut secrets: Vec<&str> = vec![PASSWORD, BASIC, "_state"];
    secrets.push(&tokens[0]);
    assert_hides(&error, &secrets);
    for shown in [
        format!("{registry:?}"),
        format!("{repository:?}"),
        format!("{upload:?}"),
    ] {
        for secret in &secrets {
            assert!(!shown.contains(secret), "{secret} in {shown}");
        }
    }
    assert_eq!(
        fake.with(|s| s.token_requests[0].2.clone()).as_deref(),
        Some(BASIC),
        "the token service got the Basic credentials"
    );
}
