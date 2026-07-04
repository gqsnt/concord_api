use super::helpers::*;
use quote::quote;

#[test]
fn generated_auth_plan_uses_resolved_requirements() {
    let out = expanded(quote! {
        client AuthPlanApi {
            base "https://example.com"
            secret token: String
            credential key = api_key(secret.token)
        }

        GET Search
            path ["search"]
            auth header "X-Api-Key" = key
            -> Json<String>
    });

    assert_contains_all(
        &out,
        &[
            "::concord_core::advanced::AuthRequirement",
            "::concord_core::advanced::AuthPlacement::Header",
            "::concord_core::advanced::AuthUsageId::new(\"header\")",
            "AuthProvenance::new(\"endpoint\")",
            "step_id: ::core::option::Option::Some(\"Search:0:key\")",
        ],
    );
    assert!(
        !out.contains("emit_auth_usage_id"),
        "generated code should not call old auth-use helpers"
    );
    assert!(
        !out.contains("endpoint_qualified_name(ep)"),
        "generated code should not reconstruct endpoint names"
    );
}

#[test]
fn generated_inherited_auth_step_ids_use_final_endpoint_target() {
    let out = expanded(quote! {
        client InheritedAuthStepsApi {
            base "https://example.com"
            secret client_token: String
            secret scope_token: String
            secret endpoint_token: String
            credential client_auth = api_key(secret.client_token)
            credential scope_auth = api_key(secret.scope_token)
            credential endpoint_auth = api_key(secret.endpoint_token)
            auth header "X-Read-Token" = client_auth
        }

        scope users {
            path ["users"]
            auth query "read_key" = scope_auth

            GET Me
                path ["me"]
                auth header "X-Endpoint-Token" = endpoint_auth
                -> Json<()>
        }
    });

    assert_contains_all(
        &out,
        &[
            "users::Me:0:client_auth",
            "users::Me:1:scope_auth",
            "users::Me:2:endpoint_auth",
        ],
    );
}

#[test]
fn generated_oauth2_client_credentials_provider_is_typed() {
    let out = expanded(quote! {
        client OAuthProviderApi {
            base "https://example.com"
            secret client_id: String
            secret client_secret: String

            credential oauth = oauth2_client {
                token_url: "https://auth.example.com/oauth/token",
                client_id: secret.client_id,
                client_secret: secret.client_secret,
                scope: "read:me",
            }
        }

        GET OAuthMe
            path ["oauth-me"]
            auth bearer oauth
            -> Json<String>
    });

    assert_contains_all(
        &out,
        &[
            "::concord_core::advanced::OAuth2ClientCredentialsProvider::from_validated_token_url",
            ".scope(\"read:me\")",
            "CredentialId::new(\"OAuthProviderApi\",\"oauth\")",
        ],
    );
}

#[test]
fn generated_auth_session_contains_auth_state_and_acquire_sugar() {
    let out = expanded(quote! {
        client SnapshotAuth {
            base "https://example.com"
            secret upstream_key: String

            credential upstream = api_key(secret.upstream_key)
            credential session = endpoint auth_api::LoginForSession
        }

        scope auth_api {
            POST LoginForSession(body: Json<LoginRequest>)
                path ["login"]
                auth header "X-Upstream-Key" = upstream
                -> Json<AccessToken>
        }

        scope protected {
            auth bearer session

            GET Me
                as me
                path ["me"]
                -> Json<User>
        }
    });

    assert_contains_all(
        &out,
        &[
            "pub struct SnapshotAuthAuthState",
            "pub fn session (& self) -> SnapshotAuthSessionAuth",
            "pub fn auth_state (& self) -> SnapshotAuthAuth",
            "pub async fn acquire < R >",
            "pub async fn set (& self , value : AccessToken ,) -> :: core :: result :: Result < () , :: concord_core :: advanced :: AuthError >",
            "pub async fn clear (& self) -> :: core :: result :: Result < () , :: concord_core :: advanced :: AuthError >",
            "pub async fn is_set (& self) -> :: core :: result :: Result < bool , :: concord_core :: advanced :: AuthError >",
            "pub async fn acquire_auth_session",
            "pub async fn set_auth_session_value (& self , value : AccessToken ,) -> :: core :: result :: Result < () , :: concord_core :: advanced :: AuthError >",
            "pub async fn clear_auth_session (& self) -> :: core :: result :: Result < () , :: concord_core :: advanced :: AuthError >",
            "pub async fn has_auth_session (& self) -> :: core :: result :: Result < bool , :: concord_core :: advanced :: AuthError >",
            "pub trait SnapshotAuthAcquireAsSessionExt",
            "fn acquire_as_session (self,) -> :: core :: pin :: Pin",
            ". with_missing_hint (\"client.acquire_auth_session(...)\")",
            ":: concord_core :: advanced :: AuthPlacement :: Bearer",
            ":: concord_core :: advanced :: AuthPlacement :: Header (\"X-Upstream-Key\")",
        ],
    );
}

#[test]
fn generated_endpoint_backed_auth_helpers_use_structured_endpoint_target() {
    let out = expanded(quote! {
        client EndpointAuthTarget {
            base "https://example.com"
            secret upstream_key: String
            credential upstream = api_key(secret.upstream_key)

            credential session = endpoint auth_api::LoginForSession
        }

        scope auth_api {
            POST LoginForSession(body: Json<LoginRequest>)
                path ["login"]
                auth header "X-Upstream-Key" = upstream
                -> Json<AccessToken>
        }
    });

    assert_contains_all(
        &out,
        &[
            "pub async fn acquire_auth_session",
            "pub trait EndpointAuthTargetAcquireAsSessionExt",
            "fn acquire_as_session (self,) -> :: core :: pin :: Pin",
            "endpoints :: auth_api :: LoginForSession",
            ". with_missing_hint (\"client.acquire_auth_session(...)\")",
        ],
    );
}
