use quote::{format_ident, quote};

use super::super::ast::{
    AuthOperation, DefaultExpr, RateLimitAttachmentDecl, RateLimitDecl, ValueKind,
};
use super::*;

fn parse_ok(tokens: TokenStream) -> SourceApi {
    parse(tokens).expect("source AST")
}

fn inline_rate_limit(attachment_site: &AttachmentSite) -> &RateLimitDecl {
    match &attachment_site.rate_limits[0].value {
        RateLimitAttachmentDecl::Inline(rule) => rule,
        RateLimitAttachmentDecl::Named(_) => panic!("anonymous rate-limit attachment"),
    }
}

#[test]
fn parses_singular_client_root_children_values_and_all_attachment_sites() {
    let api = parse_ok(quote! {
        client FirstApi {
            base "https://example.test:8443/fixed";
            vars {
                tenant: String;
                locale: Option<String>;
                limit: u32 = 50;
            }
            host ["api"];
            path ["v1"];
            query { "tenant" = vars.tenant; }
            headers { "X-Locale" = vars.locale; }
            timeout 5s;
        }
        GET root(id: String, verbose: Option<bool>, page: u32 = 10) -> Bytes {
            path [ep.id];
        }
        scope regions(region: String, language: Option<String>) {
            host [scope.region, "api"];
            path ["regions", scope.region];
            scope users(account: String) {
                path ["users", scope.account];
                METHOD("PROPFIND") inspect() -> Text;
            }
        }
    });
    assert_eq!(canonical_ident(&api.client.name), "FirstApi");
    assert_eq!(api.client.values.as_ref().unwrap().value.len(), 3);
    assert_eq!(api.children.len(), 2);
}

#[test]
fn singular_api_structure_and_vars_diagnostics_are_focused() {
    let empty = parse(TokenStream::new()).err().expect("missing client");
    assert!(empty.to_string().contains("exactly one `client`"));

    let child_first = parse(quote! {
        GET status() -> Bytes;
        client Api { base "https://example.test"; }
    })
    .err()
    .expect("child before client");
    assert!(child_first.to_string().contains("must be the first"));

    let second_client = parse(quote! {
        client FirstApi { base "https://one.example.test"; }
        client SecondApi { base "https://two.example.test"; }
    })
    .err()
    .expect("second client");
    assert!(second_client.to_string().contains("exactly one `client`"));
    assert!(
        second_client
            .to_string()
            .contains("separate `api!` invocations")
    );

    let api = parse_ok(quote! {
        client Api { base "https://example.test"; }
        GET root() -> Bytes;
        scope first() { GET nested() -> Bytes; }
        scope second() { }
    });
    assert_eq!(api.children.len(), 3);

    let no_vars = parse_ok(quote! {
        client Api { base "https://example.test"; }
    });
    assert!(no_vars.client.values.is_none());

    let empty_vars = parse_ok(quote! {
        client Api { base "https://example.test"; vars { } }
    });
    assert!(empty_vars.client.values.as_ref().unwrap().value.is_empty());

    let values = parse_ok(quote! {
        client Api {
            base "https://example.test";
            vars {
                required: String;
                optional: Option<String>;
                defaulted: usize = crate::DEFAULT_SIZE;
            }
        }
    });
    let values = &values.client.values.as_ref().unwrap().value;
    assert!(matches!(values[0].kind, ValueKind::Required));
    assert!(matches!(values[1].kind, ValueKind::Optional));
    assert!(matches!(
        values[2].kind,
        ValueKind::Defaulted(DefaultExpr::Path(_))
    ));

    let duplicate_value = parse(quote! {
        client Api {
            base "https://example.test";
            vars { value: String; value: String; }
        }
    })
    .err()
    .expect("duplicate value");
    assert!(
        duplicate_value
            .to_string()
            .contains("duplicate client value `value`")
    );

    parse(quote! {
        client Api {
            credentials { }
            vars { value: String; }
            base "https://example.test";
        }
    })
    .expect("client clauses are order-independent");

    let duplicate_vars = parse(quote! {
        client Api {
            base "https://example.test";
            vars { }
            vars { }
        }
    })
    .err()
    .expect("duplicate vars");
    assert!(
        duplicate_vars
            .to_string()
            .contains("duplicate `vars` clause")
    );

    for tokens in [
        quote! { client Api { base "https://example.test"; scope nested() { } } },
        quote! { client Api { base "https://example.test"; GET nested() -> Bytes; } },
    ] {
        let error = parse(tokens).err().expect("client child");
        assert!(error.to_string().contains("after the `client` block"));
    }

    let root_config = parse(quote! {
        client Api { base "https://example.test"; }
        timeout 1s;
    })
    .err()
    .expect("root client configuration");
    assert!(
        root_config
            .to_string()
            .contains("inside the `client` block")
    );
}

