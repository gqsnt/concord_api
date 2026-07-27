use quote::quote;

use super::model::{
    CloneEligibility, PaginationInitializer, PaginationValue, RequiredArgument,
    ResolvedChallengePolicy, ResolvedValue, ValueReference, ValueShape,
};
use super::*;
use crate::syntax as parse;
use crate::{MAX_AUTH_PLACEMENTS, MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_RULES_PER_PLAN};

fn resolve_ok(tokens: proc_macro2::TokenStream) -> Api {
    resolve(parse::parse(tokens).expect("AST")).expect("normalized IR")
}

#[test]
fn rate_limit_normalization_inherits_lexical_order_and_static_values() {
    let api = resolve_ok(quote! {
        client RateApi {
            base "https://example.test";
            rate_limit {
                500 / 10s;
                30000 / 10m;
            }
        }
        scope regions(region: String) {
            rate_limit {
                cost 2;
                20 / 1s;
            }
            GET read(id: String) -> Bytes {
                rate_limit {
                    5 / 1s;
                }
            }
            GET public() -> Bytes;
        }
    });
    let client = &api.client;
    assert!(client.requires_limiter);
    let client_rule = &client.config.rate_limits;
    assert_eq!(client_rule.len(), 1);
    assert_eq!(client_rule[0].id, "rate_api.rate_limit");
    assert_eq!(client_rule[0].origin, "client");
    assert_eq!(client_rule[0].cost, 1);
    assert_eq!(client_rule[0].windows.len(), 2);
    let Child::Scope(scope) = &client.children[0] else {
        panic!("scope")
    };
    assert_eq!(scope.config.rate_limits.len(), 2);
    assert_eq!(scope.config.rate_limits[0].id, "rate_api.rate_limit");
    assert_eq!(
        scope.config.rate_limits[1].id,
        "rate_api.regions.rate_limit"
    );
    assert_eq!(scope.config.rate_limits[1].origin, "scope regions");
    let Child::Endpoint(read) = &scope.children[0] else {
        panic!("read")
    };
    assert_eq!(read.config.rate_limits.len(), 3);
    assert_eq!(
        read.config.rate_limits[2].id,
        "rate_api.regions.read.rate_limit"
    );
    assert_eq!(read.config.rate_limits[2].cost, 1);
    assert_eq!(read.config.rate_limits[2].windows[0].capacity, 5);
    let Child::Endpoint(public) = &scope.children[1] else {
        panic!("public")
    };
    assert_eq!(public.config.rate_limits.len(), 2);
}

#[test]
fn rate_limit_normalization_does_not_resolve_request_values() {
    let api = resolve_ok(quote! {
        client Api {
            base "https://example.test";
        }
        scope regions(region: String) {
            GET read(item: String) -> Bytes {
                rate_limit { 1 / 1s; }
            }
        }
    });
    let Child::Scope(scope) = &api.client.children[0] else {
        panic!("scope")
    };
    let Child::Endpoint(endpoint) = &scope.children[0] else {
        panic!("endpoint")
    };
    assert_eq!(endpoint.config.rate_limits.len(), 1);
    assert!(
        endpoint.config.rate_limits[0]
            .id
            .ends_with(".read.rate_limit")
    );
    assert_eq!(
        endpoint.config.rate_limits[0].origin,
        "endpoint regions::read"
    );
}

#[test]
fn rate_limit_requires_direct_complete_declarations() {
    let syntax = parse::parse(quote! {
        client Api {
            base "https://example.test";
            rate_limit missing;
        }
    })
    .err()
    .expect("named declaration is removed");
    assert!(syntax.to_string().contains("direct `rate_limit { ... }`"));
}

#[test]
fn rate_limit_normalization_rejects_the_first_excess_effective_rule() {
    let mut source = String::from(
        r#"
            client RateApi {
                base "https://example.test";
                rate_limit { 1 / 1s; }
            }
        "#,
    );
    for _ in 1..MAX_RATE_LIMIT_RULES_PER_PLAN {
        source.push_str("scope s() { rate_limit { 1 / 1s; }");
    }
    source.push_str("GET read() -> Bytes { rate_limit { 1 / 1s; } }");
    for _ in 1..MAX_RATE_LIMIT_RULES_PER_PLAN {
        source.push('}');
    }
    let tokens = source
        .parse::<proc_macro2::TokenStream>()
        .expect("test token stream");
    let source = parse::parse(tokens).expect("source syntax");
    let error = resolve(source)
        .err()
        .expect("first excess effective rate-limit rule");

    assert!(
        error
            .to_string()
            .contains("an effective rate-limit plan permits at most 64 rules")
    );
}

