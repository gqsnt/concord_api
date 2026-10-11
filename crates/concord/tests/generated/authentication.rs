//! Generated authentication surface through downstream-declared clients (§11):
//! protected builder inputs, credential sources versus placements, the
//! synchronous local `credentials().clear_x()/replace_x()` controls,
//! explicit `store_as_*` installation ordering with its two counters,
//! source-aware challenge behavior, provider single flight and cooldown, and
//! value-free failure and observation truth (V3-E11..E15, V3-S10 for
//! credential material).
//!
//! Credentials are only ever sent over HTTPS, so every execution test here
//! reaches the loopback `TlsServer` through an explicit CONNECT tunnel.

// Without `default-tls` only the build-time and local-control tests run; the
// TLS harness helpers below are then intentionally unused.
#![cfg_attr(not(feature = "default-tls"), allow(unused_imports, dead_code))]

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use concord::api;
use concord::auth::{
    AccessToken, ApiKey, AuthError, AuthErrorKind, BasicCredential, BoxFuture, CredentialMaterial,
    CredentialMaterialErrorKind, CredentialProvider, CredentialReason, CredentialRequest,
    CredentialVersion, MAX_BASIC_PASSWORD_BYTES, MAX_BASIC_USERNAME_BYTES,
    MAX_CREDENTIAL_SECRET_BYTES, RecoveryMode, SecretCredential, StoreRequest,
};
use concord::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use concord::error::{
    ClientBuildErrorKind, ClientInputKind, DispatchKnowledge, ErrorCategory, FailureStage,
    ProviderAcquisitionFacts, ProviderAcquisitionOutcome,
};
use concord::http::StatusCode;
#[cfg(feature = "default-tls")]
use concord::http::{HttpConfigBuilder, SafeProxy};
use concord::observation::{CompletionOutcome, OperationEvent, OperationObserver};
use concord::rate_limit::{
    Enforcement, LimiterCapability, RateLimitError, RateLimitPermit, RateLimitRequest, RateLimiter,
};
use concord::runtime::OperationId;
use concord::{ApiError, Binary, Bytes, ClientBuildError, SecretString, StoreOutcome};
#[cfg(feature = "default-tls")]
use concord_test_support::proxy::ConnectProxy;
#[cfg(feature = "default-tls")]
use concord_test_support::tls::TlsServer;

// ---------------------------------------------------------------------------
// Owned fake material, codecs and providers (no real credentials).
// ---------------------------------------------------------------------------

/// Custom, deliberately non-`Clone` credential material with an optional
/// expiry.
pub struct SessionToken {
    secret: SecretString,
    expires_at: Option<Instant>,
}

impl SessionToken {
    fn new(value: &str) -> Self {
        Self {
            secret: secret(value),
            expires_at: None,
        }
    }

    fn expired(value: &str) -> Self {
        Self {
            secret: secret(value),
            expires_at: Some(
                Instant::now()
                    .checked_sub(Duration::from_secs(1))
                    .expect("representable past instant"),
            ),
        }
    }
}

fn secret(value: impl AsRef<str>) -> SecretString {
    SecretString::try_new(value).expect("bounded test protected string")
}

impl CredentialMaterial for SessionToken {
    fn expires_at(&self) -> Option<Instant> {
        self.expires_at
    }
}

impl SecretCredential for SessionToken {
    fn expose_secret(&self) -> &str {
        self.secret.expose_secret()
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SessionToken(<redacted>)")
    }
}

/// Buffered source-endpoint codec producing complete credential material.
pub struct SessionCodec;

impl DecodeBody for SessionCodec {
    type Output = SessionToken;

    const ACCEPT: Option<&'static str> = Some("application/session");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let value = String::from_utf8(body.to_vec())
            .map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        SecretString::try_new(value)
            .map(|secret| SessionToken {
                secret,
                expires_at: None,
            })
            .map_err(|_| CodecError::new(CodecErrorKind::Decode))
    }
}

/// Provider producing non-`Clone` custom material, counting acquisitions and
/// (optionally) its own drop.
pub struct SessionProvider {
    acquires: Arc<AtomicUsize>,
    drops: Option<Arc<AtomicUsize>>,
}

impl SessionProvider {
    fn new() -> Self {
        Self {
            acquires: Arc::new(AtomicUsize::new(0)),
            drops: None,
        }
    }

    fn counting(acquires: &Arc<AtomicUsize>) -> Self {
        Self {
            acquires: Arc::clone(acquires),
            drops: None,
        }
    }

    fn dropping(drops: &Arc<AtomicUsize>) -> Self {
        Self {
            acquires: Arc::new(AtomicUsize::new(0)),
            drops: Some(Arc::clone(drops)),
        }
    }
}

impl Drop for SessionProvider {
    fn drop(&mut self) {
        if let Some(drops) = &self.drops {
            drops.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl CredentialProvider for SessionProvider {
    type Material = SessionToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.acquires.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(SessionToken::new("provider-token")) })
    }
}

/// Refresh-capable bearer provider: the first acquisition yields the initial
/// token, every later acquisition or refresh yields the refreshed token.
pub struct BearerProvider {
    acquires: Arc<AtomicUsize>,
    refreshes: Arc<AtomicUsize>,
}

impl BearerProvider {
    fn new() -> Self {
        Self {
            acquires: Arc::new(AtomicUsize::new(0)),
            refreshes: Arc::new(AtomicUsize::new(0)),
        }
    }
}

const INITIAL_BEARER: &str = "provider-bearer-token";
const REFRESHED_BEARER: &str = "refreshed-provider-bearer-token";

impl CredentialProvider for BearerProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        let first = self.acquires.fetch_add(1, Ordering::SeqCst) == 0;
        Box::pin(async move {
            let token = if first {
                INITIAL_BEARER
            } else {
                REFRESHED_BEARER
            };
            Ok(AccessToken::try_new(token).expect("bounded provider access token"))
        })
    }

    fn refresh<'a>(
        &'a self,
        current: &'a Self::Material,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        assert_eq!(context.reason, CredentialReason::Rejected);
        assert_eq!(current.expose_secret(), INITIAL_BEARER);
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(AccessToken::try_new(REFRESHED_BEARER).expect("bounded token")) })
    }

    fn recovery_mode(&self) -> RecoveryMode {
        RecoveryMode::Refresh
    }
}

// ---------------------------------------------------------------------------
// Declarations.
// ---------------------------------------------------------------------------

api! {
    client AuthApi {
        timeout 30s;
        base "https://localhost/";
        values {
            query tenant: String;
        }
        credentials {
            secret api_secret;
            secret bearer_secret;
            secret username;
            secret password;
            credential application_key = api_key(secret.api_secret);
            credential static_bearer = bearer(secret.bearer_secret);
            credential login = basic(secret.username, secret.password);
            credential rotating_bearer = custom BearerProvider;
            credential acquired = endpoint login;
            credential session = custom SessionProvider;
        }

        /// Explicit source endpoint of the `acquired` credential.
        GET login() -> SessionToken via SessionCodec {
            id "auth.login";
            path "login";
        }
        GET api_key_endpoint() -> Bytes via Binary {
            path "api-key";
            auth { header "X-Api-Key" = application_key; }
        }
        GET static_challenged() -> Bytes via Binary {
            path "static-challenged";
            auth { header "X-Api-Key" = application_key challenge unauthorized; }
        }
        GET bearer_endpoint() -> Bytes via Binary {
            path "bearer";
            auth { bearer static_bearer; }
        }
        GET basic_endpoint() -> Bytes via Binary {
            path "basic";
            auth { basic login; }
        }
        GET provider_bearer_endpoint() -> Bytes via Binary {
            path "provider-bearer";
            auth { bearer rotating_bearer challenge unauthorized; }
        }
        POST provider_write() -> Bytes via Binary {
            path "provider-write";
            auth { bearer rotating_bearer challenge unauthorized; }
        }
        GET acquired_challenged() -> Bytes via Binary {
            path "acquired-challenged";
            auth { bearer acquired challenge unauthorized; }
        }
        scope nested() {
            path "nested";
            auth { header "X-Session" = session; }
            GET inherited() -> Bytes via Binary { path "inherited"; }
            GET replaced() -> Bytes via Binary {
                path "replaced";
                auth replace { query "token" = acquired; }
            }
            GET public() -> Bytes via Binary {
                path "public";
                auth off;
            }
        }
    }
}

api! {
    client SharedSecretApi {
        timeout 30s;
        base "https://localhost/";
        credentials {
            secret shared;
            credential key = api_key(secret.shared);
            credential token = bearer(secret.shared);
        }

        GET keyed() -> Bytes via Binary {
            path "keyed";
            auth { header "X-Api-Key" = key; }
        }
        GET bearer() -> Bytes via Binary {
            path "bearer";
            auth { bearer token; }
        }
    }
}

api! {
    client CredentialProofApi {
        timeout 30s;
        base "https://localhost/";
        credentials {
            secret api_secret;
            credential application_key = api_key(secret.api_secret);
            credential session = custom SessionProvider;
        }

        GET api_key_endpoint() -> Bytes via Binary {
            path "api-key";
            auth { header "X-Api-Key" = application_key; }
        }
        GET session_endpoint() -> Bytes via Binary {
            path "session";
            auth { header "X-Session" = session; }
        }
        GET exact_authorization() -> Bytes via Binary {
            path "exact";
            auth { authorization = application_key; }
        }
        GET scheme_authorization() -> Bytes via Binary {
            path "scheme";
            auth { authorization "tOkEn" = session; }
        }
    }
}