#[test]
fn removed_retry_dsl_has_a_focused_migration_diagnostic() {
    for declaration in [
        quote! {
            client Api {
                base "https://example.test";
                retry off;
            }
        },
        quote! {
            client Api {
                base "https://example.test";
                retry reqwest_default;
            }
        },
    ] {
        let error = parse(declaration)
            .err()
            .expect("removed retry DSL must fail");
        let diagnostic = error.to_string();
        assert!(diagnostic.contains("`retry` DSL clause was removed"));
        assert!(diagnostic.contains("`.retry(policy)`"));
    }
}

#[test]
fn retired_declarations_have_focused_replacement_diagnostics() {
    let cases = [
        (
            quote! { client Api { base "https://example.test"; secret token: String; } },
            "`credentials { ... }` block",
        ),
        (
            quote! { client Api { base "https://example.test"; credential token = bearer(secret.token); } },
            "`credentials { ... }` block",
        ),
        (
            quote! { client Api { base "https://example.test"; default { timeout 1s; } } },
            "`default { ... }` is not DSL syntax",
        ),
        (
            quote! { client Api { base "https://example.test"; defaults { timeout 1s; } } },
            "`defaults { ... }` is not DSL syntax",
        ),
        (
            quote! { client Api { base "https://example.test"; policies { } } },
            "`policies { ... }` is not DSL syntax",
        ),
        (
            quote! { client Api { base "https://example.test"; behavior finite; } },
            "behavior profiles are not DSL syntax",
        ),
        (
            quote! { client Api { base "https://example.test"; prefix ["v1"]; } },
            "`prefix` is not DSL syntax",
        ),
        (
            quote! { client Api { base "https://example.test"; params { "id" = vars.id; } } },
            "`params { ... }` is not DSL syntax",
        ),
        (
            quote! { client Api { base "https://example.test"; profile standard { } } },
            "`profile` is not a DSL keyword",
        ),
        (
            quote! { client Api { base "https://example.test"; access_token token: String; } },
            "`access_token` is not DSL syntax",
        ),
    ];

    for (tokens, expected) in cases {
        let error = parse(tokens).err().expect("retired syntax must fail");
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn rejects_arbitrary_defaults_and_public_expressions() {
    let default_error = parse(quote! {
        client Api {
            base "https://example.test";
            vars { value: String = make_value(); }
        }
    })
    .err()
    .expect("call default");
    assert!(default_error.to_string().contains("Rust const path"));

    let expression_error = parse(quote! {
        client Api {
            base "https://example.test";
        }
        GET root(id: String) -> Bytes { path [ep.id.to_string()]; }
    })
    .err()
    .expect("method expression");
    assert!(
        expression_error
            .to_string()
            .contains("unsupported endpoint clause")
            || expression_error
                .to_string()
                .contains("arbitrary Rust expressions")
    );
}

#[test]
fn duration_and_custom_method_boundaries_are_checked() {
    let zero = parse(quote! {
        client Api { base "https://example.test"; timeout 0s; }
    })
    .err()
    .expect("zero timeout");
    assert!(zero.to_string().contains("greater than zero"));

    let non_decimal = parse(quote! {
        client Api { base "https://example.test"; timeout 0x10s; }
    })
    .err()
    .expect("non-decimal timeout");
    assert!(non_decimal.to_string().contains("base-10"));

    let invalid = parse(quote! {
        client Api { base "https://example.test"; }
        METHOD("BAD METHOD") root() -> Bytes;
    })
    .err()
    .expect("invalid method");
    assert!(invalid.to_string().contains("valid HTTP method token"));

    let overflow = parse(quote! {
        client Api {
            base "https://example.test";
            timeout 18446744073709551615h;
        }
    })
    .err()
    .expect("overflowing timeout");
    assert!(overflow.to_string().contains("overflows"));
}

fn nested_scopes(depth: usize) -> TokenStream {
    let mut child = quote!(GET leaf() -> Bytes;);
    for index in 0..depth {
        let name = format_ident!("scope_{index}");
        child = quote!(scope #name() { #child });
    }
    quote!(client Api { base "https://example.test"; } #child)
}

fn nested_format(depth: usize) -> TokenStream {
    let mut expression = quote!("leaf");
    for _ in 0..depth {
        expression = quote!(fmt[#expression]);
    }
    quote! {
        client Api {
            base "https://example.test";
        }
        GET leaf() -> Bytes { path [#expression]; }
    }
}

#[test]
fn scope_and_format_depth_bounds_accept_64_and_reject_65() {
    parse(nested_scopes(64)).expect("64 nested scopes");
    let scope_error = parse(nested_scopes(65)).err().expect("65 nested scopes");
    assert!(
        scope_error
            .to_string()
            .contains("maximum supported depth of 64")
    );

    parse(nested_format(64)).expect("64 nested formats");
    let format_error = parse(nested_format(65)).err().expect("65 nested formats");
    assert!(format_error.to_string().contains("maximum depth of 64"));
}

#[test]
fn focused_structure_and_local_duplicate_diagnostics_are_stable() {
    let body_order = parse_ok(quote! {
        client Api {
            base "https://example.test";
        }
        POST create(#[body] body: Text, id: String) -> Bytes;
    });
    let SourceChild::Endpoint(endpoint) = &body_order.children[0] else {
        panic!("endpoint")
    };
    assert_eq!(endpoint.body.as_ref().unwrap().position, 0);
    assert_eq!(endpoint.parameters[0].position, 1);

    let duplicate_query = parse(quote! {
        client Api {
            base "https://example.test";
            query { "key" = "one"; remove "key"; }
        }
    })
    .err()
    .expect("duplicate query key");
    assert!(
        duplicate_query
            .to_string()
            .contains("duplicate local query key")
    );

    let duplicate_header = parse(quote! {
        client Api {
            base "https://example.test";
            headers { "X-Key" = "one"; "x-key" = "two"; }
        }
    })
    .err()
    .expect("case-insensitive duplicate header");
    assert!(duplicate_header.to_string().contains("case-insensitive"));
}

#[test]
fn defaults_formats_and_static_atoms_obey_grammar_boundaries() {
    parse(quote! {
        client Api {
            base "https://example.test";
            vars {
                text: &'static str = "value";
                negative: i64 = -7;
                enabled: bool = true;
                constant: usize = crate::DEFAULT_SIZE;
            }
            host ["a12345678901234567890123456789012345678901234567890123456789012"];
        }
        GET read(id: u32) -> Bytes { path [fmt["id-", ep.id]]; }
    })
    .expect("restricted defaults and 63-byte host label");

    let host = parse(quote! {
        client Api {
            base "https://example.test";
            host ["a123456789012345678901234567890123456789012345678901234567890123"];
        }
    })
    .err()
    .expect("64-byte host label");
    assert!(host.to_string().contains("1-63 byte"));

    let segment = parse(quote! {
        client Api { base "https://example.test"; path ["two/segments"]; }
    })
    .err()
    .expect("static path structure");
    assert!(segment.to_string().contains("static path atom"));

    let commas = parse(quote! {
        client Api {
            base "https://example.test";
        }
        GET read(id: u32) -> Bytes { path [fmt["id-" ep.id]]; }
    })
    .err()
    .expect("missing format comma");
    assert!(commas.to_string().contains("separated by commas"));

    parse(quote! {
        client Api {
            base "https://example.test";
            credentials { }
        }
    })
    .expect("an empty credentials block is structurally valid");
}

#[test]
fn credentials_parse_all_sources_and_enforce_definition_order() {
    let api = parse_ok(quote! {
        client AuthApi {
            base "https://example.test";
            credentials {
                secret api_key: String;
                secret bearer: String;
                secret username: String;
                secret password: String;
                credential key = api_key(secret.api_key);
                credential token = bearer(secret.bearer);
                credential login = basic(secret.username, secret.password);
                credential acquired = endpoint login;
                credential custom = custom crate::SessionProvider;
            }
        }
        GET login() -> Bytes;
    });
    let credentials = api.client.credentials.as_ref().expect("credentials block");
    assert_eq!(credentials.value.secrets.len(), 4);
    assert_eq!(credentials.value.credentials.len(), 5);

    let error = parse(quote! {
        client AuthApi {
            base "https://example.test";
            credentials {
                credential key = api_key(secret.later);
                secret later: String;
            }
        }
    })
    .err()
    .expect("forward secret reference");
    assert!(
        error
            .to_string()
            .contains("not-yet-declared secret `later`")
    );

    let error = parse(quote! {
        client AuthApi {
            base "https://example.test";
            credentials { secret first: String; }
            credentials { secret second: String; }
        }
    })
    .err()
    .expect("second credentials block");
    assert!(error.to_string().contains("duplicate `credentials` clause"));
}

#[test]
fn authentication_attachment_site_operations_and_challenge_suffixes_are_bounded() {
    parse(quote! {
        client AuthApi {
            base "https://example.test";
            credentials {
                secret token: String;
                credential session = bearer(secret.token);
            }
            auth replace bearer session challenge unauthorized_or_forbidden;
            auth query "fallback" = session challenge never;
        }
        scope public() { auth off; }
    })
    .expect("bounded auth syntax");

    let off = parse(quote! {
        client AuthApi {
            base "https://example.test";
            auth off;
            auth bearer session;
        }
    })
    .err()
    .expect("auth off exclusivity");
    assert!(off.to_string().contains("`auth off` is exclusive"));

    let late_replace = parse(quote! {
        client AuthApi {
            base "https://example.test";
            auth bearer first;
            auth replace bearer second;
        }
    })
    .err()
    .expect("late replacement");
    assert!(late_replace.to_string().contains("must be the first"));

    parse(quote! {
        client AuthApi {
            base "https://example.test";
            timeout 1s;
            auth bearer session;
        }
    })
    .expect("authentication entries may follow timeout");
}

#[test]
fn secret_shapes_and_custom_provider_paths_have_focused_diagnostics() {
    let optional = parse(quote! {
        client AuthApi {
            base "https://example.test";
            credentials { secret token: Option<String>; }
        }
    })
    .err()
    .expect("optional secret");
    assert!(optional.to_string().contains("required `String`"));

    let non_path_provider = parse(quote! {
        client AuthApi {
            base "https://example.test";
            credentials {
                credential session = custom (crate::Provider,);
            }
        }
    })
    .err()
    .expect("non-path provider type");
    assert!(
        non_path_provider
            .to_string()
            .contains("provider must be a Rust type path")
    );
}

#[test]
fn lexical_rate_limit_declarations_parse_in_order() {
    let api = parse_ok(quote! {
        client RateApi {
            base "https://example.test";
            vars { tenant: String; }
            timeout 5s;
            rate_limit {
                500 / 10s;
                30000 / 10m;
            }
        }
        scope tenants(tenant: String) {
            rate_limit {
                100 / 1m;
            }
            GET read(item: String) -> Bytes {
                timeout 2s;
                rate_limit {
                    cost 2;
                    10 / 1s;
                }
            }
            GET public() -> Bytes;
        }
    });
    let client = &api.client;
    assert_eq!(inline_rate_limit(&client.attachment_site).windows.len(), 2);
    let SourceChild::Scope(scope) = &api.children[0] else {
        panic!("scope")
    };
    assert_eq!(scope.attachment_site.rate_limits.len(), 1);
    let SourceChild::Endpoint(read) = &scope.children[0] else {
        panic!("read endpoint")
    };
    assert_eq!(inline_rate_limit(&read.attachment_site).cost, 2);
    let SourceChild::Endpoint(public) = &scope.children[1] else {
        panic!("public endpoint")
    };
    assert!(public.attachment_site.rate_limits.is_empty());
}

#[test]
fn rate_limit_cost_defaults_and_equal_capacity_is_valid() {
    let api = parse_ok(quote! {
        client Api {
            base "https://example.test";
            rate_limit {
                1 / 1s;
            }
        }
        scope narrow() {
            rate_limit {
                cost 2;
                2 / 1s;
                10 / 1m;
            }
            GET read() -> Bytes;
        }
    });
    assert_eq!(inline_rate_limit(&api.client.attachment_site).cost, 1);
    let SourceChild::Scope(scope) = &api.children[0] else {
        panic!("scope")
    };
    let rule = inline_rate_limit(&scope.attachment_site);
    assert_eq!(rule.cost, 2);
    assert_eq!(rule.windows.len(), 2);
    assert_eq!(rule.windows[0].capacity, 2);
    assert_eq!(rule.windows[1].capacity, 10);
}

#[test]
fn rate_limit_syntax_rejects_invalid_structure_values_dimensions_and_order() {
    let duplicate_rule = parse(quote! {
        client Api {
            base "https://example.test";
            rate_limit { 1 / 1s; }
            rate_limit { 1 / 1s; }
        }
    })
    .err()
    .expect("duplicate direct rule");
    assert!(
        duplicate_rule
            .to_string()
            .contains("at most one anonymous `rate_limit")
    );

    for (tokens, expected) in [
        (
            quote! { client Api { base "https://example.test"; rate_limit { cost 0; 1 / 1s; } } },
            "cost must be greater than zero",
        ),
        (
            quote! { client Api { base "https://example.test"; rate_limit { 0 / 1s; } } },
            "capacity must be greater than zero",
        ),
        (
            quote! { client Api { base "https://example.test"; rate_limit { 1 / 0s; } } },
            "duration must be greater than zero",
        ),
        (
            quote! { client Api { base "https://example.test"; rate_limit { 1 / 0x10s; } } },
            "duration integer literals must be base-10",
        ),
        (
            quote! { client Api { base "https://example.test"; rate_limit { cost 2; 1 / 1s; } } },
            "must not exceed any window capacity",
        ),
        (
            quote! { client Api { base "https://example.test"; rate_limit { cost 2; 100 / 1s; 1 / 1m; } } },
            "must not exceed any window capacity",
        ),
    ] {
        let error = parse(tokens).err().expect("invalid rate syntax");
        assert!(
            error.to_string().contains(expected),
            "expected `{expected}`, got `{error}`"
        );
    }

    parse(quote! {
        client Api {
            base "https://example.test";
        }
        GET read() -> Bytes {
            rate_limit { 1 / 1s; }
            timeout 1s;
        }
    })
    .expect("endpoint clauses are order-independent");
}

#[test]
fn pagination_struct_forms_parse_with_restricted_initializers() {
    let api = parse_ok(quote! {
        client Api {
            base "https://example.test";
        }
        GET offset(offset: u64, limit: u32) -> PageSpec<Vec<u32>> {
            paginate concord_core::contracts::OffsetController {
                offset: bind ep.offset,
                page_size: bind ep.limit,
            }
        }
        GET page(page: u64, per_page: u32) -> PageSpec<Vec<u32>> {
            paginate PageController {
                page: bind ep.page,
                page_size: bind ep.per_page,
            }
        }
        GET cursor(cursor: Option<String>) -> PageSpec<Vec<u32>> {
            paginate CursorController<String> { cursor: bind ep.cursor }
        }
        scope regions(region: String) {
            GET custom(cursor: Option<String>, maximum: u32) -> PageSpec<Vec<u32>> {
                paginate crate::pagination::VendorCursor {
                    cursor: bind ep.cursor,
                    region: scope.region,
                    empty_pages: 0,
                    enabled: true,
                    label: "vendor",
                    maximum: vars.maximum,
                    policy: crate::pagination::DEFAULT_POLICY,
                }
            }
        }
    });
    assert_eq!(api.children.len(), 4);
}

#[test]
fn pagination_accepts_an_explicit_request_body_for_rust_trait_validation() {
    let api = parse_ok(quote! {
        client Api {
            base "https://example.test";
        }
        POST search(
            offset: u64,
            #[body] filter: Json<Filter>,
        ) -> PageSpec<Vec<u32>> {
            paginate OffsetController {
                offset: bind ep.offset,
                page_size: 25,
            }
        }
    });
    let SourceChild::Endpoint(endpoint) = &api.children[0] else {
        panic!("body pagination endpoint");
    };
    assert!(endpoint.body.is_some());
    assert!(endpoint.pagination.is_some());
}

#[test]
fn pagination_rejects_old_forms_and_unsafe_initializers() {
    let cases = [
        (
            quote! {
                client Api { base "https://example.test"; }
                GET read(offset: u64, limit: u32) -> PageSpec<Vec<u32>> {
                    paginate OffsetController
                }
            },
            "struct field block",
        ),
        (
            quote! {
                client Api { base "https://example.test"; }
                GET read(offset: u64, limit: u32) -> PageSpec<Vec<u32>> {
                    paginate OffsetController { offset = ep.offset }
                }
            },
            "field: bind ep.field",
        ),
        (
            quote! {
                client Api { base "https://example.test"; }
                GET read(offset: u64, limit: u32) -> PageSpec<Vec<u32>> {
                    paginate OffsetController {
                        offset: bind ep.offset,
                        offset: bind ep.limit,
                    }
                }
            },
            "duplicate pagination controller field",
        ),
        (
            quote! {
                client Api { base "https://example.test"; }
                GET read(offset: u64, limit: u32) -> PageSpec<Vec<u32>> {
                    paginate OffsetController {
                        offset: bind ep.offset,
                        page_size: bind ep.offset,
                    }
                }
            },
            "duplicate pagination binding destination",
        ),
        (
            quote! {
                client Api { base "https://example.test"; }
                GET read(offset: u64, limit: u32) -> PageSpec<Vec<u32>> {
                    paginate OffsetController { offset: bind ep.state.offset }
                }
            },
            "must be direct",
        ),
        (
            quote! {
                client Api { base "https://example.test"; }
                GET read(offset: u64) -> PageSpec<Vec<u32>> {
                    paginate OffsetController { offset }
                }
            },
            "field shorthand",
        ),
        (
            quote! {
                client Api { base "https://example.test"; }
                GET read(offset: u64) -> PageSpec<Vec<u32>> {
                    paginate OffsetController { .. }
                }
            },
            "rest syntax",
        ),
        (
            quote! {
                client Api { base "https://example.test"; }
                GET read() -> PageSpec<Vec<u32>> {
                    paginate crate::Controller { value: crate::make() }
                }
            },
            "calls, methods, macros",
        ),
        (
            quote! {
                client Api { base "https://example.test"; }
                GET read() -> PageSpec<Vec<u32>> {
                    paginate custom crate::factory();
                }
            },
            "old `paginate custom factory(...)` syntax",
        ),
    ];
    for (tokens, expected) in cases {
        let error = parse(tokens).err().expect("invalid pagination");
        assert!(
            error.to_string().contains(expected),
            "expected `{expected}`, got `{error}`"
        );
    }
}

fn clause_permutations(clauses: &[TokenStream]) -> Vec<TokenStream> {
    fn visit(
        clauses: &[TokenStream],
        used: &mut [bool],
        current: &mut Vec<TokenStream>,
        output: &mut Vec<TokenStream>,
    ) {
        if current.len() == clauses.len() {
            output.push(quote!(#(#current)*));
            return;
        }
        for index in 0..clauses.len() {
            if used[index] {
                continue;
            }
            used[index] = true;
            current.push(clauses[index].clone());
            visit(clauses, used, current, output);
            current.pop();
            used[index] = false;
        }
    }

    let mut output = Vec::new();
    visit(
        clauses,
        &mut vec![false; clauses.len()],
        &mut Vec::new(),
        &mut output,
    );
    output
}

#[test]
fn named_client_scope_and_endpoint_entries_are_order_independent() {
    let client_clauses = [
        quote!(base "https://example.test";),
        quote!(vars { tenant: String; }),
        quote!(timeout 3s;),
        quote!(headers { "X-Static" = "one"; }),
    ];
    for clauses in clause_permutations(&client_clauses) {
        parse(quote! {
            client Api { #clauses }
            GET read() -> Bytes;
        })
        .expect("client clause permutation");
    }

    let endpoint_clauses = [
        quote!(id "api.items.read";),
        quote!(path ["items"];),
        quote!(query { "kind" = "all"; }),
        quote!(headers { "X-Static" = "one"; }),
        quote!(timeout 2s;),
    ];
    for clauses in clause_permutations(&endpoint_clauses) {
        parse(quote! {
            client Api { base "https://example.test"; }
            GET read() -> Bytes { #clauses }
        })
        .expect("endpoint clause permutation");
    }

    let scope_entries = [
        quote!(path ["items"];),
        quote!(timeout 2s;),
        quote!(GET first() -> Bytes;),
        quote!(scope nested() { GET second() -> Bytes; }),
    ];
    for entries in clause_permutations(&scope_entries) {
        parse(quote! {
            client Api { base "https://example.test"; }
            scope items() { #entries }
        })
        .expect("scope clause/child permutation");
    }

    let auth = parse_ok(quote! {
        client Api { base "https://example.test"; }
        GET read() -> Bytes {
            auth bearer first;
            timeout 2s;
            auth query "fallback" = second;
            path ["items"];
        }
    });
    let SourceChild::Endpoint(endpoint) = &auth.children[0] else {
        panic!("endpoint")
    };
    let Some(AuthOperation::Modify { placements, .. }) = &endpoint.attachment_site.auth else {
        panic!("auth placements")
    };
    assert_eq!(canonical_ident(&placements[0].credential), "first");
    assert_eq!(canonical_ident(&placements[1].credential), "second");
}

#[test]
fn duplicate_named_clauses_report_duplicate_and_first_spans() {
    let error = parse(quote! {
        client Api { base "https://example.test"; }
        GET read() -> Bytes {
            timeout 1s;
            path ["items"];
            timeout 2s;
        }
    })
    .err()
    .expect("duplicate timeout");
    assert!(error.to_string().contains("duplicate `timeout` clause"));
    let diagnostic_tokens = error.into_compile_error().to_string();
    assert_eq!(diagnostic_tokens.matches("compile_error").count(), 2);
    assert!(diagnostic_tokens.contains("first `timeout` clause"));
}

#[test]
fn source_names_body_roles_optionality_and_stable_ids_follow_v2_syntax() {
    let api = parse_ok(quote! {
        client HTTPApi { base "https://example.test"; }
        scope http_status() {
            GET get_http_status(
                first: Option<String>,
                #[body] payload: Text,
                body: String,
                second: std::option::Option<u32>,
                alias_value: crate::MaybeString,
            ) -> Bytes {
                id "http.status_v2.get";
            }
            GET r#type() -> Bytes;
        }
    });
    let SourceChild::Scope(scope) = &api.children[0] else {
        panic!("scope")
    };
    let SourceChild::Endpoint(endpoint) = &scope.children[0] else {
        panic!("endpoint")
    };
    assert!(matches!(endpoint.parameters[0].kind, ValueKind::Optional));
    assert_eq!(endpoint.body.as_ref().unwrap().position, 1);
    assert!(matches!(endpoint.parameters[1].kind, ValueKind::Required));
    assert!(matches!(endpoint.parameters[2].kind, ValueKind::Optional));
    assert!(matches!(endpoint.parameters[3].kind, ValueKind::Required));
    assert_eq!(
        endpoint.contract_id.as_ref().unwrap().value.value(),
        "http.status_v2.get"
    );

    for (tokens, expected) in [
        (
            quote! {
                client Api { base "https://example.test"; }
                GET PascalCase() -> Bytes;
            },
            "must be snake_case",
        ),
        (
            quote! {
                client Api { base "https://example.test"; }
                scope PascalCase() { }
            },
            "must be snake_case",
        ),
        (
            quote! {
                client Api { base "https://example.test"; }
                POST duplicate(#[body] first: Text, #[body] second: Bytes) -> Bytes;
            },
            "two request body parameters",
        ),
        (
            quote! {
                client Api { base "https://example.test"; }
                POST malformed(#[body(value)] payload: Text) -> Bytes;
            },
            "bare `#[body]`",
        ),
        (
            quote! {
                client Api { base "https://example.test"; }
                POST defaulted(#[body] payload: Text = crate::DEFAULT) -> Bytes;
            },
            "cannot have a default",
        ),
    ] {
        let error = parse(tokens).err().expect("invalid v2 syntax");
        assert!(
            error.to_string().contains(expected),
            "expected `{expected}`, got `{error}`"
        );
    }
}

#[test]
fn stable_id_format_is_focused() {
    for invalid in [
        "",
        ".leading",
        "trailing.",
        "two..dots",
        "Upper.case",
        "-leading",
        "trailing-",
        "contains/slash",
    ] {
        let invalid = LitStr::new(invalid, proc_macro2::Span::call_site());
        let error = parse(quote! {
            client Api { base "https://example.test"; }
            GET read() -> Bytes { id #invalid; }
        })
        .err()
        .expect("invalid stable ID");
        assert!(error.to_string().contains("endpoint ID"));
    }
}
