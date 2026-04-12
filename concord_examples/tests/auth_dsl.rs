use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;
use http::header::{AUTHORIZATION, CONTENT_TYPE};

#[derive(Clone)]
pub struct DslStaticTokenProvider;

impl<Cx: ClientContext> CredentialProvider<Cx> for DslStaticTokenProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("dsl", "static-token")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async { Ok(AccessToken::new("macro-token")) })
    }
}

#[derive(Clone, Copy)]
pub struct DslFormattingBearerAuth {
    prefix: &'static str,
}

impl DslFormattingBearerAuth {
    fn new(prefix: &'static str) -> Self {
        Self { prefix }
    }
}

impl<Cx, E> AuthUsage<Cx, E, AccessToken> for DslFormattingBearerAuth
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
{
    fn name(&self) -> AuthUsageId {
        AuthUsageId::new("dsl-formatting-bearer")
    }

    fn apply(
        &self,
        ctx: AuthApplyContext<'_, Cx, E>,
        material: &AccessToken,
    ) -> Result<AuthIdentity, ApiClientError> {
        let value = format!("Bearer {}{}", self.prefix, material.token.expose());
        let value =
            http::HeaderValue::from_str(&value).map_err(|_| ApiClientError::InvalidParam {
                ctx: ctx.error_context(),
                param: "authorization formatted bearer token",
            })?;
        ctx.request.headers.insert(AUTHORIZATION, value);
        Ok(material.safe_identity())
    }
}

#[derive(Clone)]
pub struct DslLoginProvider {
    username: SecretString,
    password: SecretString,
}

impl DslLoginProvider {
    fn new(username: SecretString, password: SecretString) -> Self {
        Self { username, password }
    }
}

#[derive(serde::Deserialize)]
struct DslLoginTokenResponse {
    access_token: String,
}

impl<Cx: ClientContext> CredentialProvider<Cx> for DslLoginProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("dsl", "login-token")
    }

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let mut headers = http::HeaderMap::new();
            headers.insert(
                CONTENT_TYPE,
                http::HeaderValue::from_static("application/x-www-form-urlencoded"),
            );

            let body = {
                let mut form = url::form_urlencoded::Serializer::new(String::new());
                form.append_pair("username", self.username.expose());
                form.append_pair("password", self.password.expose());
                form.finish()
            };

            let url = format!("{}://{}/login", Cx::SCHEME, Cx::DOMAIN)
                .parse()
                .expect("valid login url");

            let resp = ctx
                .executor
                .send(AuthHttpRequest {
                    method: http::Method::POST,
                    url,
                    headers,
                    body: Some(bytes::Bytes::from(body)),
                    mode: AuthMode::SkipAuth,
                    policy: AuthInternalPolicy::default(),
                })
                .await?;

            if !resp.status.is_success() {
                return Err(AuthError::new(
                    AuthErrorKind::AcquireFailed,
                    format!("login returned {}", resp.status),
                ));
            }

            let token: DslLoginTokenResponse =
                serde_json::from_slice(&resp.body).map_err(|e| {
                    AuthError::new(
                        AuthErrorKind::AcquireFailed,
                        format!("login response decode failed: {e}"),
                    )
                })?;

            Ok(AccessToken::new(token.access_token))
        })
    }
}

#[tokio::test]
async fn scope_header_auth_uses_api_key_credential_and_secret_setter_rebuilds_state() {
    api! {
        client ApiDslHeader {
            scheme: https,
            host: "example.com",
            secret {
                api_key: String
            }
            auth {
                credential api_key: ApiKey(secret.api_key)
            }
        }

        scope protected {
            use_auth HeaderAuth("X-Api-Key", api_key)
            path["v1"]

            GET Ping {
                -> Json<()>;
            }
        }
    }

    use api_dsl_header::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();

    let mut api = ApiDslHeader::new_with_transport("tok1".to_string(), transport);
    api.request(endpoints::Ping::new()).execute().await.unwrap();
    api.set_api_key("tok2");
    api.request(endpoints::Ping::new()).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);
    assert_request(&reqs[0])
        .path("/v1")
        .header("x-api-key", "tok1");
    assert_request(&reqs[1])
        .path("/v1")
        .header("x-api-key", "tok2");

    h.finish();
}