#[test]
fn rate_limit_normalization_rejects_duplicate_effective_ids() {
    let source = parse::parse(quote! {
        client RateApi {
            base "https://example.test";
            rate_limit { 1 / 1s; }
        }
        GET read() -> Bytes {
            id "rate_api";
            rate_limit { 1 / 1s; }
        }
    })
    .expect("source syntax");
    let error = resolve(source)
        .err()
        .expect("duplicate effective rate-limit rule ID");

    assert!(
        error
            .to_string()
            .contains("an effective rate-limit plan contains a duplicate rule ID")
    );
}

#[test]
fn rate_limit_normalization_rejects_the_first_excess_id_byte() {
    const SUFFIX: &str = ".rate_limit";
    let client_name = format!(
        "R{}",
        "a".repeat(MAX_RATE_LIMIT_ID_BYTES + 1 - SUFFIX.len() - 1)
    );
    let tokens = format!(
        r#"
            client {client_name} {{
                base "https://example.test";
                rate_limit {{ 1 / 1s; }}
            }}
        "#
    )
    .parse::<proc_macro2::TokenStream>()
    .expect("test token stream");
    let source = parse::parse(tokens).expect("source syntax");
    let error = resolve(source)
        .err()
        .expect("first excess generated rate-limit ID byte");

    assert!(
        error
            .to_string()
            .contains("a rate-limit rule ID permits at most 256 UTF-8 bytes")
    );
}

