#![cfg(feature = "dangerous-dev-tools")]

use bytes::Bytes;
use concord_core::advanced::{PreparedBody, PreparedEndpoint, PreparedRequestEntity};
use concord_core::prelude::{ApiClient, ClientContext, Text};
use http::{Method, StatusCode};

#[derive(Clone)]
#[cfg(not(feature = "default-tls"))]
struct HttpContext;

#[cfg(not(feature = "default-tls"))]
impl ClientContext for HttpContext {
    type Vars = ();
    type AuthVars = ();
    type AuthState = ();

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTP;
    const DOMAIN: &'static str = "feature-boundary.example";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}
}

#[derive(Clone)]
struct HttpsContext;

impl ClientContext for HttpsContext {
    type Vars = ();
    type AuthVars = ();
    type AuthState = ();

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "feature-boundary.example";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}
}

fn endpoint() -> PreparedEndpoint<Text<String>> {
    PreparedEndpoint::new(
        "TlsFeatureBoundary",
        Method::GET,
        "/value",
        PreparedRequestEntity {
            body: PreparedBody::empty(),
        },
    )
}

fn synthetic_client<Cx: ClientContext>(
    executor: concord_core::__development::DeterministicNativeExecutor,
) -> ApiClient<Cx>
where
    Cx::Vars: Default,
    Cx::AuthVars: Default,
{
    ApiClient::with_safe_reqwest_builder(Default::default(), Default::default(), |builder| {
        concord_core::__development::configure_application_executor(builder, executor)
            .expect("application executor configuration")
    })
    .expect("managed client")
}

#[cfg(not(feature = "default-tls"))]
#[tokio::test]
async fn no_tls_feature_boundary_http_executes_and_https_preflights() {
    let http_executor = concord_core::__development::DeterministicNativeExecutor::application();
    http_executor.script_response(
        concord_core::__development::ScriptedNativeResponse::bytes(
            StatusCode::OK,
            Bytes::from_static(b"http-ok"),
        )
        .with_header(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/plain"),
        ),
    );
    let http_client = synthetic_client::<HttpContext>(http_executor.clone());
    assert_eq!(
        endpoint()
            .execute(&http_client)
            .await
            .expect("HTTP remains usable without TLS"),
        "http-ok"
    );
    assert_eq!(http_executor.captures().len(), 1);
    assert_eq!(http_executor.remaining_scripts(), 0);

    let https_executor = concord_core::__development::DeterministicNativeExecutor::application();
    https_executor.script_response(concord_core::__development::ScriptedNativeResponse::bytes(
        StatusCode::OK,
        Bytes::from_static(b"unused"),
    ));
    let https_client = synthetic_client::<HttpsContext>(https_executor.clone());
    let error = endpoint()
        .execute(&https_client)
        .await
        .expect_err("HTTPS must fail before deterministic execution");
    assert!(matches!(
        &error,
        concord_core::prelude::ApiClientError::TlsCapabilityUnavailable { .. }
    ));
    assert!(https_executor.captures().is_empty());
    assert_eq!(https_executor.remaining_scripts(), 1);

    let production_error = endpoint()
        .execute(&ApiClient::<HttpsContext>::new((), ()))
        .await
        .expect_err("ordinary managed execution uses the same early preflight");
    assert!(matches!(
        &production_error,
        concord_core::prelude::ApiClientError::TlsCapabilityUnavailable { .. }
    ));
    assert_eq!(production_error.category(), error.category());
    assert_eq!(production_error.to_string(), error.to_string());
    assert_eq!(format!("{production_error:?}"), format!("{error:?}"));
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn tls_enabled_feature_boundary_https_reaches_deterministic_execution() {
    let executor = concord_core::__development::DeterministicNativeExecutor::application();
    executor.script_response(
        concord_core::__development::ScriptedNativeResponse::bytes(
            StatusCode::OK,
            Bytes::from_static(b"https-ok"),
        )
        .with_header(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/plain"),
        ),
    );
    let client = synthetic_client::<HttpsContext>(executor.clone());
    assert_eq!(
        endpoint()
            .execute(&client)
            .await
            .expect("compiled TLS capability permits HTTPS"),
        "https-ok"
    );
    assert_eq!(executor.captures().len(), 1);
    assert_eq!(executor.remaining_scripts(), 0);
}
