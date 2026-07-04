use super::helpers::{analyze_err, assert_error_contains};

#[test]
fn same_layer_duplicate_auth_materialization_targets_are_rejected() {
    for (label, source, expected) in [
        (
            "header",
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret token_a: String
                    secret token_b: String
                    credential a = api_key(secret.token_a)
                    credential b = api_key(secret.token_b)
                }

                GET Show
                    path ["show"]
                    auth header "X-Trace" = a
                    auth header "x-trace" = b
                    -> Json<()>
            }
            "#,
            "duplicate auth header",
        ),
        (
            "query",
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret token_a: String
                    secret token_b: String
                    credential a = api_key(secret.token_a)
                    credential b = api_key(secret.token_b)
                }

                GET Show
                    path ["show"]
                    auth query "api_key" = a
                    auth query "api_key" = b
                    -> Json<()>
            }
            "#,
            "duplicate auth query parameter",
        ),
    ] {
        let err = analyze_err(source);
        assert!(
            err.to_string().contains(expected),
            "{label} duplicate should fail with `{expected}`, got `{err}`"
        );
    }
}

#[test]
fn final_auth_materialization_rejects_case_insensitive_header_collisions() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret client_token: String
                secret scope_token: String
                credential client_auth = api_key(secret.client_token)
                credential scope_auth = api_key(secret.scope_token)
                auth header "X-Token" = client_auth
            }

            scope protected {
                path ["protected"]
                auth header "x-token" = scope_auth

                GET Show
                    path ["show"]
                    -> Json<()>
            }
        }
        "#,
    );
    assert_error_contains(&err, "final endpoint `protected::Show`");
    assert_error_contains(&err, "header `x-token`");
    assert_error_contains(&err, "client");
    assert_error_contains(&err, "scope:0");
}

#[test]
fn final_auth_materialization_rejects_query_collisions() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret client_token: String
                secret endpoint_token: String
                credential client_auth = api_key(secret.client_token)
                credential endpoint_auth = api_key(secret.endpoint_token)
                auth query "api_key" = client_auth
            }

            scope protected {
                path ["protected"]

                GET Show
                    path ["show"]
                    auth query "api_key" = endpoint_auth
                    -> Json<()>
            }
        }
        "#,
    );
    assert_error_contains(&err, "final endpoint `protected::Show`");
    assert_error_contains(&err, "query `api_key`");
    assert_error_contains(&err, "client");
    assert_error_contains(&err, "endpoint");
}

#[test]
fn final_auth_materialization_rejects_bearer_plus_basic_authorization_collisions() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret bearer_token: String
                secret basic_user: String
                secret basic_password: String
                credential bearer_auth = bearer(secret.bearer_token)
                credential basic_auth = basic(secret.basic_user, secret.basic_password)
                auth bearer bearer_auth
            }

            scope protected {
                path ["protected"]
                auth basic basic_auth

                GET Show
                    path ["show"]
                    -> Json<()>
            }
        }
        "#,
    );
    assert_error_contains(&err, "final endpoint `protected::Show`");
    assert_error_contains(&err, "Authorization");
    assert_error_contains(&err, "client");
    assert_error_contains(&err, "scope:0");
    assert_error_contains(&err, "between `client` and `scope:0`");
}

#[test]
fn final_auth_materialization_rejects_bearer_plus_custom_authorization_header_collisions() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret bearer_token: String
                secret header_token: String
                credential bearer_auth = bearer(secret.bearer_token)
                credential header_auth = api_key(secret.header_token)
                auth bearer bearer_auth
            }

            GET Show
                path ["show"]
                auth header "Authorization" = header_auth
                -> Json<()>
        }
        "#,
    );
    assert_error_contains(&err, "final endpoint `Show`");
    assert_error_contains(&err, "Authorization");
    assert_error_contains(&err, "client");
    assert_error_contains(&err, "endpoint");
    assert_error_contains(&err, "between `client` and `endpoint`");
}

#[test]
fn final_auth_materialization_rejects_duplicate_bearer_across_layers() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret client_token: String
                secret scope_token: String
                credential client_auth = bearer(secret.client_token)
                credential scope_auth = bearer(secret.scope_token)
                auth bearer client_auth
            }

            scope protected {
                path ["protected"]
                auth bearer scope_auth

                GET Show
                    path ["show"]
                    -> Json<()>
            }
        }
        "#,
    );
    assert_error_contains(&err, "Authorization");
    assert_error_contains(&err, "client");
    assert_error_contains(&err, "scope:0");
    assert_error_contains(&err, "between `client` and `scope:0`");
}

#[test]
fn final_auth_materialization_rejects_duplicate_basic_across_layers() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret client_user: String
                secret client_pass: String
                secret endpoint_user: String
                secret endpoint_pass: String
                credential client_basic = basic(secret.client_user, secret.client_pass)
                credential endpoint_basic = basic(secret.endpoint_user, secret.endpoint_pass)
                auth basic client_basic
            }

            GET Show
                path ["show"]
                auth basic endpoint_basic
                -> Json<()>
        }
        "#,
    );
    assert_error_contains(&err, "Authorization");
    assert_error_contains(&err, "client");
    assert_error_contains(&err, "endpoint");
    assert_error_contains(&err, "between `client` and `endpoint`");
}

#[test]
fn final_auth_materialization_rejects_certificate_collisions() {
    let err = analyze_err(
        r#"
        api! {
            client Api {
                base "https://example.com"
                credential client_cert = endpoint auth_a::IssueClientCert
                credential scope_cert = endpoint auth_b::IssueScopeCert
            }

            scope auth_a {
                path ["auth-a"]

                GET IssueClientCert
                    path ["cert"]
                    -> Json<ClientCertificate>
            }

            scope auth_b {
                path ["auth-b"]

                GET IssueScopeCert
                    path ["cert"]
                    -> Json<ClientCertificate>
            }

            scope protected {
                path ["protected"]
                auth certificate client_cert

                GET Show
                    path ["show"]
                    auth certificate scope_cert
                    -> Json<()>
            }
        }
        "#,
    );
    assert_error_contains(&err, "final endpoint `protected::Show`");
    assert_error_contains(&err, "certificate");
    assert_error_contains(&err, "endpoint");
}