#[test]
fn normalization_removes_layer_operations_and_preserves_order() {
    let api = resolve_ok(quote! {
        client ShapeApi {
            base "https://example.test/root";
            vars { tenant: String; }
            path ["v1"];
            query { "first" = "one"; "replace" = "old"; }
            headers { "X-First" = "one"; }
        }
        scope users(region: String) {
            path replace ["users", scope.region];
            query { remove "replace"; "second" = vars.tenant; }
            headers replace { "X-Second" = scope.region; }
            GET by_id(id: String) -> Bytes { path [ep.id]; timeout 2s; }
        }
    });
    let Child::Scope(scope) = &api.client.children[0] else {
        panic!("scope")
    };
    let Child::Endpoint(endpoint) = &scope.children[0] else {
        panic!("endpoint")
    };
    assert_eq!(endpoint.config.path.len(), 3);
    assert_eq!(
        endpoint
            .config
            .query
            .iter()
            .map(|entry| entry.key.value())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(endpoint.config.headers.len(), 1);
    assert_eq!(
        endpoint.config.timeout,
        Some(std::time::Duration::from_secs(2))
    );
}

#[test]
fn lexical_resolution_rejects_ambiguous_visible_scope_names() {
    let error = resolve(
        parse::parse(quote! {
            client Api {
                base "https://example.test";
            }
            scope outer(id: String) {
                scope inner(id: String) {
                    GET read() -> Bytes { path [scope.id]; }
                }
            }
        })
        .expect("AST"),
    )
    .err()
    .expect("ambiguous scope name");
    assert!(error.to_string().contains("ambiguous visible scope value"));
}

#[test]
fn endpoint_surfaces_include_final_paths_names_and_capabilities() {
    let api = resolve_ok(quote! {
        client ExampleApi {
            base "https://example.test";
        }
        scope regions(region: String) {
            scope users(tenant: String) {
                GET fetch(verbose: Option<bool>) -> Bytes;
            }
        }
    });
    let Child::Scope(regions) = &api.client.children[0] else {
        panic!()
    };
    let Child::Scope(users) = &regions.children[0] else {
        panic!()
    };
    let Child::Endpoint(endpoint) = &users.children[0] else {
        panic!()
    };
    assert_eq!(endpoint.surface.endpoint_type_name, "Fetch");
    assert_eq!(endpoint.surface.request_type_name, "FetchRequest");
    assert_eq!(
        endpoint
            .surface
            .module_path
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["regions", "users"]
    );
    assert_eq!(endpoint.surface.setters.len(), 1);
    assert_eq!(endpoint.surface.setters[0].setter, "set_verbose");
    assert_eq!(
        endpoint.surface.setters[0]
            .clear
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("clear_verbose")
    );
    assert!(endpoint.surface.setters[0].reset.is_none());
    assert!(endpoint.surface.terminals.response);
    assert!(!endpoint.surface.terminals.pagination);
    assert_eq!(endpoint.surface.clone_eligibility, CloneEligibility::Always);
    assert_eq!(
        endpoint.surface.docs_identity,
        "ExampleApi::regions::users::fetch"
    );
}

#[test]
fn normalization_resolves_replace_remove_off_and_type_shapes() {
    let api = resolve_ok(quote! {
        client LayerApi {
            base "https://example.test/fixed";
            host ["client"];
            path ["client"];
            query { "keep" = "client"; "change" = "client"; }
            headers { "X-Keep" = "client"; "X-Change" = "client"; }
            timeout 5s;
        }
        scope shaped() {
            host off;
            path replace ["scope"];
            query {
                "change" = "scope";
                "local" = "scope";
            }
            headers {
                "x-change" = "scope";
                "X-Local" = "scope";
            }
            timeout off;
            GET modified(
                scalar: String,
                optional: Option<String>,
                vector: Vec<String>,
                optional_vector: Option<Vec<String>>,
            ) -> Bytes;
            GET cleared() -> Bytes {
                host ["endpoint"];
                path off;
                query off;
                headers off;
                timeout 1ms;
            }
            GET replaced() -> Bytes {
                query replace { "only" = "endpoint"; }
                headers replace { "X-Only" = "endpoint"; }
            }
        }
    });
    let Child::Scope(scope) = &api.client.children[0] else {
        panic!("scope")
    };
    let Child::Endpoint(modified) = &scope.children[0] else {
        panic!("modified endpoint")
    };
    assert!(modified.config.host.is_empty());
    assert_eq!(modified.config.path.len(), 1);
    assert_eq!(
        modified
            .config
            .query
            .iter()
            .map(|entry| entry.key.value())
            .collect::<Vec<_>>(),
        ["keep", "change", "local"]
    );
    assert_eq!(
        modified
            .config
            .headers
            .iter()
            .map(|entry| entry.canonical_name.as_str())
            .collect::<Vec<_>>(),
        ["x-keep", "x-change", "x-local"]
    );
    assert_eq!(modified.config.timeout, None);
    assert!(matches!(modified.parameters[0].shape, ValueShape::Scalar));
    assert!(matches!(
        modified.parameters[1].shape,
        ValueShape::Option { .. }
    ));
    assert!(matches!(
        modified.parameters[2].shape,
        ValueShape::Vec { .. }
    ));
    assert!(matches!(
        modified.parameters[3].shape,
        ValueShape::OptionVec { .. }
    ));

    let Child::Endpoint(cleared) = &scope.children[1] else {
        panic!("cleared endpoint")
    };
    assert_eq!(cleared.config.host.len(), 1);
    assert!(cleared.config.path.is_empty());
    assert!(cleared.config.query.is_empty());
    assert!(cleared.config.headers.is_empty());
    assert_eq!(
        cleared.config.timeout,
        Some(std::time::Duration::from_millis(1))
    );

    let Child::Endpoint(replaced) = &scope.children[2] else {
        panic!("replaced endpoint")
    };
    assert_eq!(replaced.config.query[0].key.value(), "only");
    assert_eq!(replaced.config.query.len(), 1);
    assert_eq!(replaced.config.headers[0].canonical_name, "x-only");
    assert_eq!(replaced.config.headers.len(), 1);
}

#[test]
fn absence_and_default_capabilities_are_independent_and_syntactic() {
    let api = resolve_ok(quote! {
        client CapabilityApi {
            base "https://example.test";
            vars {
                plain: String;
                absence: Option<String>;
                defaulted: u32 = 7;
                both: Option<String> = None;
                both_vector: Option<Vec<String>> = None;
                opaque_option_alias: MaybeString;
            }
        }
        GET read() -> Bytes;
    });
    let [plain, absence, defaulted, both, both_vector, opaque] = api.client.values.as_slice()
    else {
        panic!("capability values")
    };
    let type_name = |ty: &syn::Type| quote!(#ty).to_string();

    assert!(!plain.accepts_absence());
    assert!(!plain.has_default());
    assert_eq!(type_name(plain.setter_argument_type()), "String");

    assert!(absence.is_optional());
    assert!(absence.accepts_absence());
    assert!(!absence.has_default());
    assert_eq!(type_name(absence.setter_argument_type()), "String");
    assert_eq!(type_name(absence.client_builder_storage_type()), "String");

    assert!(!defaulted.accepts_absence());
    assert!(defaulted.has_default());
    assert_eq!(type_name(defaulted.setter_argument_type()), "u32");

    assert!(!both.is_optional());
    assert!(both.accepts_absence());
    assert!(both.has_default());
    assert_eq!(type_name(both.setter_argument_type()), "String");
    assert_eq!(
        type_name(both.client_builder_storage_type()),
        "Option < String >"
    );

    assert!(both_vector.accepts_absence());
    assert!(both_vector.has_default());
    assert_eq!(
        type_name(both_vector.setter_argument_type()),
        "Vec < String >"
    );
    assert_eq!(
        type_name(both_vector.client_builder_storage_type()),
        "Option < Vec < String > >"
    );

    assert!(opaque.is_required());
    assert!(!opaque.accepts_absence());
    assert!(!opaque.has_default());
    assert_eq!(type_name(opaque.setter_argument_type()), "MaybeString");
}

#[test]
fn semantic_references_and_header_vector_use_fail_with_focused_diagnostics() {
    let unknown = resolve(
        parse::parse(quote! {
            client Api {
                base "https://example.test";
            }
            GET read() -> Bytes { path [ep.missing]; }
        })
        .expect("AST"),
    )
    .err()
    .expect("unknown endpoint value");
    assert!(
        unknown
            .to_string()
            .contains("unknown endpoint value `missing`")
    );

    let vector_header = resolve(
        parse::parse(quote! {
            client Api {
                base "https://example.test";
            }
            GET read(values: Vec<String>) -> Bytes {
                headers { "X-Values" = ep.values; }
            }
        })
        .expect("AST"),
    )
    .err()
    .expect("vector header");
    assert!(
        vector_header
            .to_string()
            .contains("do not accept vector values")
    );

    let secret_namespace = parse::parse(quote! {
        client Api {
            base "https://example.test";
        }
        GET read() -> Bytes { query { "token" = credentials.token; } }
    })
    .err()
    .expect("credential expression namespace");
    assert!(
        secret_namespace
            .to_string()
            .contains("secrets and credentials are not expressible")
    );
}

#[test]
fn authentication_normalization_appends_replaces_clears_and_resolves_defaults() {
    let api = resolve_ok(quote! {
        client AuthApi {
            base "https://example.test";
            credentials {
                secret key_value: String;
                secret bearer_value: String;
                credential key = api_key(secret.key_value);
                credential bearer = bearer(secret.bearer_value);
                credential rotating = custom crate::RotatingProvider;
                credential acquired = endpoint login;
                credential custom = custom crate::SessionProvider;
            }
            auth header "X-Api-Key" = key;
        }
        GET login() -> Bytes;
        scope secured() {
            auth bearer rotating challenge unauthorized;
            GET inherited() -> Bytes;
            GET replaced() -> Bytes {
                auth replace bearer bearer;
            }
            GET custom() -> Bytes {
                auth replace header "X-Session" = custom challenge never;
            }
            GET public() -> Bytes { auth off; }
        }
    });
    let client = &api.client;
    assert_eq!(client.config.auth.len(), 1);
    assert_eq!(
        client.config.auth[0].challenge,
        ResolvedChallengePolicy::Never
    );
    let Child::Scope(scope) = &client.children[1] else {
        panic!("secured scope")
    };
    assert_eq!(scope.config.auth.len(), 2);
    assert_eq!(
        scope.config.auth[0].challenge,
        ResolvedChallengePolicy::Never
    );
    assert_eq!(
        scope.config.auth[1].challenge,
        ResolvedChallengePolicy::Unauthorized
    );
    let Child::Endpoint(inherited) = &scope.children[0] else {
        panic!("inherited endpoint")
    };
    assert_eq!(inherited.config.auth.len(), 2);
    let Child::Endpoint(replaced) = &scope.children[1] else {
        panic!("replaced endpoint")
    };
    assert_eq!(replaced.config.auth.len(), 1);
    assert_eq!(
        replaced.config.auth[0].challenge,
        ResolvedChallengePolicy::Never
    );
    let Child::Endpoint(custom) = &scope.children[2] else {
        panic!("custom endpoint")
    };
    assert_eq!(custom.config.auth.len(), 1);
    assert_eq!(
        custom.config.auth[0].challenge,
        ResolvedChallengePolicy::Never
    );
    let Child::Endpoint(public) = &scope.children[3] else {
        panic!("public endpoint")
    };
    assert!(public.config.auth.is_empty());
}

#[test]
fn generated_authentication_plan_accepts_exact_limit_and_rejects_first_excess_after_inheritance() {
    fn placements(start: usize, end: usize) -> Vec<proc_macro2::TokenStream> {
        (start..end)
            .map(|index| {
                let name =
                    syn::LitStr::new(&format!("auth-{index}"), proc_macro2::Span::call_site());
                quote!(auth query #name = key;)
            })
            .collect()
    }

    let exact = placements(0, MAX_AUTH_PLACEMENTS);
    let api = resolve_ok(quote! {
        client ExactAuthApi {
            base "https://example.test";
            credentials {
                secret key_value: String;
                credential key = api_key(secret.key_value);
            }
            #(#exact)*
        }
        GET read() -> Bytes;
    });
    assert_eq!(api.client.config.auth.len(), MAX_AUTH_PLACEMENTS);

    let client = placements(0, MAX_AUTH_PLACEMENTS / 2);
    let scope = placements(MAX_AUTH_PLACEMENTS / 2, MAX_AUTH_PLACEMENTS);
    let error = resolve(
        parse::parse(quote! {
            client InheritedAuthApi {
                base "https://example.test";
                credentials {
                    secret key_value: String;
                    credential key = api_key(secret.key_value);
                }
                #(#client)*
            }
            scope secured() {
                #(#scope)*
                GET first_excess() -> Bytes {
                    auth query "auth-first-excess" = key;
                }
            }
        })
        .expect("first-excess authentication AST"),
    )
    .err()
    .expect("first-excess effective authentication plan");
    assert_eq!(
        error.to_string(),
        format!(
            "an effective authentication plan permits at most {MAX_AUTH_PLACEMENTS} placements"
        )
    );
}

#[test]
fn authentication_semantics_reject_unknown_incompatible_ambiguous_and_colliding_plans() {
    fn error(tokens: proc_macro2::TokenStream) -> String {
        match parse::parse(tokens) {
            Ok(source) => resolve(source).err().expect("semantic error").to_string(),
            Err(error) => error.to_string(),
        }
    }

    let unknown = error(quote! {
        client Api {
            base "https://example.test";
        }
        GET read() -> Bytes { auth bearer missing; }
    });
    assert!(unknown.contains("unknown credential `missing`"));

    let incompatible = error(quote! {
        client Api {
            base "https://example.test";
            credentials {
                secret key_value: String;
                credential key = api_key(secret.key_value);
            }
        }
        GET read() -> Bytes { auth bearer key; }
    });
    assert!(incompatible.contains("incompatible"));

    let custom = error(quote! {
        client Api {
            base "https://example.test";
            credentials { credential session = custom crate::SessionProvider; }
        }
        GET read() -> Bytes { auth bearer session; }
    });
    assert!(custom.contains("requires an explicit challenge suffix"));

    let challengers = error(quote! {
        client Api {
            base "https://example.test";
            credentials {
                secret first_value: String;
                secret second_value: String;
                credential first = bearer(secret.first_value);
                credential second = api_key(secret.second_value);
            }
        }
        GET read() -> Bytes {
            auth bearer first challenge unauthorized;
            auth query "key" = second challenge unauthorized_or_forbidden;
        }
    });
    assert!(challengers.contains("at most one challenge-enabled"));

    let authorization = error(quote! {
        client Api {
            base "https://example.test";
            credentials {
                secret token: String;
                credential bearer = bearer(secret.token);
            }
            headers { "Authorization" = "public"; }
        }
        GET read() -> Bytes { auth bearer bearer; }
    });
    assert!(authorization.contains("reserved by transport, authentication"));

    let header = error(quote! {
        client Api {
            base "https://example.test";
            credentials {
                secret key_value: String;
                credential key = api_key(secret.key_value);
            }
            headers { "X-Key" = "public"; }
        }
        GET read() -> Bytes { auth header "x-key" = key; }
    });
    assert!(header.contains("collides"));

    let query = error(quote! {
        client Api {
            base "https://example.test";
            credentials {
                secret key_value: String;
                credential key = api_key(secret.key_value);
            }
            query { "key" = "public"; }
        }
        GET read() -> Bytes { auth query "key" = key; }
    });
    assert!(query.contains("collides"));

    let duplicate_auth = error(quote! {
        client Api {
            base "https://example.test";
            credentials {
                secret first_value: String;
                secret second_value: String;
                credential first = api_key(secret.first_value);
                credential second = api_key(secret.second_value);
            }
        }
        GET read() -> Bytes {
            auth header "X-Key" = first;
            auth header "x-key" = second;
        }
    });
    assert!(duplicate_auth.contains("collides"));
}

#[test]
fn optional_public_values_defer_authentication_material_collision_to_request_preparation() {
    resolve_ok(quote! {
        client Api {
            base "https://example.test";
            credentials {
                secret key_value: String;
                credential key = api_key(secret.key_value);
            }
        }
        GET optional(
            query_value: Option<String>,
            header_value: Option<String>,
        ) -> Bytes {
            query { "key" = ep.query_value; }
            headers { "X-Key" = ep.header_value; }
            auth query "key" = key;
            auth header "X-Key" = key;
        }
    });
}

#[test]
fn endpoint_backed_dependency_and_authentication_cycles_are_rejected() {
    fn error(tokens: proc_macro2::TokenStream) -> String {
        resolve(parse::parse(tokens).expect("source AST"))
            .err()
            .expect("cycle")
            .to_string()
    }

    let direct = error(quote! {
        client Api {
            base "https://example.test";
            credentials { credential session = endpoint login; }
        }
        GET login() -> Bytes {
            auth bearer session challenge never;
        }
    });
    assert!(direct.contains("dependency or authentication cycle"));

    let mutual = error(quote! {
        client Api {
            base "https://example.test";
            credentials {
                credential first = endpoint first;
                credential second = endpoint second;
            }
        }
        GET first() -> Bytes {
            auth bearer second challenge never;
        }
        GET second() -> Bytes {
            auth bearer first challenge never;
        }
    });
    assert!(mutual.contains("dependency or authentication cycle"));
}

#[test]
fn generated_name_collisions_cover_request_types_and_inherent_methods() {
    let retry_method = resolve(
        parse::parse(quote! {
            client Api {
                base "https://example.test";
            }
            GET r#retry() -> Bytes;
        })
        .expect("AST"),
    )
    .err()
    .expect("generated retry method collision");
    assert!(retry_method.to_string().contains("retry"));
    assert!(retry_method.to_string().contains("reserved Concord method"));

    let trait_item = resolve(
        parse::parse(quote! {
            client Api {
                base "https://example.test";
            }
            GET read(value: Option<String>) -> Bytes;
            GET read_request() -> Bytes;
        })
        .expect("AST"),
    )
    .err()
    .expect("request wrapper collision");
    assert!(trait_item.to_string().contains("collides"));

    let request_method = resolve_ok(quote! {
        client Api {
            base "https://example.test";
        }
        GET read(timeout: Option<u64>) -> Bytes;
    });
    let request_method = test_support::endpoint(&request_method.client.children, "read");
    assert_eq!(request_method.surface.setters[0].setter, "set_timeout");

    let helper_names = resolve_ok(quote! {
        client Api {
            base "https://example.test";
        }
        GET read(
            value: Option<String>,
            clear_value: Option<String>,
            limit: u32 = 25,
            reset_limit: Option<bool>,
        ) -> Bytes;
    });
    let helper_names = test_support::endpoint(&helper_names.client.children, "read");
    assert_eq!(
        helper_names
            .surface
            .setters
            .iter()
            .map(|setter| setter.setter.to_string())
            .collect::<Vec<_>>(),
        [
            "set_value",
            "set_clear_value",
            "set_limit",
            "set_reset_limit"
        ]
    );

    let child_setter = resolve(
        parse::parse(quote! {
            client Api {
                base "https://example.test";
            }
            scope resources(region: Option<String>) {
                GET set_region() -> Bytes;
            }
        })
        .expect("AST"),
    )
    .err()
    .expect("child method colliding with set-prefixed scope control");
    assert!(child_setter.to_string().contains("set_region"));

    for helper in ["clear_label", "reset_label"] {
        let helper = syn::Ident::new(helper, proc_macro2::Span::call_site());
        let collision = resolve(
            parse::parse(quote! {
                client Api {
                    base "https://example.test";
                }
                scope resources(label: Option<String> = None) {
                    GET #helper() -> Bytes;
                }
            })
            .expect("AST"),
        )
        .err()
        .expect("child method colliding with a defaulted Option helper");
        assert!(collision.to_string().contains(&helper.to_string()));
        assert!(collision.to_string().contains("collides"));
    }

    let scope_type = resolve(
        parse::parse(quote! {
            client Api {
                base "https://example.test";
            }
            scope resources() {
                GET resources() -> Bytes;
            }
        })
        .expect("AST"),
    )
    .err()
    .expect("scope facade type collision");
    assert!(scope_type.to_string().contains("Resources"));

    let endpoint_trait = resolve(
        parse::parse(quote! {
            client Api {
                base "https://example.test";
            }
            GET endpoint() -> Bytes;
        })
        .expect("AST"),
    )
    .err()
    .expect("endpoint trait collision");
    assert!(endpoint_trait.to_string().contains("Endpoint"));

    let storage_terminal = resolve_ok(quote! {
        client Api {
            base "https://example.test";
            credentials {
                credential session = endpoint source;
            }
        }
        GET source(store_as_session: Option<bool>) -> Bytes;
    });
    let storage_terminal = test_support::endpoint(&storage_terminal.client.children, "source");
    assert_eq!(
        storage_terminal.surface.setters[0].setter,
        "set_store_as_session"
    );
    assert_eq!(
        storage_terminal.surface.terminals.credential_storage[0].method,
        "store_as_session"
    );
}

#[test]
fn authentication_generated_names_cover_builder_accessor_and_acquisition_namespaces() {
    fn error(tokens: proc_macro2::TokenStream) -> String {
        resolve(parse::parse(tokens).expect("source AST"))
            .err()
            .expect("generated name collision")
            .to_string()
    }

    let reserved_secret = error(quote! {
        client Api {
            base "https://example.test";
            credentials {
                secret build: String;
                credential key = api_key(secret.build);
            }
        }
    });
    assert!(reserved_secret.contains("build"));
    assert!(reserved_secret.contains("collides"));

    let helper_secret = error(quote! {
        client Api {
            base "https://example.test";
            vars { token: Option<String>; }
            credentials {
                secret clear_token: String;
                credential key = api_key(secret.clear_token);
            }
        }
    });
    assert!(helper_secret.contains("clear_token"));
    assert!(helper_secret.contains("collides"));

    let reset_secret = error(quote! {
        client Api {
            base "https://example.test";
            vars { limit: u32 = 25; }
            credentials {
                secret reset_limit: String;
                credential key = api_key(secret.reset_limit);
            }
        }
    });
    assert!(reset_secret.contains("reset_limit"));
    assert!(reset_secret.contains("collides"));

    let defaulted_option_clear_secret = error(quote! {
        client Api {
            base "https://example.test";
            vars { label: Option<String> = None; }
            credentials {
                secret clear_label: String;
                credential key = api_key(secret.clear_label);
            }
        }
    });
    assert!(defaulted_option_clear_secret.contains("clear_label"));
    assert!(defaulted_option_clear_secret.contains("collides"));

    let defaulted_option_reset_secret = error(quote! {
        client Api {
            base "https://example.test";
            vars { label: Option<String> = None; }
            credentials {
                secret reset_label: String;
                credential key = api_key(secret.reset_label);
            }
        }
    });
    assert!(defaulted_option_reset_secret.contains("reset_label"));
    assert!(defaulted_option_reset_secret.contains("collides"));

    let internal_accessor = error(quote! {
        client Api {
            base "https://example.test";
            credentials {
                secret value: String;
                credential __concord_new = api_key(secret.value);
            }
        }
    });
    assert!(internal_accessor.contains("__concord_new"));
    assert!(internal_accessor.contains("collides"));

    let set_prefixed_secret = error(quote! {
        client Api {
            base "https://example.test";
            vars { region: String; }
            credentials {
                secret set_region: String;
                credential key = api_key(secret.set_region);
            }
        }
    });
    assert!(set_prefixed_secret.contains("set_region"));
    assert!(set_prefixed_secret.contains("collides"));

    let request_wrapper = error(quote! {
        client Api {
            base "https://example.test";
            credentials { credential session = endpoint read; }
        }
        GET read() -> Bytes;
        GET read_request() -> Bytes;
    });
    assert!(request_wrapper.contains("collides"));
}

#[test]
fn pagination_normalizes_one_controller_type_and_field_initializer_ir() {
    let api = resolve_ok(quote! {
        client Api {
            base "https://example.test";
            vars { tenant: String; }
        }
        GET offset(offset: u64, limit: u32) -> PageSpec<Vec<u32>> {
            paginate concord_core::contracts::OffsetController {
                offset: bind ep.offset,
                page_size: bind ep.limit,
            }
        }
        GET vendor(cursor: Option<String>) -> PageSpec<Vec<u32>> {
            paginate crate::Vendor<String> {
                cursor: bind ep.cursor,
                tenant: vars.tenant,
                attempts: 0,
            }
        }
    });
    let endpoints = &api.client.children;
    let Child::Endpoint(offset) = &endpoints[0] else {
        panic!("offset endpoint");
    };
    let pagination = offset.pagination.as_ref().expect("offset pagination");
    assert_eq!(pagination.fields.len(), 2);
    assert!(matches!(
        pagination.fields[0].initializer,
        PaginationInitializer::Bound {
            value: ResolvedValue {
                reference: ValueReference::Endpoint(0),
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        pagination.fields[1].initializer,
        PaginationInitializer::Bound {
            value: ResolvedValue {
                reference: ValueReference::Endpoint(1),
                ..
            },
            ..
        }
    ));

    let Child::Endpoint(vendor) = &endpoints[1] else {
        panic!("vendor endpoint");
    };
    let pagination = vendor.pagination.as_ref().expect("vendor pagination");
    assert_eq!(pagination.fields.len(), 3);
    assert!(matches!(
        pagination.fields[0].initializer,
        PaginationInitializer::Bound {
            value: ResolvedValue {
                reference: ValueReference::Endpoint(0),
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        pagination.fields[1].initializer,
        PaginationInitializer::Value(PaginationValue::Reference(ResolvedValue {
            reference: ValueReference::Client(0),
            ..
        }))
    ));
    assert!(matches!(
        pagination.fields[2].initializer,
        PaginationInitializer::Value(PaginationValue::Stable(_))
    ));
}

#[test]
fn pagination_resolution_validates_destination_ownership_not_controller_semantics() {
    let api = resolve_ok(quote! {
        client Api {
            base "https://example.test";
        }
        GET read(offset: String, limit: bool) -> PageSpec<Vec<u32>> {
            paginate crate::ArbitraryController {
                anything: bind ep.offset,
                another: bind ep.limit,
            }
        }
    });
    let Child::Endpoint(endpoint) = &api.client.children[0] else {
        panic!("endpoint");
    };
    assert_eq!(
        endpoint
            .pagination
            .as_ref()
            .expect("pagination")
            .fields
            .len(),
        2
    );

    let undefined = resolve(
        parse::parse(quote! {
            client Api {
                base "https://example.test";
            }
            GET read(offset: u64) -> PageSpec<Vec<u32>> {
                paginate crate::Controller {
                    state: bind ep.missing,
                }
            }
        })
        .expect("source AST"),
    )
    .err()
    .expect("undefined field");
    assert!(
        undefined
            .to_string()
            .contains("unknown endpoint value `missing`")
    );

    let body_binding = resolve(
        parse::parse(quote! {
            client Api {
                base "https://example.test";
            }
            POST read(
                offset: u64,
                #[body] filter: Text,
            ) -> PageSpec<Vec<u32>> {
                paginate crate::Controller {
                    state: bind ep.filter,
                }
            }
        })
        .expect("source AST"),
    )
    .err()
    .expect("request body must not be a pagination binding destination");
    assert!(
        body_binding
            .to_string()
            .contains("unknown endpoint value `filter`")
    );
}

#[test]
fn v2_names_required_argument_order_and_syntactic_optionality_are_resolved_once() {
    let api = resolve_ok(quote! {
        client HTTPApi { base "https://example.test"; }
        scope http_status() {
            GET get_http_status(
                first: String,
                #[body] payload: Text,
                optional: std::option::Option<u32>,
                second: bool,
                alias_value: crate::MaybeString,
            ) -> Bytes;
            GET r#type() -> Bytes;
        }
    });
    let Child::Scope(scope) = &api.client.children[0] else {
        panic!("scope")
    };
    assert_eq!(scope.surface.method_name, "http_status");
    assert_eq!(scope.surface.facade_name, "HttpStatus");
    let Child::Endpoint(endpoint) = &scope.children[0] else {
        panic!("endpoint")
    };
    assert_eq!(endpoint.surface.method_name, "get_http_status");
    assert_eq!(endpoint.surface.endpoint_type_name, "GetHttpStatus");
    assert_eq!(
        endpoint.surface.required_arguments,
        [
            RequiredArgument::Parameter(0),
            RequiredArgument::Body,
            RequiredArgument::Parameter(2),
            RequiredArgument::Parameter(3),
        ]
    );
    assert!(endpoint.parameters[1].is_optional());
    assert!(endpoint.parameters[3].is_required());
    assert_eq!(
        canonical_ident(&endpoint.body.as_ref().unwrap().name),
        "payload"
    );

    let Child::Endpoint(raw) = &scope.children[1] else {
        panic!("raw endpoint")
    };
    assert_eq!(raw.surface.method_name.to_string(), "r#type");
    assert_eq!(raw.surface.endpoint_type_name, "Type");
}

#[test]
fn explicit_and_default_contract_ids_are_stable_unique_and_rate_authoritative() {
    let api = resolve_ok(quote! {
        client RiotApi {
            base "https://example.test";
            rate_limit { 100 / 1m; }
        }
        scope match_v5() {
            rate_limit { 50 / 1m; }
            GET match_by_id() -> Bytes {
                id "riot.match_v5.match_by_id";
                rate_limit { 10 / 1m; }
            }
            GET timeline_by_id() -> Bytes;
        }
    });
    let Child::Scope(scope) = &api.client.children[0] else {
        panic!("scope")
    };
    let Child::Endpoint(explicit) = &scope.children[0] else {
        panic!("explicit endpoint")
    };
    assert_eq!(explicit.contract_id.value, "riot.match_v5.match_by_id");
    assert!(explicit.contract_id.explicit);
    assert_eq!(
        explicit
            .config
            .rate_limits
            .iter()
            .map(|rule| rule.id.as_str())
            .collect::<Vec<_>>(),
        [
            "riot_api.rate_limit",
            "riot_api.match_v5.rate_limit",
            "riot.match_v5.match_by_id.rate_limit",
        ]
    );
    let Child::Endpoint(defaulted) = &scope.children[1] else {
        panic!("default endpoint")
    };
    assert_eq!(
        defaulted.contract_id.value,
        "riot_api.match_v5.timeline_by_id"
    );
    assert!(!defaulted.contract_id.explicit);

    for endpoint_name in [quote!(renamed_once), quote!(renamed_twice)] {
        let renamed = resolve_ok(quote! {
            client OtherRustName { base "https://example.test"; }
            GET #endpoint_name() -> Bytes {
                id "stable.remote.operation";
            }
        });
        let Child::Endpoint(endpoint) = &renamed.client.children[0] else {
            panic!("renamed endpoint")
        };
        assert_eq!(endpoint.contract_id.value, "stable.remote.operation");
    }

    let duplicate = resolve(
        parse::parse(quote! {
            client Api { base "https://example.test"; }
            GET first() -> Bytes { id "duplicate.operation"; }
            GET second() -> Bytes { id "duplicate.operation"; }
        })
        .expect("source AST"),
    )
    .err()
    .expect("duplicate contract ID");
    assert!(duplicate.to_string().contains("not unique"));
    assert_eq!(
        duplicate
            .into_compile_error()
            .to_string()
            .matches("compile_error")
            .count(),
        2
    );
}