#[tokio::test]
async fn custom_bearer_usage_macro_formats_token_before_applying_it() {
    api! {
        client ApiDslCustomBearer {
            scheme: https,
            host: "example.com",
            auth {
                credential token: Custom<DslStaticTokenProvider>(DslStaticTokenProvider)
            }
        }

        GET Ping {
            use_auth Custom<DslFormattingBearerAuth>(DslFormattingBearerAuth::new("tenant-a:"), token)
            -> Json<()>;
        }
    }

    use api_dsl_custom_bearer::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();
    let api = ApiDslCustomBearer::new_with_transport(transport);

    api.request(endpoints::Ping::new()).execute().await.unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).header(AUTHORIZATION, "Bearer tenant-a:macro-token");

    h.finish();
}

#[tokio::test]
async fn custom_provider_macro_can_login_with_internal_request_against_current_api() {
    api! {
        client ApiDslCustomLogin {
            scheme: https,
            host: "example.com",
            secret {
                username: String,
                password: String
            }
            auth {
                credential session: Custom<DslLoginProvider>(
                    DslLoginProvider::new(secret.username.clone(), secret.password.clone())
                )
            }
        }

        GET Ping {
            use_auth BearerAuth(session)
            -> Json<()>;
        }
    }

    use api_dsl_custom_login::*;

    let token_body = json_bytes(&serde_json::json!({
        "access_token": "login-token"
    }));
    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(token_body),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();
    let api = ApiDslCustomLogin::new_with_transport(
        "pw".to_string(),
        "alice".to_string(),
        transport,
    );

    api.request(endpoints::Ping::new()).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);
    assert_request(&reqs[0])
        .host("example.com")
        .path("/login")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded");
    assert_eq!(
        std::str::from_utf8(reqs[0].body.as_ref().unwrap()).unwrap(),
        "username=alice&password=pw"
    );
    assert_request(&reqs[1]).header(AUTHORIZATION, "Bearer login-token");

    h.finish();
}

#[tokio::test]
async fn endpoint_query_auth_uses_api_key_credential() {
    api! {
        client ApiDslQuery {
            scheme: https,
            host: "example.com",
            secret {
                api_key: String
            }
            auth {
                credential api_key: ApiKey(secret.api_key)
            }
        }

        GET Ping {
            use_auth QueryAuth("api_key", api_key)
            -> Json<()>;
        }
    }

    use api_dsl_query::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();
    let api = ApiDslQuery::new_with_transport("query-token".to_string(), transport);

    api.request(endpoints::Ping::new()).execute().await.unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).query_has("api_key", "query-token");

    h.finish();
}

#[tokio::test]
async fn oauth2_client_credentials_dsl_uses_internal_token_request_then_bearer_auth() {
    api! {
        client ApiDslOAuth {
            scheme: https,
            host: "example.com",
            secret {
                client_id: String,
                client_secret: String
            }
            auth {
                credential token: OAuth2ClientCredentials {
                    token_url: "https://auth.example.com/token",
                    client_id: secret.client_id,
                    client_secret: secret.client_secret,
                    scope: "read"
                }
            }
        }

        GET Ping {
            use_auth BearerAuth(token)
            -> Json<()>;
        }
    }

    use api_dsl_o_auth::*;

    let token_body = json_bytes(&serde_json::json!({
        "access_token": "oauth-token",
        "token_type": "Bearer",
        "expires_in": 3600,
        "scope": "read"
    }));
    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(token_body),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();
    let api = ApiDslOAuth::new_with_transport(
        "client".to_string(),
        "secret".to_string(),
        transport,
    );

    api.request(endpoints::Ping::new()).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);
    assert_request(&reqs[0])
        .host("auth.example.com")
        .path("/token")
        .header(AUTHORIZATION, "Basic Y2xpZW50OnNlY3JldA==")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded");
    assert_eq!(
        std::str::from_utf8(reqs[0].body.as_ref().unwrap()).unwrap(),
        "grant_type=client_credentials&scope=read"
    );
    assert_request(&reqs[1]).header(AUTHORIZATION, "Bearer oauth-token");

    h.finish();
}