api! {
    /// Endpoint-backed session whose source endpoint is admitted through an
    /// application limiter, which lets tests hold a started storage operation
    /// between its intent reservation and its dispatch.
    client SessionApi {
        timeout 30s;
        base "https://localhost/";
        credentials {
            credential session = endpoint login;
        }
        rate_limits {
            windows open { allow 1000 per 1s; }
        }

        GET login() -> SessionToken via SessionCodec {
            id "session.login";
            path "login";
            rate_limit "session.login" key [origin] using open;
        }
        GET protected() -> Bytes via Binary {
            id "session.protected";
            path "protected";
            auth { header "X-Session" = session challenge unauthorized; }
        }
    }
}

api! {
    /// A provider-backed placement used for single-flight and cooldown.
    client FlightApi {
        timeout 30s;
        base "https://localhost/";
        credentials {
            credential session = custom FlightProvider;
        }

        GET protected() -> Bytes via Binary {
            id "flight.protected";
            path "protected";
            auth { header "X-Session" = session; }
        }
    }
}

// ---------------------------------------------------------------------------
// Loopback helpers.
// ---------------------------------------------------------------------------

#[cfg(feature = "default-tls")]
struct Tls {
    server: TlsServer,
    tunnel: ConnectProxy,
}

#[cfg(feature = "default-tls")]
impl Tls {
    fn start() -> Self {
        let server = TlsServer::start().expect("loopback TLS server");
        let tunnel = ConnectProxy::start(server.address()).expect("loopback CONNECT tunnel");
        Self { server, tunnel }
    }

    fn http(&self) -> impl FnOnce(HttpConfigBuilder) -> HttpConfigBuilder + 'static {
        let proxy = SafeProxy::parse(self.tunnel.url()).expect("credential-free proxy origin");
        let certificate = self.server.certificate_pem();
        move |http| {
            http.http1_only()
                .proxy(proxy)
                .add_root_certificate_pem(certificate)
        }
    }

    fn queue(&self, status: u16, body: &str) {
        self.server
            .queue_response(status, body.as_bytes().to_vec())
            .expect("queued TLS response");
    }

    fn queue_session(&self, token: &str) {
        self.server
            .queue_response_with_headers(
                200,
                &[("content-type", "application/session")],
                token.as_bytes().to_vec(),
            )
            .expect("queued session response");
    }

    fn request_count(&self) -> usize {
        self.server.requests().len()
    }
}

fn assert_omits(rendered: &str, sentinels: &[&str]) {
    for sentinel in sentinels {
        assert!(
            !rendered.contains(sentinel),
            "automatic output exposed `{sentinel}`: {rendered}"
        );
    }
}

fn render_error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut rendered = format!("{error}\n{error:?}");
    let mut source = error.source();
    while let Some(current) = source {
        rendered.push_str(&format!("\n{current}\n{current:?}"));
        source = current.source();
    }
    rendered
}

fn auth_builder() -> auth_api::AuthApiBuilder {
    AuthApi::builder()
        .set_tenant("tenant")
        .api_secret(secret("api-secret"))
        .bearer_secret(secret("bearer-secret"))
        .username(secret("user"))
        .password(secret("password"))
        .rotating_bearer(BearerProvider::new())
        .session(SessionProvider::new())
}

fn credential_proof_builder() -> credential_proof_api::CredentialProofApiBuilder {
    CredentialProofApi::builder()
        .api_secret(secret("proof-api-key"))
        .session(SessionProvider::new())
}

// ---------------------------------------------------------------------------
// Builder surface and protected inputs.
// ---------------------------------------------------------------------------

/// Exercises every generated local control with its exact material type:
/// `replace_x(material) -> Result<CredentialVersion, AuthError>` and
/// `clear_x() -> Result<(), AuthError>` (§11.3). No handle, reference or
/// source-selection API exists on the facade.
fn exercise_local_controls(
    credentials: &auth_api::Credentials<'_>,
) -> Result<Vec<CredentialVersion>, AuthError> {
    let versions = vec![
        credentials.replace_application_key(ApiKey::try_new("k").map_err(AuthError::from)?)?,
        credentials.replace_static_bearer(AccessToken::try_new("b").map_err(AuthError::from)?)?,
        credentials.replace_login(BasicCredential::try_new("u", "p").map_err(AuthError::from)?)?,
        credentials.replace_rotating_bearer(AccessToken::try_new("r").map_err(AuthError::from)?)?,
        credentials.replace_acquired(SessionToken::new("a"))?,
        credentials.replace_session(SessionToken::new("s"))?,
    ];
    credentials.clear_application_key()?;
    credentials.clear_static_bearer()?;
    credentials.clear_login()?;
    credentials.clear_rotating_bearer()?;
    credentials.clear_acquired()?;
    credentials.clear_session()?;
    Ok(versions)
}

#[cfg(feature = "default-tls")] // https base: TlsUnavailable without TLS
#[test]
fn generated_authentication_surface_has_source_specific_local_controls() {
    let client = auth_builder().build().expect("generated auth client");
    let credentials = client.credentials();
    let versions = exercise_local_controls(&credentials).expect("synchronous local controls");
    assert_eq!(versions.len(), 6);
    // Clearing an already empty slot still succeeds (it advances the intent).
    credentials
        .clear_acquired()
        .expect("clear of an empty slot");
    assert_eq!(format!("{credentials:?}"), "Credentials { .. }");

    // The storage terminal exists only on the designated source endpoint and
    // is inert until polled.
    let pending: StoreRequest = client.login().store_as_acquired();
    drop(pending);
    let _: auth_api::nested::InheritedRequest = client.nested().inherited();
    let _: auth_api::nested::ReplacedRequest = client.nested().replaced();
    let _: auth_api::nested::PublicRequest = client.nested().public();
}

#[test]
fn every_required_client_input_accumulates_with_safe_builder_metadata() {
    let error = AuthApi::builder()
        .build()
        .expect_err("every required input is absent");
    assert_eq!(error.kind(), ClientBuildErrorKind::MissingInput);
    assert_eq!(
        error.missing_fields(),
        [
            ("set_tenant", ClientInputKind::Value),
            ("api_secret", ClientInputKind::Secret),
            ("bearer_secret", ClientInputKind::Secret),
            ("username", ClientInputKind::Secret),
            ("password", ClientInputKind::Secret),
            ("rotating_bearer", ClientInputKind::CredentialProvider),
            ("session", ClientInputKind::CredentialProvider),
        ]
    );
    assert_eq!(error.input_field(), Some("set_tenant"));
    assert_eq!(error.input_kind(), Some(ClientInputKind::Value));

    fn first_missing(
        result: Result<AuthApi, ClientBuildError>,
        field: &'static str,
        kind: ClientInputKind,
        sentinel: &str,
    ) {
        let error = result.expect_err("a required input is still absent");
        assert_eq!(error.kind(), ClientBuildErrorKind::MissingInput);
        assert_eq!(error.input_field(), Some(field));
        assert_eq!(error.input_kind(), Some(kind));
        assert_eq!(error.missing_fields().first(), Some(&(field, kind)));
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert_omits(&rendered, &[sentinel]);
        }
    }

    first_missing(
        AuthApi::builder()
            .set_tenant("tenant-value-sentinel")
            .build(),
        "api_secret",
        ClientInputKind::Secret,
        "tenant-value-sentinel",
    );
    first_missing(
        AuthApi::builder()
            .set_tenant("tenant")
            .api_secret(secret("first-secret-sentinel-9aa1"))
            .build(),
        "bearer_secret",
        ClientInputKind::Secret,
        "first-secret-sentinel-9aa1",
    );
    first_missing(
        AuthApi::builder()
            .set_tenant("tenant")
            .api_secret(secret("api"))
            .bearer_secret(secret("bearer-secret-sentinel-5f01"))
            .build(),
        "username",
        ClientInputKind::Secret,
        "bearer-secret-sentinel-5f01",
    );
    first_missing(
        AuthApi::builder()
            .set_tenant("tenant")
            .api_secret(secret("api"))
            .bearer_secret(secret("bearer"))
            .username(secret("user-sentinel"))
            .build(),
        "password",
        ClientInputKind::Secret,
        "user-sentinel",
    );
    first_missing(
        AuthApi::builder()
            .set_tenant("tenant")
            .api_secret(secret("api"))
            .bearer_secret(secret("bearer"))
            .username(secret("user"))
            .password(secret("password-sentinel"))
            .build(),
        "rotating_bearer",
        ClientInputKind::CredentialProvider,
        "password-sentinel",
    );
    let error = AuthApi::builder()
        .set_tenant("tenant")
        .api_secret(secret("api"))
        .bearer_secret(secret("bearer"))
        .username(secret("user"))
        .password(secret("last-secret-sentinel-4b71"))
        .rotating_bearer(BearerProvider::new())
        .build()
        .expect_err("session provider is absent");
    assert_eq!(error.kind(), ClientBuildErrorKind::MissingInput);
    assert_eq!(
        error.missing_fields(),
        [("session", ClientInputKind::CredentialProvider)]
    );
    for rendered in [error.to_string(), format!("{error:?}")] {
        assert_omits(&rendered, &["last-secret-sentinel-4b71"]);
    }
}

