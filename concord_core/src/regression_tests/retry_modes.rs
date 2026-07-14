use super::common::{TestAuthState, TestAuthVars, deterministic_mock};
use super::test_api::{
    EndpointMeta, EndpointPlan, PreparedBody, RequestOverrides, RequestPlan, ResolvedPolicy,
    ResolvedRoute, ResponsePlan,
};
use crate::prelude::{ApiClient, ClientContext, RetryMode, Text};
use http::{HeaderValue, Method, StatusCode};

#[derive(Clone)]
struct DisabledRetryCx;

impl ClientContext for DisabledRetryCx {
    type Vars = ();
    type AuthVars = TestAuthVars;
    type AuthState = TestAuthState<Self>;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTP;
    const DOMAIN: &'static str = "retry.example";

    fn init_auth_state(_vars: &Self::Vars, auth: &Self::AuthVars) -> Self::AuthState {
        TestAuthState::new(auth)
    }

    fn auth_provider_binding<'a>(
        credential: &crate::advanced::CredentialId,
        auth_state: &'a Self::AuthState,
    ) -> Option<crate::advanced::AuthProviderBinding<'a, Self>> {
        auth_state.binding(credential)
    }
}

fn disabled_retry_plan() -> RequestPlan {
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name: "DisabledRetryMode",
                method: Method::GET,
                idempotent: true,
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTP, "retry.example", "/status"),
            policy: ResolvedPolicy::default(),
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static("text/plain")),
                no_content: false,
                format: crate::codec::Format::Text,
            },
            pagination: None,
        },
        body: PreparedBody::empty(),
        overrides: RequestOverrides::default(),
    }
}

#[tokio::test]
async fn disabled_retry_mode_installs_without_a_concord_resend() {
    let (mock, handle) = deterministic_mock::deterministic_mock()
        .reply(deterministic_mock::ScriptedReply::status(
            StatusCode::SERVICE_UNAVAILABLE,
        ))
        .build();
    let client = ApiClient::<DisabledRetryCx>::with_safe_reqwest_builder_and_retry_mode(
        (),
        TestAuthVars::default(),
        RetryMode::Disabled,
        |builder| Ok(mock.configure_application(builder)),
    )
    .expect("disabled retry client");

    let error = client
        .execute_plan::<Text<String>>(disabled_retry_plan())
        .await
        .expect_err("503 remains terminal when retries are disabled");

    assert!(matches!(
        error,
        crate::prelude::ApiClientError::HttpStatus { .. }
    ));
    assert_eq!(handle.recorded_len(), 1);
}
