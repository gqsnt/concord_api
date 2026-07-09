use super::helpers::{
    analyze_ok, auth_for_endpoint, auth_requirement_names, auth_requirement_provenance_labels,
    auth_requirement_step_ids, endpoint_by_name,
};

#[test]
fn auth_requirements_combine_in_client_scope_endpoint_order() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret api_key: String
                secret token: String
                secret scope_key: String
                credential client_key = api_key(secret.api_key)
                credential scope_key = api_key(secret.scope_key)
                credential token = bearer(secret.token)
                auth header "X-Client" = client_key
            }

            scope protected {
                path ["protected"]
                auth query "scope_key" = scope_key

                GET Me
                    path ["me"]
                    auth bearer token
                    -> Json<()>
            }
        }
        "#,
    );
    let auth = auth_for_endpoint(&api, "Me");

    assert_eq!(auth.len(), 3);
    assert_eq!(
        auth_requirement_names(auth),
        vec!["client_key", "scope_key", "token"]
    );
    assert_eq!(
        auth_requirement_step_ids(auth),
        vec![
            "protected::Me:0:client_key".to_string(),
            "protected::Me:1:scope_key".to_string(),
            "protected::Me:2:token".to_string(),
        ]
    );
    assert_eq!(
        auth_requirement_provenance_labels(auth),
        vec!["client", "scope:0", "endpoint"]
    );
}

#[test]
fn auth_inheritance_combines_client_scope_behavior_and_endpoint_in_order() {
    let api = analyze_ok(
        r#"
        api! {
            client AuthOrderApi {
                base "https://example.com"
                secret client_token: String
                secret scope_token: String
                secret endpoint_token: String
                secret direct_token: String

                credential client_auth = bearer(secret.client_token)
                credential scope_auth = api_key(secret.scope_token)
                credential endpoint_auth = api_key(secret.endpoint_token)
                credential direct_auth = api_key(secret.direct_token)

                profiles {
                    profile client_behavior {
                        auth bearer client_auth
                    }

                    profile scope_behavior {
                        auth header "X-Scope" = scope_auth
                    }

                    profile endpoint_behavior {
                        auth query "X-Endpoint" = endpoint_auth
                    }
                }

                default {
                    profile client_behavior
                }
            }

            scope protected {
                path ["protected"]
                profile scope_behavior

                GET Show
                    path ["show"]
                    profile endpoint_behavior
                    auth header "X-Direct" = direct_auth
                    -> Json<()>
            }
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "Show");

    assert_eq!(
        auth_requirement_names(&endpoint.policy.auth),
        vec![
            "client_auth".to_string(),
            "scope_auth".to_string(),
            "endpoint_auth".to_string(),
            "direct_auth".to_string(),
        ]
    );
    assert_eq!(
        auth_requirement_step_ids(&endpoint.policy.auth),
        vec![
            "protected::Show:0:client_auth".to_string(),
            "protected::Show:1:scope_auth".to_string(),
            "protected::Show:2:endpoint_auth".to_string(),
            "protected::Show:3:direct_auth".to_string(),
        ]
    );
    assert_eq!(
        auth_requirement_provenance_labels(&endpoint.policy.auth),
        vec!["client", "scope:0", "endpoint", "endpoint"]
    );
}
