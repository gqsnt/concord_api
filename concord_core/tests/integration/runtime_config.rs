use concord_core::advanced::{Transport, TransportError, TransportRequest, TransportResponse};
use concord_core::prelude::{ApiClient, ClientContext, DebugLevel};
use std::future::Future;
use std::pin::Pin;

#[derive(Clone)]
struct TestCx;

#[derive(Clone)]
struct NoopTransport;

impl Transport for NoopTransport {
    fn send(
        &self,
        _req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        Box::pin(async move {
            Err(TransportError::with_kind(
                concord_core::advanced::TransportErrorKind::Request,
                std::io::Error::other("noop transport should not be used"),
            ))
        })
    }
}

impl ClientContext for TestCx {
    type Vars = ();
    type AuthVars = ();
    type AuthState = ();

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_: &Self::Vars, _: &Self::AuthVars) -> Self::AuthState {}
}

#[test]
fn configure_updates_debug_level_and_pagination_loop_detection() {
    let mut api: ApiClient<TestCx, NoopTransport> =
        ApiClient::with_transport((), (), NoopTransport);

    api.configure(|cfg| {
        cfg.debug_level(DebugLevel::VV)
            .pagination_detect_loops(false);
    });

    assert_eq!(api.debug_level(), DebugLevel::VV);
    assert!(!api.pagination_detect_loops());
}