fn assert_invalid_credential(
    result: Result<AuthApi, ClientBuildError>,
    field: &'static str,
    sentinel: &str,
) {
    let error = result.expect_err("credential input must be rejected");
    assert_eq!(error.kind(), ClientBuildErrorKind::InvalidCredential);
    assert_eq!(error.input_field(), Some(field));
    assert_eq!(error.input_kind(), Some(ClientInputKind::Secret));
    for rendered in [error.to_string(), format!("{error:?}")] {
        assert_omits(&rendered, &[sentinel]);
    }
}

#[cfg(feature = "default-tls")] // https base: TlsUnavailable without TLS
#[test]
fn static_built_in_credentials_validate_exact_limits_during_client_build() {
    auth_builder()
        .api_secret(secret("a".repeat(MAX_CREDENTIAL_SECRET_BYTES)))
        .build()
        .expect("exact API-key material limit");
    auth_builder()
        .bearer_secret(secret("b".repeat(MAX_CREDENTIAL_SECRET_BYTES)))
        .build()
        .expect("exact access-token material limit");
    auth_builder()
        .username(secret("u".repeat(MAX_BASIC_USERNAME_BYTES)))
        .password(secret("p".repeat(MAX_BASIC_PASSWORD_BYTES)))
        .build()
        .expect("exact Basic component limits");
}

#[cfg(feature = "default-tls")] // https base: TlsUnavailable without TLS
#[test]
fn static_built_in_credential_first_excess_fails_build_with_safe_field_metadata() {
    const FIRST_EXCESS_SENTINEL: &str = "first-excess-sentinel-";

    assert_invalid_credential(
        auth_builder()
            .api_secret(secret(format!(
                "{FIRST_EXCESS_SENTINEL}{}",
                "a".repeat(MAX_CREDENTIAL_SECRET_BYTES)
            )))
            .build(),
        "api_secret",
        FIRST_EXCESS_SENTINEL,
    );
    assert_invalid_credential(
        auth_builder()
            .bearer_secret(secret(format!(
                "{FIRST_EXCESS_SENTINEL}{}",
                "b".repeat(MAX_CREDENTIAL_SECRET_BYTES)
            )))
            .build(),
        "bearer_secret",
        FIRST_EXCESS_SENTINEL,
    );
    assert_invalid_credential(
        auth_builder()
            .username(secret(format!(
                "{FIRST_EXCESS_SENTINEL}{}",
                "u".repeat(MAX_BASIC_USERNAME_BYTES)
            )))
            .build(),
        "username",
        FIRST_EXCESS_SENTINEL,
    );
    assert_invalid_credential(
        auth_builder()
            .password(secret(format!(
                "{FIRST_EXCESS_SENTINEL}{}",
                "p".repeat(MAX_BASIC_PASSWORD_BYTES)
            )))
            .build(),
        "password",
        FIRST_EXCESS_SENTINEL,
    );
}

#[cfg(feature = "default-tls")] // https base: TlsUnavailable without TLS
#[test]
fn static_basic_username_colons_are_rejected_and_password_colons_remain_valid() {
    const USERNAME_SENTINEL: &str = "basic-user:delimiter-sentinel";
    assert_invalid_credential(
        auth_builder().username(secret(USERNAME_SENTINEL)).build(),
        "username",
        USERNAME_SENTINEL,
    );
    auth_builder()
        .password(secret("prefix:password:tail"))
        .build()
        .expect("Basic password colons remain valid");
}

#[cfg(feature = "default-tls")] // https base: TlsUnavailable without TLS
#[test]
fn repeated_provider_setters_replace_in_call_order() {
    let replaced_drops = Arc::new(AtomicUsize::new(0));
    let selected_drops = Arc::new(AtomicUsize::new(0));
    let builder = AuthApi::builder()
        .set_tenant("tenant")
        .api_secret(secret("api-secret"))
        .bearer_secret(secret("bearer-secret"))
        .username(secret("user"))
        .password(secret("password"))
        .rotating_bearer(BearerProvider::new())
        .session(SessionProvider::dropping(&replaced_drops))
        .session(SessionProvider::dropping(&selected_drops));
    assert_eq!(replaced_drops.load(Ordering::SeqCst), 1);
    assert_eq!(selected_drops.load(Ordering::SeqCst), 0);

    let client = builder.build().expect("provider replacement client");
    assert_eq!(selected_drops.load(Ordering::SeqCst), 0);
    drop(client);
    assert_eq!(selected_drops.load(Ordering::SeqCst), 1);
}

