use super::helpers::{
    analyze_err, analyze_ok, assert_auth_error_contains, auth_for_endpoint, credential_by_name,
};
use crate::sema::{AuthCredentialKindIr, AuthPlacementIr};

#[test]
fn auth_uses_resolve_all_current_placements() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret api_key_secret: String
                secret bearer_token_secret: String
                secret basic_user: String
                secret basic_password: String

                credential api_key = api_key(secret.api_key_secret)
                credential bearer_token = bearer(secret.bearer_token_secret)
                credential basic_auth = basic(secret.basic_user, secret.basic_password)
                credential cert_session = endpoint auth_api::GetCertificate
            }

            scope auth_api {
                path ["auth"]

                GET GetCertificate
                    path ["certificate"]
                    -> Json<ClientCertificate>
            }

            GET ShowPrimary
                path ["show-primary"]
                auth bearer bearer_token
                auth header "X-Api-Key" = api_key
                auth query "api_key" = api_key
                auth certificate cert_session
                -> Json<()>

            GET ShowBasic
                path ["show-basic"]
                auth basic basic_auth
                -> Json<()>
        }
        "#,
    );

    let primary = auth_for_endpoint(&api, "ShowPrimary");
    assert_eq!(primary.len(), 4);
    assert!(matches!(primary[0].placement, AuthPlacementIr::Bearer));
    assert!(matches!(
        primary[1].placement,
        AuthPlacementIr::Header { ref name } if name.value() == "X-Api-Key"
    ));
    assert!(matches!(
        primary[2].placement,
        AuthPlacementIr::Query { ref key } if key.value() == "api_key"
    ));
    assert!(matches!(primary[3].placement, AuthPlacementIr::Certificate));
    let basic = auth_for_endpoint(&api, "ShowBasic");
    assert!(matches!(basic, [req] if matches!(req.placement, AuthPlacementIr::Basic)));
    assert!(matches!(
        &credential_by_name(&api, "cert_session").kind,
        AuthCredentialKindIr::Endpoint { .. }
    ));
}

#[test]
fn auth_uses_reject_unknown_credential_with_available_names() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret token: String
                credential known = api_key(secret.token)
            }

            GET Show
                path ["show"]
                auth bearer missing
                -> Json<()>
        }
        "#,
    );

    assert_auth_error_contains(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret token: String
                credential known = api_key(secret.token)
            }

            GET Show
                path ["show"]
                auth bearer missing
                -> Json<()>
        }
        "#,
        "unknown auth credential `missing`",
    );
    assert!(err.to_string().contains("known"));
}

#[test]
fn auth_uses_reject_material_shape_mismatch() {
    for (label, source, expected) in [
        (
            "api_key as bearer",
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret token: String
                    credential api_key = api_key(secret.token)
                }

                GET Show
                    path ["show"]
                    auth bearer api_key
                    -> Json<()>
            }
            "#,
            "BearerAuth requires an access-token credential",
        ),
        (
            "basic as bearer",
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret user: String
                    secret pass: String
                    credential basic_auth = basic(secret.user, secret.pass)
                }

                GET Show
                    path ["show"]
                    auth bearer basic_auth
                    -> Json<()>
            }
            "#,
            "BearerAuth requires an access-token credential",
        ),
        (
            "bearer as basic",
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret token: String
                    credential bearer_auth = bearer(secret.token)
                }

                GET Show
                    path ["show"]
                    auth basic bearer_auth
                    -> Json<()>
            }
            "#,
            "BasicAuth requires BasicCredential material",
        ),
        (
            "api_key as certificate",
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret token: String
                    credential api_key = api_key(secret.token)
                }

                GET Show
                    path ["show"]
                    auth certificate api_key
                    -> Json<()>
            }
            "#,
            "CertificateAuth requires ClientCertificate material",
        ),
    ] {
        let err = analyze_err(source);
        assert!(
            err.to_string().contains(expected),
            "{label} should fail with `{expected}`, got `{err}`"
        );
    }
}
