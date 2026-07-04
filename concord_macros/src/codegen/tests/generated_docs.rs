use super::helpers::*;
use quote::quote;

#[test]
fn generated_rustdoc_covers_client_endpoint_and_request_builder() {
    let out = expanded(quote! {
        client SnapshotDocs {
            base "https://example.com"
        }

        GET Search(count?: u64)
            as search
            path ["search"]
            query {
                count
            }
            -> Json<String>
    });

    assert_contains_all(
        &out,
        &[
            "#[doc=\"Generated API client.\"]",
            "#[doc=\"Create a client with the default reqwest transport.\"]",
            "#[doc=\"Builder for required client configuration.\"]",
            "#[doc=\"GET / search\"]",
            "#[doc=\"Query params: `count`\"]",
            "#[doc=\"Response: Json<String>\"]",
            "#[doc=\"Advanced explicit endpoint request. Prefer facade methods for normal use.\"]",
            "#[doc=\"Create this advanced explicit endpoint request.\"]",
            "#[doc=\"Set optional query parameter `count`.\"]",
            "#[doc=\"Set or clear optional query parameter `count` from an Option; None clears it.\"]",
            "#[doc=\"Clear optional query parameter `count`.\"]",
            "#[doc=\"Request-builder extension methods for this endpoint.\"]",
        ],
    );
    assert_generated_doc_attrs_do_not_expose_hidden_names(&out);
}

#[test]
fn generated_rustdoc_includes_behavior_names() {
    let out = expanded(quote! {
        client BehaviorDocs {
            base "https://example.com"

            behavior client_read {
                retry off
            }

            behavior scope_read {
                retry off
            }

            behavior endpoint_read {
                retry off
            }

            defaults {
                behavior client_read
            }
        }

        scope users {
            path ["users"]
            behavior scope_read

            GET Me
                path ["me"]
                behavior endpoint_read
                -> Json<()>
        }
    });

    assert_contains_all(
        &out,
        &["#[doc=\"Behavior: `client_read`, `scope_read`, `endpoint_read`\"]"],
    );
}

#[test]
fn generated_rustdoc_includes_endpoint_contract_without_secret_values() {
    let out = expanded(quote! {
        client SnapshotRichDocs {
            base "https://example.com"
            var tenant: String
            secret api_key: String
            credential key = api_key(secret.api_key)

            default {
                retry read
                rate_limit app
            }

            retry read {
                max_attempts 2
                methods [GET, POST]
            }

            rate_limit app {
                bucket application by [host] {
                    10 / 1s
                }
            }
        }

        POST Create(id: String, filter?: String, count: u64 = 20, body: Json<CreateBody>)
            path ["items", id]
            query {
                filter
                count
            }
            headers {
                "X-Tenant" = vars.tenant
            }
            auth header "X-Api-Key" = key
            -> Json<CreateResponse>

        GET List(id: String, count: u64 = 20)
            path ["items", id]
            query {
                count
            }
            paginate OffsetLimitPagination {
                offset = 0,
                limit = count
            }
            -> Json<Vec<CreateResponse>>
    });

    assert_contains_all(
        &out,
        &[
            "#[doc=\"POST / items / {id}\"]",
            "#[doc=\"Required params: `id`\"]",
            "#[doc=\"Query params: `count`, `filter`\"]",
            "#[doc=\"Headers: `X-Tenant`\"]",
            "#[doc=\"Auth:\"]",
            "#[doc=\"- header `X-Api-Key` = `key`\"]",
            "#[doc=\"Retry: configured\"]",
            "#[doc=\"Rate limit: configured\"]",
            "#[doc=\"Pagination: OffsetLimitPagination\"]",
            "#[doc=\"Body: Json<CreateBody>\"]",
            "#[doc=\"Response: Json<CreateResponse>\"]",
            "#[doc=\"Set optional query parameter `filter`.\"]",
            "#[doc=\"Set defaulted query parameter `count` (default: `20`).\"]",
            "#[doc=\"Reset defaulted query parameter `count` to its default `20`.\"]",
        ],
    );
    assert_generated_doc_attrs_do_not_expose_hidden_names(&out);
    assert_generated_doc_attrs_do_not_contain(&out, "api_key");
}

#[test]
fn generated_rustdoc_redaction_does_not_render_secret_literals() {
    let out = expanded(quote! {
        client SnapshotSecretDocs {
            base "https://example.com"

            auth {
                secret api_key: String
                secret bearer_token: String
                secret username: String
                secret password: String
                secret client_id: String
                secret client_secret: String

                credential upstream = api_key(secret.api_key)
                credential session = bearer(secret.bearer_token)
                credential login = basic(secret.username, secret.password)
                credential oauth = oauth2_client {
                    token_url: "https://auth.example.com/token",
                    client_id: secret.client_id,
                    client_secret: secret.client_secret,
                }
            }

            behaviors {
                behavior protected_read {
                    auth bearer session
                }
            }
        }

        GET GetBearerDoc
            path ["bearer"]
            behavior protected_read
            -> Json<()>

        GET GetHeaderDoc
            path ["header"]
            auth header "X-Api-Key" = upstream
            -> Json<()>

        GET GetBasicDoc
            path ["basic"]
            auth basic login
            -> Json<()>

        GET GetOAuthDoc
            path ["oauth"]
            auth bearer oauth
            -> Json<()>
    });

    assert_contains_all(
        &out,
        &[
            "#[doc=\"Behavior: `protected_read`\"]",
            "#[doc=\"- bearer `session`\"]",
            "#[doc=\"- header `X-Api-Key` = `upstream`\"]",
            "#[doc=\"- basic `login`\"]",
            "#[doc=\"- bearer `oauth`\"]",
        ],
    );
    for secret in [
        "LEAK_SENTINEL_API_KEY_123",
        "LEAK_SENTINEL_BEARER_456",
        "LEAK_SENTINEL_PASSWORD_789",
        "LEAK_SENTINEL_CLIENT_SECRET_ABC",
    ] {
        assert_generated_doc_attrs_do_not_contain(&out, secret);
    }
    assert_generated_doc_attrs_do_not_contain(&out, "client_secret value");
    assert_generated_doc_attrs_do_not_contain(&out, "password value");
}