// ---------------------------------------------------------------------------
// Placements reach only the TLS boundary.
// ---------------------------------------------------------------------------

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn one_declared_secret_feeds_multiple_credentials_through_shared_clones() {
    let tls = Tls::start();
    let client = SharedSecretApi::builder()
        .shared(secret("one-shared-protected-value"))
        .configure_http(tls.http())
        .build()
        .expect("one bounded secret can feed multiple built-in sources");
    client.keyed().await.expect("API-key placement");
    client.bearer().await.expect("bearer placement");
    let requests = tls.server.requests();
    assert_eq!(
        requests[0].header("x-api-key"),
        Some("one-shared-protected-value")
    );
    assert_eq!(
        requests[1].header("authorization"),
        Some("Bearer one-shared-protected-value")
    );
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn non_clone_custom_material_compiles_and_executes_through_generated_storage() {
    let tls = Tls::start();
    tls.queue(200, "non-clone");
    let acquires = Arc::new(AtomicUsize::new(0));
    let client = CredentialProofApi::builder()
        .api_secret(secret("proof-api-key"))
        .session(SessionProvider::counting(&acquires))
        .configure_http(tls.http())
        .build()
        .expect("non-Clone material client");
    assert_eq!(
        acquires.load(Ordering::SeqCst),
        0,
        "building performs no provider acquisition"
    );

    client
        .session_endpoint()
        .await
        .expect("non-Clone provider material request");
    client
        .session_endpoint()
        .await
        .expect("installed provider material is reused");

    let requests = tls.server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].header("x-session"), Some("provider-token"));
    assert_eq!(requests[1].header("x-session"), Some("provider-token"));
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "default-tls")] // https base: TlsUnavailable without TLS
#[test]
fn custom_authorization_is_absent_from_generated_safe_plans() {
    const EXACT_SENTINEL: &str = "exact-plan-authorization-sentinel-8f73";
    let acquires = Arc::new(AtomicUsize::new(0));
    let client = CredentialProofApi::builder()
        .api_secret(secret(EXACT_SENTINEL))
        .session(SessionProvider::counting(&acquires))
        .build()
        .expect("custom Authorization planning client");

    let exact = client
        .plan(&CredentialProofApi::unbound().exact_authorization())
        .expect("exact Authorization safe plan");
    let scheme = client
        .scheme_authorization()
        .plan()
        .expect("scheme Authorization safe plan");
    for plan in [&exact, &scheme] {
        let rendered = format!(
            "{plan:?}\n{}\n{:?}",
            plan.observed_url(),
            plan.public_headers()
        );
        assert_omits(&rendered, &[EXACT_SENTINEL, "provider-token", "tOkEn "]);
        let authorization = concord::http::HeaderName::from_static("authorization");
        assert!(plan.public_headers().get(&authorization).is_none());
    }
    assert!(exact.capabilities().authentication());
    assert_eq!(
        acquires.load(Ordering::SeqCst),
        0,
        "planning never acquires provider material"
    );
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn generated_exact_and_scheme_authorization_reach_only_the_tls_boundary() {
    const EXACT: &str = "Exact generated authorization 18d4";
    let tls = Tls::start();
    tls.queue(200, "exact");
    tls.queue(200, "scheme");
    let client = credential_proof_builder()
        .api_secret(secret(EXACT))
        .configure_http(tls.http())
        .build()
        .expect("custom Authorization client");

    client
        .exact_authorization()
        .await
        .expect("exact Authorization request");
    client
        .scheme_authorization()
        .await
        .expect("scheme Authorization request");

    let requests = tls.server.requests();
    assert_eq!(requests[0].header("authorization"), Some(EXACT));
    assert_eq!(
        requests[1].header("authorization"),
        Some("tOkEn provider-token")
    );
}

// ---------------------------------------------------------------------------
// Local credential controls (§11.3, §23.1).
// ---------------------------------------------------------------------------

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn local_controls_share_clone_state_keep_independent_builds_and_replace_atomically() {
    let tls = Tls::start();
    let client = auth_builder()
        .configure_http(tls.http())
        .build()
        .expect("first client");
    let clone = client.clone();

    // `replace_x` on one clone is visible to every clone of the same build.
    clone
        .credentials()
        .replace_acquired(SessionToken::new("endpoint-token"))
        .expect("endpoint replacement");
    client
        .nested()
        .replaced()
        .await
        .expect("installed on clone");
    // `clear_x` on the other clone removes it for both.
    client
        .credentials()
        .clear_acquired()
        .expect("endpoint clear");
    let cleared = clone
        .nested()
        .replaced()
        .await
        .expect_err("cleared endpoint material");
    assert_eq!(cleared.category(), ErrorCategory::AuthenticationUnavailable);
    assert_eq!(
        cleared.dispatch_knowledge(),
        DispatchKnowledge::NotDispatched
    );

    // Independently built clients never share stores.
    let independent = auth_builder()
        .configure_http(tls.http())
        .build()
        .expect("independent client");
    clone
        .credentials()
        .replace_acquired(SessionToken::new("clone-only"))
        .expect("clone replacement");
    client.nested().replaced().await.expect("shared by clones");
    let isolated = independent
        .nested()
        .replaced()
        .await
        .expect_err("an independent build has its own empty store");
    assert_eq!(
        isolated.category(),
        ErrorCategory::AuthenticationUnavailable
    );

    // Built-in material replacement: complete API key, bearer and atomic
    // Basic pair.
    client
        .credentials()
        .replace_application_key(ApiKey::try_new("replacement-api-key").expect("bounded key"))
        .expect("complete API-key replacement");
    client
        .credentials()
        .replace_static_bearer(AccessToken::try_new("replacement-bearer").expect("bounded token"))
        .expect("complete bearer replacement");
    client
        .credentials()
        .replace_login(
            BasicCredential::try_new("replacement-user", "replacement-password")
                .expect("bounded Basic pair"),
        )
        .expect("atomic Basic replacement");
    // An invalid Basic pair cannot be constructed, so no partial replacement
    // can ever be attempted; the previous pair remains installed.
    let invalid = BasicCredential::try_new("invalid:user", "replacement-password")
        .expect_err("Basic usernames cannot contain the delimiter");
    assert_eq!(
        invalid.kind(),
        CredentialMaterialErrorKind::BasicUsernameContainsColon
    );
    assert_eq!(
        AuthError::from(invalid).kind(),
        AuthErrorKind::InvalidConfiguration
    );

    client.api_key_endpoint().await.expect("replaced key");
    client.bearer_endpoint().await.expect("replaced bearer");
    client.basic_endpoint().await.expect("replaced Basic pair");

    // A provider-backed source accepts manually installed material without
    // calling the provider; clearing it makes the next call acquire again.
    // (v2's runtime provider replacement is removed: providers are builder
    // inputs, and `replace_x`/`clear_x` are the local controls — §11.3.)
    let provider_client = auth_builder()
        .configure_http(tls.http())
        .build()
        .expect("provider client");
    provider_client
        .credentials()
        .replace_rotating_bearer(AccessToken::try_new("manual-bearer").expect("bounded token"))
        .expect("manual provider-slot installation");
    provider_client
        .provider_bearer_endpoint()
        .await
        .expect("manual material is used");
    provider_client
        .credentials()
        .clear_rotating_bearer()
        .expect("provider-slot clear");
    provider_client
        .provider_bearer_endpoint()
        .await
        .expect("cleared provider slot acquires again");

    let requests = tls.server.requests();
    assert_eq!(requests.len(), 7);
    assert!(requests[0].target().starts_with("/nested/replaced?"));
    assert!(requests[0].target().contains("token=endpoint-token"));
    assert!(requests[1].target().contains("token=clone-only"));
    assert_eq!(requests[2].header("x-api-key"), Some("replacement-api-key"));
    assert_eq!(
        requests[3].header("authorization"),
        Some("Bearer replacement-bearer")
    );
    assert_eq!(
        requests[4].header("authorization"),
        Some("Basic cmVwbGFjZW1lbnQtdXNlcjpyZXBsYWNlbWVudC1wYXNzd29yZA==")
    );
    assert_eq!(
        requests[5].header("authorization"),
        Some("Bearer manual-bearer")
    );
    assert_eq!(
        requests[6].header("authorization"),
        Some(format!("Bearer {INITIAL_BEARER}").as_str())
    );
}

#[cfg(feature = "default-tls")] // https base: TlsUnavailable without TLS
#[test]
fn credential_replace_exact_limit_succeeds_and_first_excess_cannot_be_constructed() {
    let client = credential_proof_builder()
        .build()
        .expect("static credential client");
    client
        .credentials()
        .replace_application_key(
            ApiKey::try_new("x".repeat(MAX_CREDENTIAL_SECRET_BYTES)).expect("exact key limit"),
        )
        .expect("exact API-key replacement limit");

    const FIRST_EXCESS_SENTINEL: &str = "excess-api-key-sentinel-";
    let excessive = format!(
        "{FIRST_EXCESS_SENTINEL}{}",
        "x".repeat(MAX_CREDENTIAL_SECRET_BYTES)
    );
    let error = ApiKey::try_new(&excessive).expect_err("first excess material");
    assert_eq!(
        error.kind(),
        CredentialMaterialErrorKind::CredentialSecretTooLarge
    );
    assert_omits(
        &format!("{error} {error:?}"),
        &[FIRST_EXCESS_SENTINEL, &excessive],
    );
    let auth = AuthError::from(error);
    assert_eq!(auth.kind(), AuthErrorKind::MaterialTooLarge);
    assert_omits(&format!("{auth} {auth:?}"), &[FIRST_EXCESS_SENTINEL]);
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn failed_static_replacement_preserves_previous_material() {
    const INITIAL_KEY: &str = "preserved-static-api-key";
    let tls = Tls::start();
    let client = credential_proof_builder()
        .api_secret(secret(INITIAL_KEY))
        .configure_http(tls.http())
        .build()
        .expect("proxied authentication client");

    client
        .api_key_endpoint()
        .await
        .expect("request before failed replacement");
    let excessive = "x".repeat(MAX_CREDENTIAL_SECRET_BYTES + 1);
    // Construction fails before the store is touched: the atomic replacement
    // is never attempted.
    let error = ApiKey::try_new(&excessive)
        .map(|key| client.credentials().replace_application_key(key))
        .expect_err("first excess replacement material");
    assert_eq!(
        AuthError::from(error).kind(),
        AuthErrorKind::MaterialTooLarge
    );
    client
        .api_key_endpoint()
        .await
        .expect("request after failed replacement");

    let requests = tls.server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].header("x-api-key"), Some(INITIAL_KEY));
    assert_eq!(requests[1].header("x-api-key"), Some(INITIAL_KEY));
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn endpoint_backed_storage_uses_normal_endpoint_execution_and_preserves_on_failure() {
    let tls = Tls::start();
    tls.queue_session("acquired-session");
    tls.queue(200, "uses-acquired");
    tls.queue(500, "terminal");
    tls.queue(200, "uses-preserved");
    let client = auth_builder()
        .configure_http(tls.http())
        .build()
        .expect("proxied auth client");

    // Ordinary `.await` returns the material and stores nothing (CALL-03).
    // Here the terminal requests explicit installation.
    let outcome = client
        .login()
        .store_as_acquired()
        .await
        .expect("normal endpoint acquisition");
    assert!(matches!(outcome, StoreOutcome::Installed { .. }));
    client
        .nested()
        .replaced()
        .await
        .expect("installed endpoint material is applied");

    client
        .credentials()
        .replace_acquired(SessionToken::new("preserved-existing"))
        .expect("existing endpoint material");
    let error = client
        .login()
        .store_as_acquired()
        .await
        .expect_err("terminal endpoint failure");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(error.endpoint_id().as_str(), "auth.login");
    client
        .nested()
        .replaced()
        .await
        .expect("a failed acquisition does not clear earlier material");

    let requests = tls.server.requests();
    assert_eq!(requests.len(), 4);
    assert!(requests[0].target().starts_with("/login?tenant=tenant"));
    assert_eq!(requests[0].header("accept"), Some("application/session"));
    assert!(requests[1].target().contains("token=acquired-session"));
    assert!(requests[3].target().contains("token=preserved-existing"));
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn ordinary_await_of_a_source_endpoint_returns_material_without_storing_it() {
    let tls = Tls::start();
    tls.queue_session("returned-not-stored");
    let client = auth_builder()
        .configure_http(tls.http())
        .build()
        .expect("client");
    let material = client.login().await.expect("source endpoint result");
    assert_eq!(material.expose_secret(), "returned-not-stored");
    let error = client
        .nested()
        .replaced()
        .await
        .expect_err("nothing was installed");
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert_eq!(tls.request_count(), 1);
}

// ---------------------------------------------------------------------------
// V3-E11: unusable material fails before application send; challenge
// behavior follows the credential source.
// ---------------------------------------------------------------------------

fn assert_before_send(error: &ApiError, category: ErrorCategory) {
    assert_eq!(error.category(), category);
    assert_eq!(error.failure_stage(), FailureStage::CredentialPreflight);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert!(!error.may_have_reached_server());
    assert_eq!(error.last_response_status(), None);
}

pub struct FailingProvider {
    calls: Arc<AtomicUsize>,
    retry_after: Option<Duration>,
}

impl CredentialProvider for FailingProvider {
    type Material = SessionToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let retry_after = self.retry_after;
        Box::pin(async move {
            let error = AuthError::with_safe_message(
                AuthErrorKind::Acquisition,
                "fake provider is unavailable",
            );
            Err(match retry_after {
                Some(delay) => error.with_retry_after(delay),
                None => error,
            })
        })
    }
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn v3_e11_missing_expired_and_unobtainable_material_fails_before_application_send() {
    let tls = Tls::start();
    let client = auth_builder()
        .configure_http(tls.http())
        .build()
        .expect("client");

    // Endpoint-backed source never stored: no implicit login, no send.
    let never = client
        .nested()
        .replaced()
        .await
        .expect_err("empty endpoint-backed source");
    assert_before_send(&never, ErrorCategory::AuthenticationUnavailable);
    assert_eq!(never.auth_kind(), Some(AuthErrorKind::Unavailable));

    // Expired endpoint-backed material is not applied.
    client
        .credentials()
        .replace_acquired(SessionToken::expired("expired-session-sentinel"))
        .expect("expired material installs locally");
    let expired = client
        .nested()
        .replaced()
        .await
        .expect_err("expired endpoint material");
    assert_before_send(&expired, ErrorCategory::AuthenticationUnavailable);
    assert_omits(&render_error_chain(&expired), &["expired-session-sentinel"]);

    // A provider that cannot produce material fails before send.
    let calls = Arc::new(AtomicUsize::new(0));
    let failing = FlightApi::builder()
        .session(FlightProvider::Failing(FailingProvider {
            calls: Arc::clone(&calls),
            retry_after: None,
        }))
        .configure_http(tls.http())
        .build()
        .expect("failing provider client");
    let unobtainable = failing
        .protected()
        .await
        .expect_err("provider acquisition fails");
    assert_before_send(&unobtainable, ErrorCategory::AuthenticationProvider);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // The login source endpoint was never invoked and nothing was sent.
    assert_eq!(tls.request_count(), 0);
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn v3_e11_static_challenge_is_final_and_rejects_the_applied_material() {
    const STATIC_KEY: &str = "static-challenged-key-sentinel";
    let tls = Tls::start();
    tls.queue(401, "static rejected");
    let client = auth_builder()
        .api_secret(secret(STATIC_KEY))
        .configure_http(tls.http())
        .build()
        .expect("client");

    let rejected = client
        .static_challenged()
        .await
        .expect_err("a static challenge is final");
    assert_eq!(rejected.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(
        rejected.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(rejected.http_status(), Some(StatusCode::UNAUTHORIZED));
    assert_eq!(
        rejected
            .operation_counts()
            .expect("counts")
            .attempts_started(),
        1,
        "static material never refreshes, so nothing is replayed"
    );
    // The exact applied material is now rejected for future calls.
    let later = client
        .static_challenged()
        .await
        .expect_err("rejected static material is not reapplied");
    assert_eq!(later.auth_kind(), Some(AuthErrorKind::Rejected));
    assert_eq!(later.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    // An explicit local replacement installs usable material again.
    client
        .credentials()
        .replace_application_key(ApiKey::try_new("replacement-after-challenge").expect("key"))
        .expect("replacement");
    client
        .static_challenged()
        .await
        .expect("replacement material");

    let requests = tls.server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].header("x-api-key"), Some(STATIC_KEY));
    assert_eq!(
        requests[1].header("x-api-key"),
        Some("replacement-after-challenge")
    );
    assert_omits(
        &format!(
            "{}\n{}",
            render_error_chain(&rejected),
            render_error_chain(&later)
        ),
        &[STATIC_KEY],
    );
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn v3_e11_endpoint_backed_challenge_never_invokes_its_login_endpoint() {
    let tls = Tls::start();
    tls.queue(401, "session expired");
    let client = auth_builder()
        .configure_http(tls.http())
        .build()
        .expect("client");
    client
        .credentials()
        .replace_acquired(SessionToken::new("stale-session"))
        .expect("stored session");

    let rejected = client
        .acquired_challenged()
        .await
        .expect_err("endpoint-backed challenge is final");
    assert_eq!(rejected.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(
        rejected.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    let later = client
        .acquired_challenged()
        .await
        .expect_err("the rejected version is not reapplied");
    assert_eq!(later.auth_kind(), Some(AuthErrorKind::Rejected));
    assert_eq!(later.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    // One challenged application request; the login endpoint was never
    // called and nothing was replayed.
    let requests = tls.server.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].target().starts_with("/acquired-challenged?"));
    assert_eq!(
        requests[0].header("authorization"),
        Some("Bearer stale-session")
    );
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn v3_e11_provider_challenge_refreshes_exactly_once_for_a_safe_request() {
    let tls = Tls::start();
    tls.queue(401, "rotate");
    tls.queue(200, "recovered");
    let provider = BearerProvider::new();
    let acquires = Arc::clone(&provider.acquires);
    let refreshes = Arc::clone(&provider.refreshes);
    let client = auth_builder()
        .rotating_bearer(provider)
        .configure_http(tls.http())
        .build()
        .expect("client");

    let response = client
        .provider_bearer_endpoint()
        .response()
        .await
        .expect("authentication recovery succeeds");
    assert_eq!(response.value(), &Bytes::from_static(b"recovered"));
    assert_eq!(response.metadata().attempts_started(), 2);
    assert_eq!(response.metadata().policy_retries(), 0);
    assert!(response.metadata().recovered_authentication());
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);

    let requests = tls.server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].header("authorization"),
        Some(format!("Bearer {INITIAL_BEARER}").as_str())
    );
    assert_eq!(
        requests[1].header("authorization"),
        Some(format!("Bearer {REFRESHED_BEARER}").as_str())
    );
}

// RETRY-02 / V3-E17 (authentication part through the generated surface): a
// non-idempotent write that received a challenge is never replayed, even with
// a refresh-capable provider; the next call refreshes before dispatch.
#[cfg(feature = "default-tls")]
#[tokio::test]
async fn v3_e17_non_idempotent_challenge_is_never_replayed_by_auth_recovery() {
    let tls = Tls::start();
    tls.queue(401, "challenge on write");
    tls.queue(200, "later write");
    let provider = BearerProvider::new();
    let refreshes = Arc::clone(&provider.refreshes);
    let client = auth_builder()
        .rotating_bearer(provider)
        .configure_http(tls.http())
        .build()
        .expect("client");

    let error = client
        .provider_write()
        .await
        .expect_err("challenged write is not replayed");
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    let counts = error.operation_counts().expect("counts");
    assert_eq!(counts.attempts_started(), 1);
    assert_eq!(counts.authentication_recoveries(), 0);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    assert_eq!(tls.request_count(), 1);

    client.provider_write().await.expect("later write");
    let requests = tls.server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1].header("authorization"),
        Some(format!("Bearer {REFRESHED_BEARER}").as_str())
    );
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
}

// ---------------------------------------------------------------------------
// V3-E12 / V3-E13: latest-started explicit installation intent.
// ---------------------------------------------------------------------------

/// One barrier: the limiter records arrival, then waits until opened.
struct Gate {
    reached: AtomicBool,
    reached_signal: tokio::sync::Notify,
    open: tokio::sync::Semaphore,
}

impl Gate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            reached: AtomicBool::new(false),
            reached_signal: tokio::sync::Notify::new(),
            open: tokio::sync::Semaphore::new(0),
        })
    }

    fn arrive(&self) {
        self.reached.store(true, Ordering::SeqCst);
        self.reached_signal.notify_waiters();
    }

    async fn wait_reached(&self) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let signal = self.reached_signal.notified();
                if self.reached.load(Ordering::SeqCst) {
                    return;
                }
                signal.await;
            }
        })
        .await
        .expect("the gated operation reached its barrier");
    }

    fn release(&self) {
        self.open.add_permits(1);
    }

    async fn wait_open(&self) {
        self.open.acquire().await.expect("gate semaphore").forget();
    }
}

