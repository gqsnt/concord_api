use super::helpers::{
    analyze_err, analyze_ok, assert_error_contains, credential_by_name, ty_string,
};
use crate::sema::{AuthCredentialKindIr, AuthMaterialShapeIr};

#[test]
fn endpoint_backed_credentials_resolve_target_and_output_shape() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                credential access_token = endpoint auth_api::LoginForToken
                credential api_key = endpoint auth_api::LoginForApiKey
                credential basic_credential = endpoint auth_api::LoginForBasic
                credential unknown_shape = endpoint auth_api::LoginForUnknown
            }

            scope auth_api {
                path ["auth"]

                POST LoginForToken
                    path ["token"]
                    -> Json<AccessToken>

                POST LoginForApiKey
                    path ["api-key"]
                    -> Json<ApiKey>

                POST LoginForBasic
                    path ["basic"]
                    -> Json<BasicCredential>

                GET LoginForUnknown
                    path ["unknown"]
                    -> Json<CustomToken>
            }
        }
        "#,
    );

    match &credential_by_name(&api, "access_token").kind {
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
            assert_eq!(target.endpoint.to_string(), "LoginForToken");
            assert_eq!(ty_string(output_ty), "AccessToken");
            assert_eq!(*material_shape, AuthMaterialShapeIr::AccessToken);
        }
        other => panic!("expected endpoint-backed credential, got {other:?}"),
    }

    match &credential_by_name(&api, "api_key").kind {
        AuthCredentialKindIr::Endpoint {
            target,
            output_ty,
            material_shape,
        } => {
            assert_eq!(target.endpoint.to_string(), "LoginForApiKey");
            assert_eq!(ty_string(output_ty), "ApiKey");
            assert_eq!(*material_shape, AuthMaterialShapeIr::SecretValue);
        }
        other => panic!("expected endpoint-backed credential, got {other:?}"),
    }

    match &credential_by_name(&api, "basic_credential").kind {
        AuthCredentialKindIr::Endpoint {
            target,
            output_ty,
            material_shape,
        } => {
            assert_eq!(target.endpoint.to_string(), "LoginForBasic");
            assert_eq!(ty_string(output_ty), "BasicCredential");
            assert_eq!(*material_shape, AuthMaterialShapeIr::Basic);
        }
        other => panic!("expected endpoint-backed credential, got {other:?}"),
    }

    match &credential_by_name(&api, "unknown_shape").kind {
        AuthCredentialKindIr::Endpoint {
            target,
            output_ty,
            material_shape,
        } => {
            assert_eq!(target.endpoint.to_string(), "LoginForUnknown");
            assert_eq!(ty_string(output_ty), "CustomToken");
            assert_eq!(*material_shape, AuthMaterialShapeIr::Unknown);
        }
        other => panic!("expected endpoint-backed credential, got {other:?}"),
    }
}

#[test]
fn endpoint_backed_credentials_reject_unknown_target() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                credential session = endpoint auth_api::Missing
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

    assert_error_contains(
        &err,
        "unknown auth endpoint `auth_api::Missing` in credential source",
    );
}

#[test]
fn endpoint_backed_credential_rejects_self_use() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret upstream_key: String
                credential upstream = api_key(secret.upstream_key)
                credential session = endpoint auth_api::Login
            }

            scope auth_api {
                path ["auth"]

                POST Login
                    path ["login"]
                    auth header "X-Upstream-Key" = upstream
                    auth bearer session
                    -> Json<AccessToken>
            }
        }
        "#,
    );

    assert_error_contains(&err, "cannot acquire via endpoint");
    assert_error_contains(&err, "uses that credential");
}

#[test]
fn endpoint_backed_credential_rejects_inherited_client_auth_self_use() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret upstream_key: String
                credential upstream = api_key(secret.upstream_key)
                credential session = endpoint auth_api::Login
                auth bearer session
            }

            scope auth_api {
                path ["auth"]

                POST Login
                    path ["login"]
                    auth header "X-Upstream-Key" = upstream
                    -> Json<AccessToken>
            }
        }
        "#,
    );

    assert_error_contains(&err, "cannot acquire via endpoint");
    assert_error_contains(&err, "uses that credential");
}

#[test]
fn endpoint_backed_credential_rejects_inherited_behavior_auth_self_use() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret upstream_key: String
                credential upstream = api_key(secret.upstream_key)
                credential session = endpoint auth_api::Login

                profiles {
                    profile default_auth {
                        auth bearer session
                    }
                }

                default {
                    profile default_auth
                }
            }

            scope auth_api {
                path ["auth"]

                POST Login
                    path ["login"]
                    auth header "X-Upstream-Key" = upstream
                    -> Json<AccessToken>
            }
        }
        "#,
    );

    assert_error_contains(&err, "cannot acquire via endpoint");
    assert_error_contains(&err, "uses that credential");
}
