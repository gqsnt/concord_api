#[allow(dead_code)]
#[path = "current_core/common.rs"]
mod common;

mod query_auth_redaction {
    use super::common::{MockResponse, MockTransport, auth_policy, decode_string, request_plan};
    use bytes::Bytes;
    use concord_core::advanced::{
        AuthAppliedCredential, AuthDecision, AuthError, AuthPlacement, AuthRequirement,
        BuiltRequest, DebugSink, RequestMeta, apply_secret_credential,
    };
    use concord_core::internal::{ClientPlanContext, RequestPlan, ResolvedPolicy};
    use concord_core::prelude::{ApiClient, ApiClientError, ClientContext, DebugLevel, Endpoint};
    use http::{HeaderMap, Method, StatusCode};
    use std::sync::{Arc, Mutex};
    use tokio::sync::Mutex as TokioMutex;

    #[derive(Clone, Debug)]
    struct RedactionAuthVars {
        token: String,
    }

    #[derive(Clone)]
    struct RedactionCx;

    impl ClientContext for RedactionCx {
        type Vars = ();
        type AuthVars = RedactionAuthVars;
        type AuthState = ();
        const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
        const DOMAIN: &'static str = "example.com";

        fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

        fn prepare_auth_requirement<'a>(
            requirement: &'a AuthRequirement,
            request: &'a mut BuiltRequest,
            _vars: &'a Self::Vars,
            auth: &'a Self::AuthVars,
            _auth_state: &'a Self::AuthState,
            _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
            _meta: &'a RequestMeta,
        ) -> concord_core::advanced::AuthFuture<'a, Result<AuthAppliedCredential, AuthError>>
        {
            Box::pin(async move {
                let material = concord_core::prelude::ApiKey::new(auth.token.clone());
                let identity = apply_secret_credential(request, requirement, &material)?;
                Ok(AuthAppliedCredential {
                    credential_id: requirement.credential.id.clone(),
                    usage_id: requirement.usage_id.clone(),
                    step_id: requirement.step_id,
                    generation: Some(1),
                    identity,
                    provenance: requirement.provenance.clone(),
                })
            })
        }