/// Application limiter admitting `session.login` operations through queued
/// gates, in call order; everything else is admitted immediately.
#[derive(Default)]
struct GateLimiter {
    gates: Mutex<VecDeque<Arc<Gate>>>,
}

impl GateLimiter {
    fn push(&self) -> Arc<Gate> {
        let gate = Gate::new();
        self.gates
            .lock()
            .expect("gate queue")
            .push_back(Arc::clone(&gate));
        gate
    }
}

impl RateLimiter for GateLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "concord.tests.gate", 1)
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let gate = if request.endpoint == "session.login" {
            self.gates.lock().expect("gate queue").pop_front()
        } else {
            None
        };
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.arrive();
                gate.wait_open().await;
            }
            Ok(RateLimitPermit::granted())
        })
    }
}

type HeadHookFn = Box<dyn FnMut(StatusCode) + Send>;

/// Observer running an installed hook synchronously when a response head is
/// observed — after dispatch, before challenge attribution.
#[derive(Default)]
struct HeadHook {
    hook: Mutex<Option<HeadHookFn>>,
}

impl HeadHook {
    fn install(&self, hook: impl FnMut(StatusCode) + Send + 'static) {
        *self.hook.lock().expect("hook") = Some(Box::new(hook));
    }

    fn remove(&self) {
        self.hook.lock().expect("hook").take();
    }
}

impl OperationObserver for HeadHook {
    fn observe(&self, event: OperationEvent<'_>) {
        if let OperationEvent::ResponseHeadObserved(head) = event {
            if let Some(hook) = self.hook.lock().expect("hook").as_mut() {
                hook(head.status);
            }
        }
    }
}

#[cfg(feature = "default-tls")]
struct SessionHarness {
    tls: Tls,
    limiter: Arc<GateLimiter>,
    hook: Arc<HeadHook>,
    client: SessionApi,
}

#[cfg(feature = "default-tls")]
fn session_harness() -> SessionHarness {
    let tls = Tls::start();
    let limiter = Arc::new(GateLimiter::default());
    let hook = Arc::new(HeadHook::default());
    let selected_limiter: Arc<dyn RateLimiter> = limiter.clone();
    let observer = Arc::clone(&hook);
    let client = SessionApi::builder()
        .configure_http(tls.http())
        .configure_runtime(move |runtime| {
            runtime
                .rate_limiter(selected_limiter)
                .operation_observer(observer)
        })
        .build()
        .expect("session client");
    SessionHarness {
        tls,
        limiter,
        hook,
        client,
    }
}

#[cfg(feature = "default-tls")]
type StoreTask = tokio::task::JoinHandle<Result<StoreOutcome, ApiError>>;

