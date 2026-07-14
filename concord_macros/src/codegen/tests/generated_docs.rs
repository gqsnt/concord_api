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
            "#[doc=\"Create a client backed by Concord's managed Reqwest client.\"]",
            "#[doc=\"Builder for required client configuration.\"]",
            "#[doc=\"GET / search\"]",
            "#[doc=\"HTTP:\"]",
            "#[doc=\"- Method: GET\"]",
            "#[doc=\"- Path: /search\"]",
            "#[doc=\"- Base: https://example.com\"]",
            "#[doc=\"Query:\"]",
            "#[doc=\"- Params: `count`\"]",
            "#[doc=\"Response:\"]",
            "#[doc=\"- Json<String>\"]",
            "#[doc=\"- Terminal: `.execute().await` returns `String`\"]",
            "#[doc=\"- Metadata terminal: `.response().await` returns `DecodedResponse<String>` with status, headers, url, and meta.\"]",
            "#[doc=\"Safety:\"]",
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
fn generated_rustdoc_includes_profile_names() {
    let out = expanded(quote! {
        client ProfileDocs {
            base "https://example.com"

            profile client_read {
            }

            profile scope_read {
            }

            profile endpoint_read {
            }

            default {
                profile client_read
            }
        }

        scope users {
            path ["users"]
            profile scope_read

            GET Me
                path ["me"]
                profile endpoint_read
                -> Json<()>
        }
    });

    assert_contains_all(
        &out,
        &["#[doc=\"Profile: `client_read`, `scope_read`, `endpoint_read`\"]"],
    );
}

#[test]
fn generated_rustdoc_describes_optional_vector_query_setters() {
    let out = expanded(quote! {
        client VectorDocs {
            base "https://example.com"
        }

        GET Search(tags?: Vec<String>)
            as search
            path ["search"]
            query { tags }
            -> Json<String>
    });

    assert_contains_all(
        &out,
        &[
            "Set optional query parameter `tags`. Values produce repeated query pairs in vector order; setting the vector replaces existing values, and an empty vector removes the key.",
            "Set or clear optional query parameter `tags` from an Option; None clears it. Values produce repeated query pairs in vector order; setting the vector replaces existing values, and an empty vector removes the key.",
        ],
    );
}

#[test]
fn generated_rustdoc_describes_only_client_query_vectors_as_repeated() {
    let out = expanded(quote! {
        client ClientSetterDocs {
            base "https://example.com"
            var optional_tags?: Vec<String>
            var default_tags: Vec<String> = Vec::new()
            var ordinary_tags: Vec<String>
            var query_scalar: String

            query {
                "tags" = vars.optional_tags
                "default-tags" = vars.default_tags
                "scalar" = vars.query_scalar
            }
        }

        GET Search
            path ["search"]
            -> Json<String>
    });

    assert_contains_all(
        &out,
        &[
            "Set client parameter `optional_tags`. Values produce repeated query pairs in vector order; setting the vector replaces existing values, and an empty vector removes the key.",
            "Clear client parameter `optional_tags`; this produces None and removes the query key. Values produce repeated query pairs in vector order; setting the vector replaces existing values, and an empty vector removes the key.",
            "Set client parameter `default_tags`. Values produce repeated query pairs in vector order; setting the vector replaces existing values, and an empty vector removes the key.",
            "Set client parameter `ordinary_tags`.",
            "Set client parameter `query_scalar`.",
        ],
    );
    assert_generated_doc_attrs_do_not_contain(
        &out,
        "Set client parameter `ordinary_tags`. Values produce repeated",
    );
    assert_generated_doc_attrs_do_not_contain(
        &out,
        "Set client parameter `query_scalar`. Values produce repeated",
    );
}