        fn handle_auth_response<'a>(
            _requirement: &'a AuthRequirement,
            _applied: &'a AuthAppliedCredential,
            _vars: &'a Self::Vars,
            _auth: &'a Self::AuthVars,
            _auth_state: &'a Self::AuthState,
            _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
            _meta: &'a RequestMeta,
            _status: StatusCode,
            _headers: &'a HeaderMap,
        ) -> concord_core::advanced::AuthFuture<'a, Result<AuthDecision, AuthError>> {
            Box::pin(async { Ok(AuthDecision::Continue) })
        }
    }

    #[derive(Clone)]
    struct RedactionEndpoint {
        policy: ResolvedPolicy,
    }

    impl Endpoint<RedactionCx> for RedactionEndpoint {
        type Response = String;

        fn plan(
            &self,
            _ctx: &ClientPlanContext<'_, RedactionCx>,
        ) -> Result<RequestPlan, ApiClientError> {
            Ok(request_plan(
                "Redaction",
                Method::GET,
                "/text",
                self.policy.clone(),
                None,
                decode_string,
            ))
        }
    }

    #[derive(Default)]
    struct UrlDebugSink {
        events: Mutex<Vec<String>>,
    }

    impl UrlDebugSink {
        fn events(&self) -> Vec<String> {
            self.events.lock().expect("debug events lock").clone()
        }
    }

    impl DebugSink for UrlDebugSink {
        fn request_start(
            &self,
            _dbg: DebugLevel,
            _method: &Method,
            url: &str,
            _endpoint: &'static str,
            _page_index: u32,
        ) {
            self.events
                .lock()
                .expect("debug events lock")
                .push(format!("request:{url}"));
        }

        fn request_headers(&self, _dbg: DebugLevel, _headers: &HeaderMap) {}

        fn request_body(
            &self,
            _dbg: DebugLevel,
            _body: &Bytes,
            _format: concord_core::internal::Format,
            _max_chars: usize,
        ) {
        }

        fn response_status(&self, _dbg: DebugLevel, _status: StatusCode, url: &str, ok: bool) {
            self.events
                .lock()
                .expect("debug events lock")
                .push(format!("response:{ok}:{url}"));
        }

        fn response_headers(&self, _dbg: DebugLevel, _headers: &HeaderMap) {}

        fn response_body(
            &self,
            _dbg: DebugLevel,
            _body: &Bytes,
            _format: concord_core::internal::Format,
            _max_chars: usize,
        ) {
        }

        fn stale_fallback(
            &self,
            _dbg: DebugLevel,
            _method: &Method,
            url: &str,
            _endpoint: &'static str,
            _page_index: u32,
        ) {
            self.events
                .lock()
                .expect("debug events lock")
                .push(format!("stale:{url}"));
        }
    }

    fn policy_with_query_auth(key: &'static str) -> ResolvedPolicy {
        let mut policy = auth_policy(AuthPlacement::Query(key));
        policy.query.push(("page".to_string(), "2".to_string()));
        policy
    }

    async fn run_debug_request(
        policy: ResolvedPolicy,
        token: &str,
        status: StatusCode,
    ) -> Result<(Vec<String>, Vec<BuiltRequest>), ApiClientError> {
        let events = Arc::new(TokioMutex::new(Vec::new()));
        let transport = MockTransport::new(events, vec![MockResponse::text(status, "ok")]);
        let sent = transport.clone();
        let mut client = ApiClient::<RedactionCx, _>::with_transport(
            (),
            RedactionAuthVars {
                token: token.to_string(),
            },
            transport,
        );
        let debug = Arc::new(UrlDebugSink::default());
        client.set_debug_sink(debug.clone());

        let request = client
            .request(RedactionEndpoint { policy })
            .debug_level(DebugLevel::V)
            .execute_decoded()
            .await;

        if status.is_success() {
            request?;
        } else {
            let err = request.expect_err("HTTP error should be returned");
            assert!(err.to_string().contains(status.as_str()));
        }

        Ok((debug.events(), sent.requests().await))
    }

    #[tokio::test]
    async fn debug_url_redacts_query_auth_secret() -> Result<(), ApiClientError> {
        let (events, requests) = run_debug_request(
            policy_with_query_auth("api_key"),
            "real-secret",
            StatusCode::OK,
        )
        .await?;

        let debug_output = events.join("\n");
        assert!(!debug_output.contains("real-secret"));
        assert!(debug_output.contains("api_key=<redacted>"));
        assert!(
            requests[0].url.as_str().contains("real-secret"),
            "transport URL should retain the real query auth secret"
        );
        Ok(())
    }

    #[tokio::test]
    async fn debug_response_url_redacts_query_auth_secret() -> Result<(), ApiClientError> {
        let (events, requests) = run_debug_request(
            policy_with_query_auth("api_key"),
            "real-secret",
            StatusCode::INTERNAL_SERVER_ERROR,
        )
        .await?;

        let debug_output = events.join("\n");
        assert!(!debug_output.contains("real-secret"));
        assert!(
            debug_output
                .contains("response:false:https://example.com/text?page=2&api_key=<redacted>")
        );
        assert!(requests[0].url.as_str().contains("real-secret"));
        Ok(())
    }

    #[tokio::test]
    async fn debug_url_preserves_non_sensitive_query_values() -> Result<(), ApiClientError> {
        let (events, _) = run_debug_request(
            policy_with_query_auth("api_key"),
            "real-secret",
            StatusCode::OK,
        )
        .await?;

        let debug_output = events.join("\n");
        assert!(debug_output.contains("page=2"));
        assert!(debug_output.contains("api_key=<redacted>"));
        assert!(!debug_output.contains("real-secret"));
        Ok(())
    }

    #[tokio::test]
    async fn debug_url_redacts_case_insensitive_sensitive_keys() -> Result<(), ApiClientError> {
        let (events, _) = run_debug_request(
            policy_with_query_auth("API_KEY"),
            "real-secret",
            StatusCode::OK,
        )
        .await?;

        let debug_output = events.join("\n");
        assert!(debug_output.contains("API_KEY=<redacted>"));
        assert!(!debug_output.contains("real-secret"));
        Ok(())
    }

    #[tokio::test]
    async fn debug_url_redacts_duplicate_sensitive_query_keys() -> Result<(), ApiClientError> {
        let mut policy = policy_with_query_auth("api_key");
        policy
            .query
            .push(("api_key".to_string(), "also-secret".to_string()));
        policy.query.push(("page".to_string(), "2".to_string()));

        let (events, requests) = run_debug_request(policy, "real-secret", StatusCode::OK).await?;

        let debug_output = events.join("\n");
        assert!(debug_output.matches("api_key=<redacted>").count() >= 2);
        assert!(debug_output.contains("page=2"));
        assert!(!debug_output.contains("real-secret"));
        assert!(!debug_output.contains("also-secret"));
        assert!(requests[0].url.as_str().contains("real-secret"));
        assert!(requests[0].url.as_str().contains("also-secret"));
        Ok(())
    }

    #[tokio::test]
    async fn debug_url_redacts_custom_query_auth_key() -> Result<(), ApiClientError> {
        let (events, requests) = run_debug_request(
            policy_with_query_auth("x-private-provider-key"),
            "real-secret",
            StatusCode::OK,
        )
        .await?;

        let debug_output = events.join("\n");
        assert!(debug_output.contains("x-private-provider-key=<redacted>"));
        assert!(debug_output.contains("page=2"));
        assert!(!debug_output.contains("real-secret"));
        assert!(requests[0].url.as_str().contains("real-secret"));
        Ok(())
    }
}