/// Starts one storage operation and waits until it has reserved its intent
/// and reached its admission barrier.
#[cfg(feature = "default-tls")]
async fn start_store(harness: &SessionHarness) -> (StoreTask, Arc<Gate>) {
    let gate = harness.limiter.push();
    let task = tokio::spawn(std::future::IntoFuture::into_future(
        harness.client.login().store_as_session(),
    ));
    gate.wait_reached().await;
    (task, gate)
}

#[cfg(feature = "default-tls")]
async fn finish(task: StoreTask) -> Result<StoreOutcome, ApiError> {
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("storage completes")
        .expect("storage task")
}

#[cfg(feature = "default-tls")]
async fn protected_session(harness: &SessionHarness) -> Result<Option<String>, ApiError> {
    harness.client.protected().await?;
    Ok(harness
        .tls
        .server
        .requests()
        .last()
        .and_then(|request| request.header("x-session").map(str::to_owned)))
}

#[cfg(feature = "default-tls")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_e12_start_then_clear_supersedes_and_leaves_the_slot_empty() {
    let harness = session_harness();
    harness.tls.queue_session("token-a");
    let (a, gate) = start_store(&harness).await;
    harness
        .client
        .credentials()
        .clear_session()
        .expect("manual clear");
    gate.release();
    assert_eq!(
        finish(a).await.expect("A completes remotely"),
        StoreOutcome::Superseded
    );
    let error = protected_session(&harness)
        .await
        .expect_err("slot is empty");
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(harness.tls.request_count(), 1, "only A's login was sent");
}

#[cfg(feature = "default-tls")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_e12_start_then_manual_replace_supersedes_and_keeps_the_replacement() {
    let harness = session_harness();
    harness.tls.queue_session("token-a");
    let (a, gate) = start_store(&harness).await;
    let replaced = harness
        .client
        .credentials()
        .replace_session(SessionToken::new("manual-b"))
        .expect("manual replace");
    gate.release();
    assert_eq!(finish(a).await.expect("A"), StoreOutcome::Superseded);
    assert_eq!(
        protected_session(&harness).await.expect("B remains"),
        Some("manual-b".to_owned())
    );
    // A later explicit replacement allocates a different installed version.
    let next = harness
        .client
        .credentials()
        .replace_session(SessionToken::new("manual-c"))
        .expect("second replace");
    assert_ne!(replaced, next);
}

#[cfg(feature = "default-tls")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_e12_two_overlapping_starts_install_only_the_latest_started() {
    let harness = session_harness();
    harness.tls.queue_session("token-a");
    harness.tls.queue_session("token-b");
    let (a, gate_a) = start_store(&harness).await;
    let (b, gate_b) = start_store(&harness).await;
    gate_a.release();
    assert_eq!(finish(a).await.expect("A"), StoreOutcome::Superseded);
    gate_b.release();
    assert!(matches!(
        finish(b).await.expect("B"),
        StoreOutcome::Installed { .. }
    ));
    assert_eq!(
        protected_session(&harness).await.expect("B installed"),
        Some("token-b".to_owned())
    );
}

#[cfg(feature = "default-tls")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_e12_newer_failure_never_revives_the_older_pending_intent() {
    let harness = session_harness();
    harness
        .client
        .credentials()
        .replace_session(SessionToken::new("pre-existing"))
        .expect("pre-existing ready value");
    harness.tls.queue(500, "b fails");
    harness.tls.queue_session("token-a");
    let (a, gate_a) = start_store(&harness).await;
    let (b, gate_b) = start_store(&harness).await;
    gate_b.release();
    let failed = finish(b).await.expect_err("B fails remotely");
    assert_eq!(failed.category(), ErrorCategory::HttpStatus);
    gate_a.release();
    assert_eq!(
        finish(a).await.expect("A completes"),
        StoreOutcome::Superseded,
        "a failed newer intent does not revive the older one"
    );
    assert_eq!(
        protected_session(&harness)
            .await
            .expect("pre-existing value remains"),
        Some("pre-existing".to_owned())
    );
}

#[cfg(feature = "default-tls")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_e12_rejecting_an_old_version_keeps_a_newer_pending_intent_eligible() {
    let harness = session_harness();
    harness
        .client
        .credentials()
        .replace_session(SessionToken::new("version-v"))
        .expect("installed version V");
    harness.tls.queue(401, "reject V");
    harness.tls.queue_session("token-b");
    let (b, gate_b) = start_store(&harness).await;
    let rejected = protected_session(&harness)
        .await
        .expect_err("V is challenged");
    assert_eq!(rejected.category(), ErrorCategory::AuthenticationRejected);
    gate_b.release();
    assert!(matches!(
        finish(b).await.expect("B"),
        StoreOutcome::Installed { .. }
    ));
    assert_eq!(
        protected_session(&harness).await.expect("B usable"),
        Some("token-b".to_owned())
    );
    let requests = harness.tls.server.requests();
    assert_eq!(requests[0].header("x-session"), Some("version-v"));
}

#[cfg(feature = "default-tls")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_e12_an_unpolled_storage_request_created_before_clear_is_a_new_intent() {
    let harness = session_harness();
    harness.tls.queue_session("token-a");
    let unpolled = harness.client.login().store_as_session();
    harness
        .client
        .credentials()
        .clear_session()
        .expect("clear before first poll");
    assert!(matches!(
        unpolled.await.expect("A first polled after clear"),
        StoreOutcome::Installed { .. }
    ));
    assert_eq!(
        protected_session(&harness).await.expect("A installed"),
        Some("token-a".to_owned())
    );
}

#[cfg(feature = "default-tls")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_e13_late_challenge_for_an_old_version_cannot_erase_newer_material_or_intent() {
    let harness = session_harness();
    harness
        .client
        .credentials()
        .replace_session(SessionToken::new("old-version"))
        .expect("old version installed");
    harness.tls.queue(401, "late challenge");
    // A newer explicit intent is pending while the old version is in flight.
    let (pending, gate) = start_store(&harness).await;
    // While the old version's challenge head is being processed, a newer
    // version is installed manually.
    let client = harness.client.clone();
    harness.hook.install(move |status| {
        if status == StatusCode::UNAUTHORIZED {
            client
                .credentials()
                .replace_session(SessionToken::new("newer-version"))
                .expect("newer version installed before attribution");
        }
    });
    let rejected = protected_session(&harness)
        .await
        .expect_err("the old version is challenged");
    harness.hook.remove();
    assert_eq!(rejected.category(), ErrorCategory::AuthenticationRejected);
    // The late challenge rejected only the applied old version: the newer
    // installed version is still usable.
    assert_eq!(
        protected_session(&harness)
            .await
            .expect("newer version usable"),
        Some("newer-version".to_owned())
    );
    let requests = harness.tls.server.requests();
    assert_eq!(requests[0].header("x-session"), Some("old-version"));

    // The manual replacement superseded the pending intent (§11.3); the
    // challenge itself did not invalidate anything newer. A fresh explicit
    // start after that still installs.
    harness.tls.queue_session("pending-token");
    gate.release();
    assert_eq!(
        finish(pending).await.expect("pending"),
        StoreOutcome::Superseded
    );
    harness.tls.queue_session("fresh-token");
    let (fresh, fresh_gate) = start_store(&harness).await;
    fresh_gate.release();
    assert!(matches!(
        finish(fresh).await.expect("fresh"),
        StoreOutcome::Installed { .. }
    ));
    assert_eq!(
        protected_session(&harness).await.expect("fresh usable"),
        Some("fresh-token".to_owned())
    );
}

// ---------------------------------------------------------------------------
// V3-E14: provider single flight, leader cancellation and cooldown.
// ---------------------------------------------------------------------------

/// Provider variants for flight/cooldown tests.
pub enum FlightProvider {
    Gated(GatedProvider),
    Failing(FailingProvider),
}

pub struct GatedProvider {
    calls: Arc<AtomicUsize>,
    gates: Mutex<VecDeque<Arc<Gate>>>,
}

impl CredentialProvider for FlightProvider {
    type Material = SessionToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        match self {
            Self::Failing(provider) => provider.acquire(context),
            Self::Gated(provider) => {
                let call = provider.calls.fetch_add(1, Ordering::SeqCst);
                let gate = provider.gates.lock().expect("provider gates").pop_front();
                Box::pin(async move {
                    if let Some(gate) = gate {
                        gate.arrive();
                        gate.wait_open().await;
                    }
                    Ok(SessionToken::new(&format!("flight-token-{call}")))
                })
            }
        }
    }
}

#[cfg(feature = "default-tls")]
fn flight_client(tls: &Tls, provider: FlightProvider) -> FlightApi {
    FlightApi::builder()
        .session(provider)
        .configure_http(tls.http())
        .build()
        .expect("flight client")
}

#[cfg(feature = "default-tls")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_e14_followers_join_the_single_leader_acquisition() {
    let tls = Tls::start();
    let calls = Arc::new(AtomicUsize::new(0));
    let gate = Gate::new();
    let client = flight_client(
        &tls,
        FlightProvider::Gated(GatedProvider {
            calls: Arc::clone(&calls),
            gates: Mutex::new(VecDeque::from([Arc::clone(&gate)])),
        }),
    );

    let leader = tokio::spawn(std::future::IntoFuture::into_future(client.protected()));
    gate.wait_reached().await;
    let mut followers: Vec<_> = (0..3)
        .map(|_| tokio::spawn(std::future::IntoFuture::into_future(client.protected())))
        .collect();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1, "followers do not acquire");
    // A cancelled follower releases its waiter slot and never cancels the
    // live leader.
    let cancelled = followers.remove(0);
    cancelled.abort();
    assert!(
        cancelled
            .await
            .expect_err("aborted follower")
            .is_cancelled()
    );
    gate.release();
    for task in std::iter::once(leader).chain(followers) {
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .expect("no hung waiter")
            .expect("join")
            .expect("request with shared material");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let requests = tls.server.requests();
    assert_eq!(requests.len(), 3);
    assert!(
        requests
            .iter()
            .all(|request| request.header("x-session") == Some("flight-token-0"))
    );
}

