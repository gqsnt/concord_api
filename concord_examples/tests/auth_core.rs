use bytes::Bytes;
use concord_core::advanced::*;
use concord_core::prelude::*;
use http::{HeaderMap, Method, StatusCode};

#[derive(Clone)]
struct TestCx;

#[derive(Clone, Default)]
struct TestVars;

#[derive(Clone, Default)]
struct TestAuthVars;

#[derive(Clone)]
struct TestAuthState {
    token: std::sync::Arc<CredentialSlot<TestCx, StaticBearerProvider>>,
}

impl ClientContext for TestCx {
    type Vars = TestVars;
    type AuthVars = TestAuthVars;
    type AuthState = TestAuthState;
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_: &Self::Vars, _: &Self::AuthVars) -> Self::AuthState {
        Self::AuthState {
            token: std::sync::Arc::new(CredentialSlot::new(StaticBearerProvider::new(
                CredentialId::new("test", "token"),
                AccessToken::new("secret"),
            ))),
        }
    }

    fn prepare_auth_requirement<'a>(
        requirement: &'a AuthRequirement,
        request: &'a mut concord_core::transport::BuiltRequest,
        vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        auth_state: &'a Self::AuthState,
        executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> concord_core::advanced::AuthFuture<'a, Result<AuthAppliedCredential, AuthError>> {
        Box::pin(async move {
            let credential_ctx = CredentialContext {
                vars,
                auth,
                auth_state,
                executor,
                credential_id: requirement.credential.id.clone(),
                reason: CredentialRefreshReason::Missing,
            };
            let lease = auth_state
                .token
                .get_or_refresh(
                    credential_ctx,
                    concord_core::advanced::AuthStepPolicy::default(),
                )
                .await?;
            let identity = concord_core::advanced::apply_secret_credential(
                request,
                requirement,
                &lease.value,
            )?;
            Ok(AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(lease.generation),
                identity,
                provenance: requirement.provenance.clone(),
            })
        })
    }
}

struct Ping;

impl Endpoint<TestCx> for Ping {
    type Response = String;

    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, TestCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        fn decode(
            resp: concord_core::transport::BuiltResponse,
            _ctx: ErrorContext,
        ) -> Result<Box<dyn std::any::Any + Send>, ApiClientError> {
            let value = String::from_utf8(resp.body.to_vec()).unwrap();
            Ok(Box::new(DecodedResponse {
                meta: resp.meta,
                url: resp.url,
                status: resp.status,
                headers: resp.headers,
                value,
            }))
        }

        let route = concord_core::internal::ResolvedRoute::new(
            http::uri::Scheme::HTTPS,
            "example.com",
            "/ping",
        );

        Ok(concord_core::internal::RequestPlan {
            endpoint: concord_core::internal::EndpointPlan {
                meta: concord_core::internal::EndpointMeta {
                    name: "Ping",
                    method: Method::GET,
                    idempotent: true,
                    facade_path: &[],
                },
                route,
                policy: ResolvedPolicy {
                    headers: HeaderMap::new(),
                    query: Vec::new(),
                    timeout: None,
                    auth: AuthPlan {
                        requirements: vec![AuthRequirement {
                            credential: CredentialRef {
                                id: CredentialId::new("test", "token"),
                            },
                            placement: AuthPlacement::Bearer,
                            usage_id: concord_core::advanced::AuthUsageId::new("bearer"),
                            step_id: Some("ping:token"),
                            provenance: concord_core::advanced::AuthProvenance::new("test"),
                            challenge: AuthChallengePolicy::Default,
                        }],
                    },
                    cache: CacheSetting::Off,
                    retry: RetrySetting::Off,
                    rate_limit: RateLimitPlan::new(),
                },
                body: concord_core::internal::BodyPlan::None,
                response: concord_core::internal::ResponsePlan {
                    accept: "text/plain",
                    no_content: false,
                    format: concord_core::internal::Format::Text,
                    decode,
                },
                pagination: None,
            },
            args: concord_core::internal::RequestArgs::default(),
            overrides: concord_core::internal::RequestOverrides::default(),
        })
    }
}

#[derive(Clone)]
struct CapturingTransport;

impl concord_core::advanced::Transport for CapturingTransport {
    fn send(
        &self,
        req: concord_core::transport::BuiltRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        concord_core::transport::TransportResponse,
                        concord_core::transport::TransportError,
                    >,
                > + Send,
        >,
    > {
        Box::pin(async move {
            assert_eq!(
                req.headers.get(http::header::AUTHORIZATION).unwrap(),
                "Bearer secret"
            );
            Ok(concord_core::transport::TransportResponse {
                meta: req.meta,
                url: req.url,
                status: StatusCode::OK,
                headers: HeaderMap::new(),
                content_length: Some(2),
                rate_limit: RateLimitPlan::new(),
                body: Box::new(StaticBody {
                    chunk: Some(Bytes::from_static(b"ok")),
                }),
            })
        })
    }
}

struct StaticBody {
    chunk: Option<Bytes>,
}

impl concord_core::transport::TransportBody for StaticBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Option<Bytes>, concord_core::transport::TransportError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move { Ok(self.chunk.take()) })
    }
}

#[tokio::test]
async fn auth_plan_applies_bearer_credential() {
    let client = ApiClient::<TestCx, CapturingTransport>::with_transport(
        TestVars,
        TestAuthVars,
        CapturingTransport,
    );
    let value = client.request(Ping).execute().await.unwrap();
    assert_eq!(value, "ok");
}
