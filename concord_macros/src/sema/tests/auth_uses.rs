use super::helpers::{analyze_err, analyze_ok, assert_auth_error_contains, auth_for_endpoint};
use crate::sema::{AuthChallengePolicyIr, AuthPlacementIr};

#[test]
fn auth_challenge_policy_is_resolved_and_defaults_to_unauthorized() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret token: String
                credential session = bearer(secret.token)
            }

            GET Default
                auth bearer session
                -> Json<()>

            GET Explicit
                auth bearer session challenge unauthorized_or_forbidden
                -> Json<()>

            GET Never
                auth bearer session challenge never_recover
                -> Json<()>
        }
        "#,
    );

    assert_eq!(
        auth_for_endpoint(&api, "Default")[0].challenge,
        AuthChallengePolicyIr::Unauthorized
    );
    assert_eq!(
        auth_for_endpoint(&api, "Explicit")[0].challenge,
        AuthChallengePolicyIr::UnauthorizedOrForbidden
    );
    assert_eq!(
        auth_for_endpoint(&api, "Never")[0].challenge,
        AuthChallengePolicyIr::NeverRecover
    );
}

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
            }

            GET ShowPrimary
                path ["show-primary"]
                auth bearer bearer_token
                auth header "X-Api-Key" = api_key
                auth query "api_key" = api_key
                -> Json<()>

            GET ShowBasic
                path ["show-basic"]
                auth basic basic_auth
                -> Json<()>
        }
        "#,
    );

    let primary = auth_for_endpoint(&api, "ShowPrimary");
    assert_eq!(primary.len(), 3);
    assert!(matches!(primary[0].placement, AuthPlacementIr::Bearer));
    assert!(matches!(
        primary[1].placement,
        AuthPlacementIr::Header { ref name } if name.value() == "X-Api-Key"
    ));
    assert!(matches!(
        primary[2].placement,
        AuthPlacementIr::Query { ref key } if key.value() == "api_key"
    ));
    let basic = auth_for_endpoint(&api, "ShowBasic");
    assert!(matches!(basic, [req] if matches!(req.placement, AuthPlacementIr::Basic)));
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
    ] {
        let err = analyze_err(source);
        assert!(
            err.to_string().contains(expected),
            "{label} should fail with `{expected}`, got `{err}`"
        );
    }
}