#[cfg(feature = "default-tls")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_e14_cancelled_leader_retires_its_flight_and_followers_do_not_hang() {
    let tls = Tls::start();
    let calls = Arc::new(AtomicUsize::new(0));
    let stuck = Gate::new();
    let client = flight_client(
        &tls,
        FlightProvider::Gated(GatedProvider {
            calls: Arc::clone(&calls),
            // The first acquisition never completes; later ones are immediate.
            gates: Mutex::new(VecDeque::from([Arc::clone(&stuck)])),
        }),
    );

    let leader = tokio::spawn(std::future::IntoFuture::into_future(client.protected()));
    stuck.wait_reached().await;
    let follower = tokio::spawn(std::future::IntoFuture::into_future(client.protected()));
    tokio::time::sleep(Duration::from_millis(100)).await;
    leader.abort();
    assert!(leader.await.expect_err("aborted leader").is_cancelled());
    tokio::time::timeout(Duration::from_secs(10), follower)
        .await
        .expect("the follower does not hang after the leader is cancelled")
        .expect("join")
        .expect("the follower completes with a new acquisition");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let requests = tls.server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].header("x-session"), Some("flight-token-1"));
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn v3_e14_provider_failure_cooldown_is_finite_and_capped() {
    let tls = Tls::start();
    let calls = Arc::new(AtomicUsize::new(0));
    let client = FlightApi::builder()
        .session(FlightProvider::Failing(FailingProvider {
            calls: Arc::clone(&calls),
            retry_after: Some(Duration::from_secs(3_600)),
        }))
        .configure_http(tls.http())
        .configure_runtime(|runtime| runtime.auth_failure_cooldown_cap(Duration::from_millis(300)))
        .build()
        .expect("cooldown client");

    let first = client.protected().await.expect_err("provider fails");
    assert_before_send(&first, ErrorCategory::AuthenticationProvider);
    let during = client
        .protected()
        .await
        .expect_err("cooldown suppresses a new acquisition");
    assert_eq!(
        during.dispatch_knowledge(),
        DispatchKnowledge::NotDispatched
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "cooldown reused the failure"
    );
    // The provider's hour-long hint is capped by the configured cooldown cap.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let after = client
        .protected()
        .await
        .expect_err("provider is asked again after the capped cooldown");
    assert_before_send(&after, ErrorCategory::AuthenticationProvider);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(tls.request_count(), 0);
}

// ---------------------------------------------------------------------------
// V3-E15: failed local installation after a successful remote response, and
// the final observation of a storage operation (§11.4, EXEC-04). Common caps
// apply through local installation, so a genuine delay before installation
// fails it with the exchange's contact facts; the final observation follows
// the fixed storage result, so a slow final observer cannot change it.
// ---------------------------------------------------------------------------

type FinalRecord = (OperationId, CompletionOutcome, Option<StatusCode>, u32);

/// Observer that blocks the final callback of one source endpoint, longer
/// than the operation's whole timeout, and records its final observations.
struct SlowFinal {
    endpoint: &'static str,
    delay: Duration,
    finished: Mutex<Vec<FinalRecord>>,
}

impl SlowFinal {
    fn new(endpoint: &'static str, delay: Duration) -> Arc<Self> {
        Arc::new(Self {
            endpoint,
            delay,
            finished: Mutex::new(Vec::new()),
        })
    }

    fn only(&self) -> FinalRecord {
        let finished = self.finished.lock().expect("final records");
        assert_eq!(finished.len(), 1, "one final observation");
        finished[0]
    }
}

impl OperationObserver for SlowFinal {
    fn observe(&self, event: OperationEvent<'_>) {
        if let OperationEvent::OperationFinished(finished) = event {
            if finished.endpoint == self.endpoint {
                self.finished.lock().expect("final records").push((
                    finished.operation_id,
                    finished.outcome,
                    finished.final_status,
                    finished.attempts_started,
                ));
                std::thread::sleep(self.delay);
            }
        }
    }
}

/// Endpoint-backed material whose application-defined expiry read, the
/// installation's own synchronous extension call, blocks once for longer than
/// the storage operation's remaining timeout: a genuine delay after the
/// exchange and before local installation.
pub struct SlowExpiryToken {
    secret: SecretString,
    delay: Duration,
    delayed: AtomicBool,
}

impl CredentialMaterial for SlowExpiryToken {
    fn expires_at(&self) -> Option<Instant> {
        if !self.delayed.swap(true, Ordering::SeqCst) {
            std::thread::sleep(self.delay);
        }
        None
    }
}

impl SecretCredential for SlowExpiryToken {
    fn expose_secret(&self) -> &str {
        self.secret.expose_secret()
    }
}

/// Source codec of [`SlowExpiryToken`].
pub struct SlowExpiryCodec;

impl DecodeBody for SlowExpiryCodec {
    type Output = SlowExpiryToken;

    const ACCEPT: Option<&'static str> = Some("application/session");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let value = String::from_utf8(body.to_vec())
            .map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        SecretString::try_new(value)
            .map(|secret| SlowExpiryToken {
                secret,
                delay: Duration::from_millis(2_000),
                delayed: AtomicBool::new(false),
            })
            .map_err(|_| CodecError::new(CodecErrorKind::Decode))
    }
}

api! {
    /// Endpoint-backed session whose material delays local installation.
    client SlowInstallApi {
        timeout 30s;
        base "https://localhost/";
        credentials {
            credential session = endpoint login;
        }

        GET ping() -> Bytes via Binary {
            id "slow_install.ping";
            path "ping";
        }
        GET login() -> SlowExpiryToken via SlowExpiryCodec {
            id "slow_install.login";
            path "login";
        }
        GET protected() -> Bytes via Binary {
            id "slow_install.protected";
            path "protected";
            auth { header "X-Session" = session; }
        }
    }
}

// A genuine delay before local installation (the material's slow expiry
// read) exceeds the storage operation's timeout: installation fails with a
// correlated Timeout at CredentialInstall that keeps the exchange's contact
// facts, nothing is installed, and the one final observation reports that
// same failure for the same operation.
#[cfg(feature = "default-tls")]
#[tokio::test]
async fn v3_e15_failed_local_install_returns_a_correlated_error_with_contact_facts() {
    let tls = Tls::start();
    tls.queue(200, "warm-up");
    tls.queue_session("never-installed-sentinel");
    let observer = SlowFinal::new("slow_install.login", Duration::ZERO);
    let recorded: Arc<dyn OperationObserver> = observer.clone();
    let client = SlowInstallApi::builder()
        .configure_http(tls.http())
        .configure_runtime(move |runtime| runtime.operation_observer(recorded))
        .build()
        .expect("client");
    // Establish the pooled connection so the storage operation's remote part
    // completes well inside its deadline.
    client.ping().await.expect("warm-up request");

    let error = client
        .login()
        .store_as_session()
        .timeout(Duration::from_millis(1_500))
        .expect("valid timeout")
        .await
        .expect_err("the deadline expires before local installation");
    // AUTH-05: the error uses the validated operational ID from the journal,
    // never the display path `SlowInstallApi::login` (uppercase and `::`).
    assert_eq!(error.endpoint_id().as_str(), "slow_install.login");
    assert_eq!(error.endpoint(), "slow_install.login");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(error.failure_stage(), FailureStage::CredentialInstall);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert_omits(&render_error_chain(&error), &["never-installed-sentinel"]);
    // EXEC-04: the final observation reports the fixed failure, not Success.
    let (operation_id, outcome, final_status, attempts) = observer.only();
    assert_eq!(error.operation_id(), Some(operation_id));
    assert_eq!(outcome, CompletionOutcome::Timeout);
    assert_eq!(final_status, Some(StatusCode::OK));
    assert_eq!(attempts, 1);
    // Nothing was installed.
    let unavailable = client.protected().await.expect_err("installation failed");
    assert_eq!(
        unavailable.category(),
        ErrorCategory::AuthenticationUnavailable
    );
    assert_eq!(tls.request_count(), 2);
}

