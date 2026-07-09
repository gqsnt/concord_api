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
        client BehaviorDocs {
            base "https://example.com"

            profile client_read {
                retry off
            }

            profile scope_read {
                retry off
            }

            profile endpoint_read {
                retry off
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
    assert_generated_doc_attrs_do_not_contain(&out, "Behavior:");
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
            "#[doc=\"- max attempts: 2\"]",
            "#[doc=\"- methods: GET, POST\"]",
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
fn generated_rustdoc_includes_no_auth_retry_rate_limit_sections() {
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
            "#[doc=\"- off\"]",
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

        GET Listed
            path ["listed"]
            -> Records<Item, NdJson>

        GET Multiparted
            path ["multiparted"]
            -> Multipart<Part, Mixed>

        GET Ssed
            path ["ssed"]
            -> Sse<Event>
    });

    assert_contains_all(
        &out,
        &[
            "#[doc=\"- Terminal: `.execute_stream().await` returns `::concord_core::advanced::StreamResponse<OctetStream>`\"]",
            "#[doc=\"- Metadata terminal: `.response().await` is unavailable; use `.execute_stream().await`.\"]",
            "#[doc=\"- Terminal: `.execute_records().await` returns `::concord_core::advanced::RecordStream<Item>`\"]",
            "#[doc=\"- Metadata terminal: `.response().await` is unavailable; use `.execute_records().await`.\"]",
            "#[doc=\"- Terminal: `.execute_multipart().await` returns `::concord_core::advanced::MultipartStream<Part>`\"]",
            "#[doc=\"- Metadata terminal: `.response().await` is unavailable; use `.execute_multipart().await`.\"]",
            "#[doc=\"- Terminal: `.execute_sse().await` returns `::concord_core::advanced::SseStream<Event>`\"]",
            "#[doc=\"- Metadata terminal: `.response().await` is unavailable; use `.execute_sse().await`.\"]",
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