#[test]
fn generated_rustdoc_keeps_client_and_endpoint_query_names_separate() {
    for endpoint_policy in [
        quote! {
            tags
            "client-tags" = vars.tags
        },
        quote! {
            "client-tags" = vars.tags
            tags
        },
    ] {
        let out = expanded(quote! {
            client NamespaceCollisionDocs {
                base "https://example.com"
                var tags: Vec<String>

                query {
                    "client-tags" = vars.tags
                }
            }

            GET Search(tags: String = "endpoint")
                path ["search"]
                query { #endpoint_policy }
                -> Json<String>
        });

        assert_contains_all(
            &out,
            &[
                "Set client parameter `tags`. Values produce repeated query pairs in vector order; setting the vector replaces existing values, and an empty vector removes the key.",
                "Set defaulted query parameter `tags`.",
            ],
        );
        assert_generated_doc_attrs_do_not_contain(
            &out,
            "Set defaulted query parameter `tags`. Values produce repeated",
        );
    }
}

#[test]
fn generated_rustdoc_keeps_client_and_scope_query_names_separate() {
    let out = expanded(quote! {
        client ScopeNamespaceCollisionDocs {
            base "https://example.com"
            var tags: Vec<String>

            query {
                "client-tags" = vars.tags
            }
        }

        scope grouped(tags: String = "scope") {
            query {
                tags
                "client-tags" = vars.tags
            }

            GET Search
                path ["search"]
                -> Json<String>
        }
    });

    assert_contains_all(
        &out,
        &[
            "Set client parameter `tags`. Values produce repeated query pairs in vector order; setting the vector replaces existing values, and an empty vector removes the key.",
            "Set defaulted scope parameter `tags`.",
        ],
    );
    assert_generated_doc_attrs_do_not_contain(
        &out,
        "Set defaulted scope parameter `tags`. Values produce repeated",
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
                rate_limit app
            }

            rate_limit app {
                bucket application by [host] {
                    10 / 1s
                }
            }
        }

        POST Create(
            id: String,
            filter?: String,
            count: u64 = { let _ = "LEAK_SENTINEL_DEFAULT"; 20 },
            body: Json<CreateBody>
        )
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
            "#[doc=\"HTTP:\"]",
            "#[doc=\"- Method: POST\"]",
            "#[doc=\"- Path: /items/{id}\"]",
            "#[doc=\"- Base: https://example.com\"]",
            "#[doc=\"Request:\"]",
            "#[doc=\"- Required params: `id`\"]",
            "#[doc=\"Query:\"]",
            "#[doc=\"- Params: `count`, `filter`\"]",
            "#[doc=\"Headers:\"]",
            "#[doc=\"- Names: `X-Tenant`\"]",
            "#[doc=\"Auth:\"]",
            "#[doc=\"- header `X-Api-Key` = `key`\"]",
            "#[doc=\"Retry:\"]",
            "#[doc=\"- selected at client construction through `RetryMode`\"]",
            "#[doc=\"Rate limit:\"]",
            "#[doc=\"- bucket `application` key [host] cost 1 windows [10 / 1s]\"]",
            "#[doc=\"Pagination:\"]",
            "#[doc=\"- Controller: OffsetLimitPagination\"]",
            "#[doc=\"Body:\"]",
            "#[doc=\"- Json<CreateBody>\"]",
            "#[doc=\"Replayability:\"]",
            "#[doc=\"- replayable\"]",
            "#[doc=\"Response:\"]",
            "#[doc=\"- Json<CreateResponse>\"]",
            "#[doc=\"- Terminal: `.execute().await` returns `CreateResponse`\"]",
            "#[doc=\"- Metadata terminal: `.response().await` returns `DecodedResponse<CreateResponse>` with status, headers, url, and meta.\"]",
            "#[doc=\"Safety:\"]",
            "#[doc=\"Set optional query parameter `filter`.\"]",
            "#[doc=\"Set defaulted query parameter `count`.\"]",
            "#[doc=\"Set defaulted query parameter `count` from an Option; None resets to the declared default.\"]",
            "#[doc=\"Reset defaulted query parameter `count` to its declared default.\"]",
        ],
    );
    assert_generated_doc_attrs_do_not_expose_hidden_names(&out);
    assert_generated_doc_attrs_do_not_contain(&out, "api_key");
    assert_generated_doc_attrs_do_not_contain(&out, "LEAK_SENTINEL_DEFAULT");
    assert_generated_doc_attrs_do_not_contain(&out, "default: `20`");
}