// A slow final observer runs after the storage result is fixed: blocking it
// for longer than the whole timeout delays delivery but cannot turn the
// installed outcome into a Timeout (EXEC-04). The material is installed and
// applied afterwards.
#[cfg(feature = "default-tls")]
#[tokio::test]
async fn v3_e15_slow_final_observer_cannot_change_a_fixed_storage_result() {
    let tls = Tls::start();
    tls.queue(200, "warm-up");
    tls.queue_session("installed-before-final-observation");
    tls.queue(200, "uses-installed");
    let observer = SlowFinal::new("auth.login", Duration::from_millis(2_000));
    let recorded: Arc<dyn OperationObserver> = observer.clone();
    let http = tls.http();
    let client = auth_builder()
        // The loopback TLS server closes a connection idle for 2 s. A pooled
        // connection idle for 1 s is discarded, so the request after the
        // 2-second final callback never reuses a connection the server may
        // already be closing.
        .configure_http(move |builder| {
            http(builder).pool_idle_timeout(Duration::from_millis(1_000))
        })
        .configure_runtime(move |runtime| runtime.operation_observer(recorded))
        .build()
        .expect("client");
    client.api_key_endpoint().await.expect("warm-up request");

    let outcome = client
        .login()
        .store_as_acquired()
        .timeout(Duration::from_millis(1_500))
        .expect("valid timeout")
        .await
        .expect("a slow final observer cannot change the fixed storage result");
    assert!(matches!(outcome, StoreOutcome::Installed { .. }));
    let (_, observed, final_status, attempts) = observer.only();
    assert_eq!(observed, CompletionOutcome::Success);
    assert_eq!(final_status, Some(StatusCode::OK));
    assert_eq!(attempts, 1);
    client
        .nested()
        .replaced()
        .await
        .expect("the installed material is applied");
    let requests = tls.server.requests();
    assert_eq!(requests.len(), 3);
    assert!(
        requests[2]
            .target()
            .contains("token=installed-before-final-observation")
    );
}

// ---------------------------------------------------------------------------
// EXEC-01 (§13.1, §13.7): a provider acquisition is its own operation
// identity with a bounded journal linked to the leading generated request.
// ---------------------------------------------------------------------------

type ProviderRecord = (ProviderAcquisitionFacts, Option<AuthErrorKind>);

#[derive(Default)]
struct ProviderJournalObserver {
    providers: Mutex<Vec<ProviderRecord>>,
    operations: Mutex<Vec<OperationId>>,
}

impl OperationObserver for ProviderJournalObserver {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::ProviderAcquisitionFinished(finished) => self
                .providers
                .lock()
                .expect("provider records")
                .push((finished.facts, finished.auth_kind)),
            OperationEvent::OperationFinished(finished) => self
                .operations
                .lock()
                .expect("operation records")
                .push(finished.operation_id),
            _ => {}
        }
    }
}

#[cfg(feature = "default-tls")]
fn observed_flight_client(
    tls: &Tls,
    provider: FlightProvider,
    observer: &Arc<ProviderJournalObserver>,
) -> FlightApi {
    let observer: Arc<dyn OperationObserver> = Arc::clone(observer) as _;
    FlightApi::builder()
        .session(provider)
        .configure_http(tls.http())
        .configure_runtime(move |runtime| runtime.operation_observer(observer))
        .build()
        .expect("observed flight client")
}

#[cfg(feature = "default-tls")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_01_generated_leader_and_follower_share_one_distinct_linked_provider_identity() {
    let tls = Tls::start();
    tls.queue(200, "leader");
    tls.queue(200, "follower");
    tls.queue(200, "installed");
    let calls = Arc::new(AtomicUsize::new(0));
    let gate = Gate::new();
    let observer = Arc::new(ProviderJournalObserver::default());
    let client = observed_flight_client(
        &tls,
        FlightProvider::Gated(GatedProvider {
            calls: Arc::clone(&calls),
            gates: Mutex::new(VecDeque::from([Arc::clone(&gate)])),
        }),
        &observer,
    );

    let leader = tokio::spawn(std::future::IntoFuture::into_future(
        client.protected().response(),
    ));
    gate.wait_reached().await;
    let follower = tokio::spawn(std::future::IntoFuture::into_future(
        client.protected().response(),
    ));
    tokio::time::sleep(Duration::from_millis(100)).await;
    gate.release();
    let mut metadata = Vec::new();
    for task in [leader, follower] {
        let response = tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .expect("no hung operation")
            .expect("join")
            .expect("provider-backed generated request");
        metadata.push(response.metadata().provider_acquisition());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1, "one flight");

    let providers = observer.providers.lock().expect("provider records").clone();
    assert_eq!(
        providers.len(),
        1,
        "the leader alone reports the acquisition"
    );
    let (facts, auth_kind) = providers[0];
    assert_eq!(auth_kind, None);
    assert_eq!(facts.outcome(), ProviderAcquisitionOutcome::Installed);
    assert_eq!(facts.reason(), CredentialReason::Missing);
    // This provider performs no helper HTTP: its separate journal records no
    // auxiliary network activity.
    assert_eq!(facts.helper_sends(), 0);
    assert_eq!(facts.last_status(), None);
    assert_eq!(facts.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    // Leader and follower link the very same provider identity...
    assert_eq!(metadata, vec![Some(facts), Some(facts)]);
    // ...which is distinct from both application operations and parented by
    // exactly one of them.
    let operations = observer
        .operations
        .lock()
        .expect("operation records")
        .clone();
    assert_eq!(operations.len(), 2);
    assert_ne!(operations[0], operations[1]);
    assert!(!operations.contains(&facts.operation_id()));
    assert!(operations.contains(&facts.parent_operation_id().expect("leader")));

    // Installed material is reused without a new acquisition.
    let later = client
        .protected()
        .response()
        .await
        .expect("request with installed material");
    assert_eq!(later.metadata().provider_acquisition(), None);
    assert_eq!(
        observer.providers.lock().expect("provider records").len(),
        1
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn exec_01_generated_provider_failure_reports_its_distinct_linked_provider_identity() {
    let tls = Tls::start();
    let calls = Arc::new(AtomicUsize::new(0));
    let observer = Arc::new(ProviderJournalObserver::default());
    let client = observed_flight_client(
        &tls,
        FlightProvider::Failing(FailingProvider {
            calls: Arc::clone(&calls),
            retry_after: None,
        }),
        &observer,
    );

    let error = client.protected().await.expect_err("provider fails");
    assert_eq!(error.category(), ErrorCategory::AuthenticationProvider);
    assert_eq!(error.failure_stage(), FailureStage::CredentialPreflight);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(tls.request_count(), 0);
    let application = error.operation_id().expect("application operation");
    let facts = error.provider_acquisition().expect("provider journal");
    assert_ne!(facts.operation_id(), application);
    assert_eq!(facts.parent_operation_id(), Some(application));
    assert_eq!(facts.outcome(), ProviderAcquisitionOutcome::Failed);
    assert_eq!(
        observer.providers.lock().expect("provider records").clone(),
        vec![(facts, Some(AuthErrorKind::Acquisition))]
    );
    assert_eq!(
        observer
            .operations
            .lock()
            .expect("operation records")
            .clone(),
        vec![application]
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

// ---------------------------------------------------------------------------
// V3-S10 (credential material): sentinels never reach automatic output while
// the real request carries the exact private value.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RecordingObserver {
    events: Mutex<Vec<String>>,
}

impl OperationObserver for RecordingObserver {
    fn observe(&self, event: OperationEvent<'_>) {
        self.events
            .lock()
            .expect("events")
            .push(format!("{event:?}"));
    }
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn v3_s10_credential_sentinels_are_absent_from_every_automatic_output() {
    const API: &str = "s10-api-key-sentinel-7c1e";
    const BEARER: &str = "s10-bearer-sentinel-2d90";
    const USER: &str = "s10-user-sentinel-a41f";
    const PASSWORD: &str = "s10-password-sentinel-93bb";
    const SESSION: &str = "s10-session-sentinel-0e55";
    let sentinels = [API, BEARER, USER, PASSWORD, SESSION];

    let tls = Tls::start();
    tls.queue(200, "ok");
    tls.queue_session(SESSION);
    tls.queue(401, "challenge");
    let recorder = Arc::new(RecordingObserver::default());
    let observer = Arc::clone(&recorder);
    let builder = AuthApi::builder()
        .set_tenant("tenant")
        .api_secret(secret(API))
        .bearer_secret(secret(BEARER))
        .username(secret(USER))
        .password(secret(PASSWORD))
        .rotating_bearer(BearerProvider::new())
        .session(SessionProvider::new())
        .configure_http(tls.http())
        .configure_runtime(move |runtime| runtime.operation_observer(observer));
    let builder_debug = format!("{builder:?}");
    let client = builder.build().expect("client");

    client.api_key_endpoint().await.expect("API key request");
    let outcome = client
        .login()
        .store_as_acquired()
        .await
        .expect("stored session");
    let rejected = client
        .acquired_challenged()
        .await
        .expect_err("challenged session");
    let plans = [
        client.api_key_endpoint().plan().expect("plan"),
        client.basic_endpoint().plan().expect("plan"),
        client.bearer_endpoint().plan().expect("plan"),
        client.acquired_challenged().plan().expect("plan"),
    ];

    let mut rendered = vec![
        builder_debug,
        format!("{client:?}"),
        format!("{:?}", client.credentials()),
        format!("{outcome:?}"),
        render_error_chain(&rejected),
        format!("{:?}", rejected.http_response()),
        format!("{:?}", client.api_key_endpoint()),
        format!("{:?}", AuthApi::contract()),
    ];
    for plan in &plans {
        rendered.push(format!(
            "{plan:?}\n{}\n{:?}",
            plan.observed_url(),
            plan.public_headers()
        ));
    }
    rendered.extend(recorder.events.lock().expect("events").iter().cloned());
    assert!(recorder.events.lock().expect("events").len() >= 6);
    assert_omits(&rendered.join("\n"), &sentinels);

    // The real private placements are exact on the wire.
    let requests = tls.server.requests();
    assert_eq!(requests[0].header("x-api-key"), Some(API));
    assert_eq!(
        requests[2].header("authorization"),
        Some(format!("Bearer {SESSION}").as_str())
    );
}
