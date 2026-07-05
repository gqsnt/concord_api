use super::helpers::{analyze_err, analyze_ok, assert_error_contains, credential_by_name};
use crate::sema::{AuthCredentialKindIr, AuthMaterialShapeIr};

#[test]
fn auth_credentials_resolve_all_static_kinds() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret api_key: String
                secret token: String
                secret basic_user: String
                secret basic_password: String
                secret client_id: String
                secret client_secret: String

                credential key = api_key(secret.api_key)
                credential bearer_token = bearer(secret.token)
                credential basic_auth = basic(secret.basic_user, secret.basic_password)
                credential oauth = oauth2_client {
                    token_url: "https://auth.example.com/oauth/token",
                    client_id: secret.client_id,
                    client_secret: secret.client_secret,
                    scope: "read:me",
                }
                credential session = endpoint auth_api::Login
            }

            scope auth_api {
                path ["auth"]

                GET Login
                    path ["login"]
                    -> Json<AccessToken>
            }
        }
        "#,
    );

    match &credential_by_name(&api, "key").kind {
        AuthCredentialKindIr::ApiKey { secret } => assert_eq!(secret.to_string(), "api_key"),
        other => panic!("expected api key credential, got {other:?}"),
    }
    match &credential_by_name(&api, "bearer_token").kind {
        AuthCredentialKindIr::StaticBearer { secret } => assert_eq!(secret.to_string(), "token"),
        other => panic!("expected bearer credential, got {other:?}"),
    }
    match &credential_by_name(&api, "basic_auth").kind {
        AuthCredentialKindIr::Basic { username, password } => {
            assert_eq!(username.to_string(), "basic_user");
            assert_eq!(password.to_string(), "basic_password");
        }
        other => panic!("expected basic credential, got {other:?}"),
    }
    match &credential_by_name(&api, "oauth").kind {
        AuthCredentialKindIr::OAuth2ClientCredentials {
            token_url,
            client_id,
            client_secret,
            scope,
        } => {
            assert_eq!(token_url.value(), "https://auth.example.com/oauth/token");
            assert_eq!(client_id.to_string(), "client_id");
            assert_eq!(client_secret.to_string(), "client_secret");
            assert_eq!(
                scope.as_ref().map(|lit| lit.value()).as_deref(),
                Some("read:me")
            );
        }
        other => panic!("expected oauth credential, got {other:?}"),
    }
    match &credential_by_name(&api, "session").kind {
        AuthCredentialKindIr::Endpoint {
            target,
            output_ty,
            material_shape,
        } => {
            assert_eq!(
                target
                    .scope_modules
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                vec!["auth_api".to_string()]
            );
            assert_eq!(target.endpoint.to_string(), "Login");
            assert_eq!(
                quote::quote!(#output_ty).to_string().replace(' ', ""),
                "AccessToken"
            );
            assert_eq!(*material_shape, AuthMaterialShapeIr::AccessToken);
        }
        other => panic!("expected endpoint credential, got {other:?}"),
    }
}

#[test]
fn auth_credentials_reject_unknown_required_secret() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret token: String
                credential key = api_key(secret.missing)
            }
        }
        "#,
    );
    assert_error_contains(&err, "unknown secret `secret.missing` in auth credential");
}

#[test]
fn auth_credentials_reject_optional_secret_material() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret token?: String
                credential key = api_key(secret.token)
            }
        }
        "#,
    );
    assert_error_contains(
        &err,
        "auth credential secret `secret.token` must be required; optional secrets are not supported yet",
    );
}

#[test]
fn auth_credentials_reject_unsafe_oauth2_token_urls() {
    for (token_url, expected) in [
        (
            "http://auth.example.com/token",
            "OAuth2 token URL must be an https URL with a host, no userinfo, and no fragment",
        ),
        (
            "https://user:pass@auth.example.com/token",
            "OAuth2 token URL must be an https URL with a host, no userinfo, and no fragment",
        ),
        (
            "https://auth.example.com/token#fragment",
            "OAuth2 token URL must be an https URL with a host, no userinfo, and no fragment",
        ),
        (
            "file:///tmp/token",
            "OAuth2 token URL must be an https URL with a host, no userinfo, and no fragment",
        ),
        (
            "https:///token",
            "OAuth2 token URL must be an https URL with a host, no userinfo, and no fragment",
        ),
    ] {
        let err = analyze_err(&format!(
            r#"
            api! {{
                client Api {{
                    base "https://example.com"
                    secret client_id: String
                    secret client_secret: String
                    credential oauth = oauth2_client {{
                        token_url: "{token_url}",
                        client_id: secret.client_id,
                        client_secret: secret.client_secret,
                    }}
                }}
            }}
            "#
        ));
        assert_error_contains(&err, expected);
        assert!(!err.to_string().contains("client_secret"));
    }
}