#[test]
fn generated_rustdoc_rate_limit_add_accumulates_inherited_buckets() {
    let out = expanded(quote! {
        client RateLimitAddDocs {
            base "https://example.com"

            rate_limit client_limit {
                bucket client by [host] {
                    1 / 1s
                }
            }

            rate_limit scope_limit {
                bucket scope by [host, endpoint] {
                    2 / 1s
                }
            }

            profiles {
                profile client_rate_limit {
                    rate_limit client_limit
                }

                profile scope_rate_limit {
                    rate_limit scope_limit
                }
            }

            default {
                profile client_rate_limit
            }
        }

        scope users {
            path ["users"]
            profile scope_rate_limit

            GET List
                path ["list"]
                -> Json<()>
        }
    });

    assert_contains_all(
        &out,
        &[
            "#[doc=\"Rate limit:\"]",
            "#[doc=\"- bucket `client` key [host] cost 1 windows [1 / 1s]\"]",
            "#[doc=\"- bucket `scope` key [host, endpoint] cost 1 windows [2 / 1s]\"]",
        ],
    );
}

#[test]
fn generated_rustdoc_rate_limit_replace_discards_inherited_buckets() {
    let out = expanded(quote! {
        client RateLimitReplaceDocs {
            base "https://example.com"

            rate_limit client_limit {
                bucket client by [host] {
                    1 / 1s
                }
            }

            rate_limit endpoint_limit {
                bucket endpoint by [host, endpoint] {
                    3 / 1s
                }
            }

            profiles {
                profile client_rate_limit {
                    rate_limit client_limit
                }
            }

            default {
                profile client_rate_limit
            }
        }

        GET Show
            path ["show"]
            rate_limit only endpoint_limit
            -> Json<()>
    });

    assert_contains_all(
        &out,
        &[
            "#[doc=\"Rate limit:\"]",
            "#[doc=\"- bucket `endpoint` key [host, endpoint] cost 1 windows [3 / 1s]\"]",
        ],
    );
    assert_generated_doc_attrs_do_not_contain(
        &out,
        "bucket `client` key [host] cost 1 windows [1 / 1s]",
    );
}

#[test]
fn generated_rustdoc_rate_limit_clear_removes_inherited_configuration() {
    let out = expanded(quote! {
        client RateLimitClearDocs {
            base "https://example.com"

            rate_limit client_limit {
                bucket client by [host] {
                    1 / 1s
                }
            }

            profiles {
                profile client_rate_limit {
                    rate_limit client_limit
                }

                profile clear_rate_limit {
                    rate_limit off
                }
            }

            default {
                profile client_rate_limit
            }
        }

        GET Show
            path ["show"]
            profile clear_rate_limit
            -> Json<()>
    });

    assert_contains_all(&out, &["#[doc=\"Rate limit:\"]", "#[doc=\"- none\"]"]);
}

#[test]
fn generated_rustdoc_includes_no_auth_or_rate_limit_sections() {
    let out = expanded(quote! {
        client EmptyContractDocs {
            base "https://example.com"
        }

        GET Ping
            path ["ping"]
            -> Json<()>
    });

    assert_contains_all(
        &out,
        &[
            "#[doc=\"HTTP:\"]",
            "#[doc=\"Response:\"]",
            "#[doc=\"- Json<()>\"]",
            "#[doc=\"Auth:\"]",
            "#[doc=\"- none\"]",
            "#[doc=\"Retry:\"]",
            "#[doc=\"- selected at client construction through `RetryMode`\"]",
            "#[doc=\"Rate limit:\"]",
            "#[doc=\"- none\"]",
            "#[doc=\"Safety:\"]",
        ],
    );
}

#[test]
fn generated_rustdoc_includes_streaming_terminal_methods() {
    let out = expanded(quote! {
        client StreamingDocs {
            base "https://example.com"
        }

        GET Streamed
            path ["streamed"]
            -> Stream<OctetStream>

    });

    assert_contains_all(
        &out,
        &[
            "#[doc=\"- Terminal: `.execute_stream().await` returns `::concord_core::advanced::StreamResponse<OctetStream>`\"]",
            "#[doc=\"- Metadata terminal: `.response().await` is unavailable; use `.execute_stream().await`.\"]",
        ],
    );
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

            profiles {
                profile protected_read {
                    auth bearer session
                }
            }
        }

        GET GetBearerDoc
            path ["bearer"]
            profile protected_read
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
            "#[doc=\"Profile: `protected_read`\"]",
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
