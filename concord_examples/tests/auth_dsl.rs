use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;
use http::header::{AUTHORIZATION, CONTENT_TYPE, WWW_AUTHENTICATE};

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

            let token: DslLoginTokenResponse = serde_json::from_slice(&resp.body).map_err(|e| {
                AuthError::new(
                    AuthErrorKind::AcquireFailed,
                    format!("login response decode failed: {e}"),
                )
            })?;

            Ok(AccessToken::new(token.access_token))
        })
    }
}

#[derive(Clone)]
pub struct DslCapturedApiKeyProvider {
    token: SecretString,
}

impl DslCapturedApiKeyProvider {
    fn new(token: SecretString) -> Self {
        Self { token }
    }
}

impl<Cx: ClientContext> CredentialProvider<Cx> for DslCapturedApiKeyProvider {
    type Credential = ApiKey;

    fn id(&self) -> CredentialId {
        CredentialId::new("dsl", "captured-api-key")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move { Ok(ApiKey::new(self.token.clone())) })
    }
}

#[derive(Clone)]
pub struct DslCertificateProvider;

impl<Cx: ClientContext> CredentialProvider<Cx> for DslCertificateProvider {
    type Credential = ClientCertificate;

    fn id(&self) -> CredentialId {
        CredentialId::new("dsl", "certificate")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move { Ok(ClientCertificate::new("dsl-cert-identity")) })
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DslSessionLoginRequest {
    username: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DslSessionLoginResponse {
    access_token: String,
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
            path["api"]

            GET Ping
            -> Json<()>
            {
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
    api.request(endpoints::protected::Ping::new())
        .execute()
        .await
        .unwrap();
    api.set_api_key("tok2");
    api.request(endpoints::protected::Ping::new())
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);
    assert_request(&reqs[0])
        .path("/api")
        .header("x-api-key", "tok1");
    assert_request(&reqs[1])
        .path("/api")
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

        GET Ping
        -> Json<()>
        {
            use_auth Custom<DslFormattingBearerAuth>(DslFormattingBearerAuth::new("tenant-a:"), token)
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
async fn auth_list_applies_all_steps_in_order() {
    api! {
        client ApiDslAuthList {
            scheme: https,
            host: "example.com",
            secret {
                api_key: String
            }
            auth {
                credential token: Custom<DslStaticTokenProvider>(DslStaticTokenProvider)
                credential api_key: ApiKey(secret.api_key)
            }
        }

        GET Ping
        -> Json<()>
        {
            use_auth [
                BearerAuth(token),
                HeaderAuth("X-Api-Key", api_key)
            ]
        }
    }

    use api_dsl_auth_list::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();
    let api = ApiDslAuthList::new_with_transport("list-key".to_string(), transport);

    api.request(endpoints::Ping::new()).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 1);
    assert_request(&reqs[0])
        .header(AUTHORIZATION, "Bearer macro-token")
        .header("x-api-key", "list-key");
    h.finish();
}

#[tokio::test]
async fn auth_one_of_falls_back_to_next_usage_after_unauthorized() {
    api! {
        client ApiDslOneOf {
            scheme: https,
            host: "example.com",
            secret {
                fallback_key: String
            }
            auth {
                credential token: Custom<DslStaticTokenProvider>(DslStaticTokenProvider)
                credential fallback: ApiKey(secret.fallback_key)
            }
        }

        GET Ping
        -> Json<()>
        {
            use_auth one_of [
                BearerAuth(token),
                HeaderAuth("X-Fallback-Key", fallback)
            ]
        }
    }

    use api_dsl_one_of::*;

    let unauthorized = MockReply::status(http::StatusCode::UNAUTHORIZED).with_header(
        http::header::WWW_AUTHENTICATE,
        http::HeaderValue::from_static("Bearer error=\"invalid_token\""),
    );
    let (transport, h) = mock()
        .replies([unauthorized, MockReply::ok_json(json_bytes(&()))])
        .build();
    let api = ApiDslOneOf::new_with_transport("fallback-secret".to_string(), transport);

    api.request(endpoints::Ping::new()).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);
    assert_request(&reqs[0])
        .header(AUTHORIZATION, "Bearer macro-token")
        .header_absent("x-fallback-key");
    assert_request(&reqs[1])
        .header("x-fallback-key", "fallback-secret")
        .header_absent(AUTHORIZATION);
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

        GET Ping
        -> Json<()>
        {
            use_auth BearerAuth(session)
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
    let api =
        ApiDslCustomLogin::new_with_transport("pw".to_string(), "alice".to_string(), transport);

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

        GET Ping
        -> Json<()>
        {
            use_auth QueryAuth("api_key", api_key)
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

        GET Ping
        -> Json<()>
        {
            use_auth BearerAuth(token)
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
    let api =
        ApiDslOAuth::new_with_transport("client".to_string(), "secret".to_string(), transport);

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

#[tokio::test]
async fn secret_setter_rebuild_updates_all_client_clones() {
    api! {
        client ApiDslCloneRebuild {
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
            GET Ping
            -> Json<()>
            {
            }
        }
    }

    use api_dsl_clone_rebuild::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();
    let mut api = ApiDslCloneRebuild::new_with_transport("tok1".to_string(), transport);
    let clone = api.clone();

    api.request(endpoints::protected::Ping::new())
        .execute()
        .await
        .unwrap();
    clone
        .request(endpoints::protected::Ping::new())
        .execute()
        .await
        .unwrap();
    api.set_api_key("tok2");
    clone
        .request(endpoints::protected::Ping::new())
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 3);
    assert_request(&reqs[0]).header("x-api-key", "tok1");
    assert_request(&reqs[1]).header("x-api-key", "tok1");
    assert_request(&reqs[2]).header("x-api-key", "tok2");
    h.finish();
}

#[tokio::test]
async fn custom_provider_secret_update_rebuilds_state() {
    api! {
        client ApiDslCustomRebuild {
            scheme: https,
            host: "example.com",
            secret {
                api_key: String
            }
            auth {
                credential captured: Custom<DslCapturedApiKeyProvider>(
                    DslCapturedApiKeyProvider::new(secret.api_key.clone())
                )
            }
        }

        GET Ping
        -> Json<()>
        {
            use_auth HeaderAuth("X-Api-Key", captured)
        }
    }

    use api_dsl_custom_rebuild::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();
    let mut api = ApiDslCustomRebuild::new_with_transport("tok1".to_string(), transport);

    api.request(endpoints::Ping::new()).execute().await.unwrap();
    api.set_api_key("tok2");
    api.request(endpoints::Ping::new()).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);
    assert_request(&reqs[0]).header("x-api-key", "tok1");
    assert_request(&reqs[1]).header("x-api-key", "tok2");
    h.finish();
}

#[tokio::test]
async fn endpoint_backed_manual_credential_requires_explicit_acquire_and_shares_lifecycle() {
    api! {
        client ApiDslEndpointManual {
            scheme: https,
            host: "example.com",
            auth {
                credential session: Endpoint(auth::LoginForSession)
            }
        }

        scope auth {
            POST LoginForSession(body: Json<DslSessionLoginRequest>)
            -> Json<DslSessionLoginResponse> | AccessToken => {
                AccessToken::new(r.access_token)
            }
            {
                path["login"]
            }
        }

        GET Protected
        -> Json<()>
        {
            path["protected"]
            use_auth BearerAuth(session)
        }
    }

    use api_dsl_endpoint_manual::*;

    let token_body = json_bytes(&DslSessionLoginResponse {
        access_token: "session-token".to_string(),
    });
    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(token_body),
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();
    let api = ApiDslEndpointManual::new_with_transport(transport);

    let err = api
        .request(endpoints::Protected::new())
        .execute()
        .await
        .expect_err("manual credential must fail before acquisition");
    match err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::MissingCredential);
            assert!(source.message.contains("acquire_auth_session"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(
        h.recorded().len(),
        0,
        "missing credential should fail before transport"
    );

    api.acquire_auth_session(endpoints::auth::LoginForSession::new(
        DslSessionLoginRequest {
            username: "alice".to_string(),
        },
    ))
    .await
    .unwrap();
    assert!(api.has_auth_session().await);

    api.request(endpoints::Protected::new())
        .execute()
        .await
        .unwrap();
    let clone = api.clone();
    clone
        .request(endpoints::Protected::new())
        .execute()
        .await
        .unwrap();
    assert!(clone.has_auth_session().await);

    api.clear_auth_session().await;
    assert!(!clone.has_auth_session().await);

    let err = clone
        .request(endpoints::Protected::new())
        .execute()
        .await
        .expect_err("cleared credential must fail");
    match err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::MissingCredential);
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 3);
    assert_request(&reqs[0]).path("/login");
    assert_eq!(
        std::str::from_utf8(reqs[0].body.as_ref().expect("login body")).unwrap(),
        "{\"username\":\"alice\"}"
    );
    assert_request(&reqs[1]).header(AUTHORIZATION, "Bearer session-token");
    assert_request(&reqs[2]).header(AUTHORIZATION, "Bearer session-token");
    h.finish();
}

#[tokio::test]
async fn endpoint_backed_manual_credential_401_invalidates_without_auto_retry_and_login_can_use_auth()
 {
    api! {
        client ApiDslEndpointManualInvalidation {
            scheme: https,
            host: "example.com",
            secret {
                upstream_key: String
            }
            auth {
                credential upstream: ApiKey(secret.upstream_key)
                credential session: Endpoint(auth::LoginForSession)
            }
        }

        scope auth {
            POST LoginForSession
            -> Json<DslSessionLoginResponse> | AccessToken => {
                AccessToken::new(r.access_token)
            }
            {
                path["login"]
                use_auth HeaderAuth("X-Upstream-Key", upstream)
            }
        }

        GET Protected
        -> Json<()>
        {
            path["protected"]
            use_auth BearerAuth(session)
        }
    }

    use api_dsl_endpoint_manual_invalidation::*;

    let unauthorized = MockReply::status(http::StatusCode::UNAUTHORIZED).with_header(
        WWW_AUTHENTICATE,
        http::HeaderValue::from_static("Bearer error=\"invalid_token\""),
    );
    let token_body = json_bytes(&DslSessionLoginResponse {
        access_token: "session-token".to_string(),
    });
    let (transport, h) = mock()
        .replies([MockReply::ok_json(token_body), unauthorized])
        .build();
    let api = ApiDslEndpointManualInvalidation::new_with_transport("up-key".to_string(), transport);

    api.acquire_auth_session(endpoints::auth::LoginForSession::new())
        .await
        .unwrap();
    assert!(api.has_auth_session().await);

    let err = api
        .request(endpoints::Protected::new())
        .execute()
        .await
        .expect_err("401 should bubble without auth auto-retry for manual endpoint credentials");
    match err {
        ApiClientError::HttpStatus { status, .. } => {
            assert_eq!(status, http::StatusCode::UNAUTHORIZED)
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert!(
        !api.has_auth_session().await,
        "401 rejection should invalidate manual credential"
    );

    let err = api
        .request(endpoints::Protected::new())
        .execute()
        .await
        .expect_err("after invalidation, missing credential should fail before send");
    match err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::MissingCredential);
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2, "no auth retry should be attempted");
    assert_request(&reqs[0]).header("x-upstream-key", "up-key");
    assert_request(&reqs[1]).header(AUTHORIZATION, "Bearer session-token");
    h.finish();
}

#[tokio::test]
async fn certificate_auth_macro_usage_is_supported() {
    api! {
        client ApiDslCertificate {
            scheme: https,
            host: "example.com",
            auth {
                credential cert: Custom<DslCertificateProvider>(DslCertificateProvider)
            }
        }

        GET Ping
        -> Json<()>
        {
            use_auth CertificateAuth(cert)
        }
    }

    use api_dsl_certificate::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();
    let api = ApiDslCertificate::new_with_transport(transport);
    api.request(endpoints::Ping::new()).execute().await.unwrap();
    h.finish();
}
