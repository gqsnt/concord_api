use std::time::Duration;

use quote::quote;

use super::model::{
    CloneEligibility, PaginationInitializer, PaginationValue, RequiredArgument,
    ResolvedAuthPlacement, ResolvedChallengePolicy, ResolvedValue, ValuePresence, ValueReference,
};
use super::*;
use crate::syntax as parse;
use crate::{MAX_AUTH_PLACEMENTS, MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_POLICIES_PER_PLAN};

fn resolve_ok(tokens: proc_macro2::TokenStream) -> Api {
    resolve(parse::parse(tokens).expect("AST")).expect("normalized IR")
}

#[test]
fn endpoint_status_effect_defaults_and_pagination_gate_are_semantic_data() {
    let api = resolve_ok(quote! {
        client PolicyApi {
            timeout 10s;
            base "https://example.test";
            GET read() -> Bytes via Binary;
            PUT replace() -> Bytes via Binary;
            POST create() -> Bytes via Binary;
            POST lookup() -> Vec<u8> via PageCodec {
                effect safe;
                status [200..=202, 206];
                paginate via Controller {}
            }
        }
    });
    let read = test_support::endpoint(&api.client.children, "read");
    let replace = test_support::endpoint(&api.client.children, "replace");
    let create = test_support::endpoint(&api.client.children, "create");
    let lookup = test_support::endpoint(&api.client.children, "lookup");
    assert_eq!(read.effect, Effect::Safe);
    assert_eq!(replace.effect, Effect::Idempotent);
    assert_eq!(create.effect, Effect::NonIdempotent);
    assert_eq!(lookup.effect, Effect::Safe);
    assert_eq!(
        lookup.accepted_status,
        AcceptedStatus::Codes(vec![200, 201, 202, 206])
    );
    assert!(lookup.status_explicit);
    assert!(lookup.effect_explicit);

    let error = test_support::try_resolve_tokens(quote! {
        client InvalidApi {
            timeout 10s;
            base "https://example.test";
            POST pages() -> Vec<u8> via PageCodec {
                paginate via Controller {}
            }
        }
    })
    .err()
    .expect("non-safe pagination");
    assert!(error.to_string().contains("effect `safe`"));
}

#[test]
fn rate_limit_normalization_inherits_lexical_order_and_static_values() {
    let api = resolve_ok(quote! {
        client RateApi {
            timeout 30s;
            base "https://example.test";
            rate_limit { id "concord.tests.1"; key [origin]; cost 1;
                allow 500 per 10s;
                allow 30000 per 10m;
            }

        scope regions(region: String) {
            rate_limit { id "concord.tests.2"; key [origin];
                cost 2;
                allow 20 per 1s;
            }
            GET read(id: String) -> Bytes via Binary {
                rate_limit { id "concord.tests.3"; key [origin]; cost 1;
                    allow 5 per 1s;
                }
            }
            GET public() -> Bytes via Binary;
        }
    }});
    let client = &api.client;
    assert!(client.requires_limiter);
    let client_rule = &client.config.rate_limits;
    assert_eq!(client_rule.len(), 1);
    assert_eq!(client_rule[0].id, "concord.tests.1");
    assert_eq!(client_rule[0].origin, "client");
    assert_eq!(client_rule[0].cost, 1);
    assert_eq!(client_rule[0].windows.len(), 2);
    let Child::Scope(scope) = &client.children[0] else {
        panic!("scope")
    };
    assert_eq!(scope.config.rate_limits.len(), 2);
    assert_eq!(scope.config.rate_limits[0].id, "concord.tests.1");
    assert_eq!(scope.config.rate_limits[1].id, "concord.tests.2");
    assert_eq!(scope.config.rate_limits[1].origin, "scope regions");
    let Child::Endpoint(read) = &scope.children[0] else {
        panic!("read")
    };
    assert_eq!(read.config.rate_limits.len(), 3);
    assert_eq!(read.config.rate_limits[2].id, "concord.tests.3");
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
            timeout 30s;
            base "https://example.test";

        scope regions(region: String) {
            GET read(item: String) -> Bytes via Binary {
                rate_limit { id "concord.tests.4"; key [origin]; cost 1;  allow 1 per 1s; }
            }
        }
    }});
    let Child::Scope(scope) = &api.client.children[0] else {
        panic!("scope")
    };
    let Child::Endpoint(endpoint) = &scope.children[0] else {
        panic!("endpoint")
    };
    assert_eq!(endpoint.config.rate_limits.len(), 1);
    assert_eq!(endpoint.config.rate_limits[0].id, "concord.tests.4");
    assert_eq!(
        endpoint.config.rate_limits[0].origin,
        "endpoint regions::read"
    );
}

#[test]
fn rate_limit_requires_a_complete_inline_or_named_attachment() {
    let syntax = parse::parse(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            rate_limit missing;
        }
    })
    .err()
    .expect("incomplete attachment");
    let message = syntax.to_string();
    assert!(message.contains("named rate-limit attachment"), "{message}");
}

#[test]
fn rate_limit_normalization_rejects_the_first_excess_effective_rule() {
    let mut source = String::from(
        r#"
            client RateApi {
                timeout 30s;
                base "https://example.test";
                rate_limit { key [origin]; cost 1;  id "p.0"; allow 1 per 1s; }
        "#,
    );
    for index in 1..MAX_RATE_LIMIT_POLICIES_PER_PLAN {
        source.push_str(&format!(
            "scope s{index}() {{ rate_limit {{ id \"p.{index}\"; key [origin]; cost 1; allow 1 per 1s; }}"
        ));
    }
    source.push_str(&format!("GET read() -> Bytes via Binary {{ rate_limit {{ id \"p.{MAX_RATE_LIMIT_POLICIES_PER_PLAN}\"; key [origin]; cost 1; allow 1 per 1s; }} }}"));
    for _ in 1..MAX_RATE_LIMIT_POLICIES_PER_PLAN {
        source.push('}');
    }
    source.push('}');
    let tokens = source
        .parse::<proc_macro2::TokenStream>()
        .expect("test token stream");
    let source = parse::parse(tokens).expect("source syntax");
    let error = resolve(source)
        .err()
        .expect("first excess effective rate-limit policy");

    let message = error.to_string();
    assert!(
        message.contains("an effective rate-limit plan permits at most 64 policies"),
        "{message}"
    );
}

#[test]
fn rate_limit_normalization_rejects_duplicate_effective_ids() {
    let source = parse::parse(quote! {
        client RateApi {
            timeout 30s;
            base "https://example.test";
            rate_limit { id "shared.policy"; key [origin]; cost 1;  allow 1 per 1s; }

        GET read() -> Bytes via Binary {
            id "rate_api";
            rate_limit { id "shared.policy"; key [origin]; cost 1;  allow 1 per 1s; }
        }
    }})
    .expect("source syntax");
    let error = resolve(source)
        .err()
        .expect("duplicate effective rate-limit policy ID");

    assert!(
        error
            .to_string()
            .contains("an effective rate-limit plan contains a duplicate policy ID")
    );
}

#[test]
fn rate_limit_syntax_rejects_the_first_excess_stable_id_byte() {
    let policy_id = "a".repeat(MAX_RATE_LIMIT_ID_BYTES + 1);
    let tokens = format!(
        r#"
            client RateApi {{
                timeout 30s;
                base "https://example.test";
                rate_limit {{ id "{policy_id}"; key [origin]; cost 1; allow 1 per 1s; }}
            }}
        "#
    )
    .parse::<proc_macro2::TokenStream>()
    .expect("test token stream");
    let error = parse::parse(tokens)
        .err()
        .expect("first excess stable rate-policy ID byte");

    assert!(
        error
            .to_string()
            .contains("a stable rate-policy ID must be 1-256 ASCII bytes")
    );
}

#[test]
fn normalization_removes_attachment_site_operations_and_preserves_order() {
    let api = resolve_ok(quote! {
        client ShapeApi {
            timeout 30s;
            base "https://example.test/root";
            values { tenant: String; }
            path ["v1"];
            query { "first" = "one"; "replace" = "old"; }
            headers { "X-First" = "one"; }

        scope users(region: String) {
            path replace ["users", users.region];
            query { remove "replace"; "second" = client.tenant; }
            headers replace { "X-Second" = users.region; }
            GET by_id(id: String) -> Bytes via Binary { path [endpoint.id]; timeout 2s; }
        }
    }});
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
                timeout 30s;
                base "https://example.test";

            scope branch(id: String) {
                scope branch(id: String) {
                    GET read() -> Bytes via Binary { path [branch.id]; }
                }
            }
        }})
        .expect("AST"),
    )
    .err()
    .expect("ambiguous scope name");
    assert!(error.to_string().contains("duplicates an ancestor"));
}

#[test]
fn endpoint_surfaces_include_final_paths_names_and_capabilities() {
    let api = resolve_ok(quote! {
        client ExampleApi {
            timeout 30s;
            base "https://example.test";

        scope regions(region: String) {
            scope users(tenant: String) {
                GET fetch(verbose?: bool) -> Bytes via Binary;
            }
        }
    }});
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
            .omit
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("omit_verbose")
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
fn normalization_resolves_replace_remove_off_and_explicit_presence() {
    let api = resolve_ok(quote! {
        client AttachmentApi {
            base "https://example.test/fixed";
            subdomain ["client"];
            path ["client"];
            query { "keep" = "client"; "change" = "client"; }
            headers { "X-Keep" = "client"; "X-Change" = "client"; }
            timeout 5s;

        scope shaped() {
            subdomain off;
            path replace ["scope"];
            query {
                "change" = "scope";
                "local" = "scope";
            }
            headers {
                "x-change" = "scope";
                "X-Local" = "scope";
            }
            timeout 30s;
            GET modified(
                scalar: String,
                optional?: String,
                vector: Vec<String>,
                optional_vector?: Vec<String>,
            ) -> Bytes via Binary;
            GET cleared() -> Bytes via Binary {
                subdomain ["endpoint"];
                path off;
                query off;
                headers off;
                timeout 1ms;
            }
            GET replaced() -> Bytes via Binary {
                query replace { "only" = "endpoint"; }
                headers replace { "X-Only" = "endpoint"; }
            }
        }
    }});
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
    assert_eq!(modified.config.timeout, Some(Duration::from_secs(30)));
    assert!(matches!(
        modified.parameters[0].presence,
        ValuePresence::Required
    ));
    assert!(matches!(
        modified.parameters[1].presence,
        ValuePresence::Optional
    ));
    assert!(matches!(
        modified.parameters[2].presence,
        ValuePresence::Required
    ));
    assert!(matches!(
        modified.parameters[3].presence,
        ValuePresence::Optional
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
            timeout 30s;
            base "https://example.test";
            values {
                plain: String;
                absence?: String;
                defaulted: u32 = 7;
                both?: Option<String> = None;
                both_vector?: Option<Vec<String>> = None;
                opaque_option_alias: MaybeString;
            }

        GET read() -> Bytes via Binary;
    }});
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
    assert_eq!(
        type_name(absence.setter_argument_type()),
        ":: std :: option :: Option < String >"
    );
    assert_eq!(
        type_name(absence.client_builder_storage_type()),
        ":: std :: option :: Option < String >"
    );
    assert_eq!(type_name(absence.with_argument_type()), "String");

    assert!(!defaulted.accepts_absence());
    assert!(defaulted.has_default());
    assert_eq!(type_name(defaulted.setter_argument_type()), "u32");

    assert!(both.is_optional());
    assert!(both.accepts_absence());
    assert!(both.has_default());
    assert_eq!(
        type_name(both.setter_argument_type()),
        ":: std :: option :: Option < Option < String > >"
    );
    assert_eq!(type_name(both.with_argument_type()), "Option < String >");
    assert_eq!(
        type_name(both.client_builder_storage_type()),
        ":: std :: option :: Option < Option < String > >"
    );

    assert!(both_vector.accepts_absence());
    assert!(both_vector.has_default());
    assert_eq!(
        type_name(both_vector.setter_argument_type()),
        ":: std :: option :: Option < Option < Vec < String > > >"
    );
    assert_eq!(
        type_name(both_vector.client_builder_storage_type()),
        ":: std :: option :: Option < Option < Vec < String > > >"
    );

    assert!(opaque.is_required());
    assert!(!opaque.accepts_absence());
    assert!(!opaque.has_default());
    assert_eq!(type_name(opaque.setter_argument_type()), "MaybeString");
}

#[test]
fn semantic_references_fail_focused_and_header_payload_types_remain_opaque() {
    let unknown = resolve(
        parse::parse(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";

            GET read() -> Bytes via Binary { path [endpoint.missing]; }
        }})
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
                timeout 30s;
                base "https://example.test";

            GET read(values: Vec<String>) -> Bytes via Binary {
                headers { "X-Values" = endpoint.values; }
            }
        }})
        .expect("AST"),
    )
    .expect("header payload type is opaque to semantic resolution");
    let endpoint = test_support::endpoint(&vector_header.client.children, "read");
    assert_eq!(endpoint.config.headers.len(), 1);

    let secret_namespace = parse::parse(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";

        GET read() -> Bytes via Binary { query { "token" = credentials.token; } }
    }})
    .err()
    .expect("credential expression namespace");
    assert!(
        secret_namespace
            .to_string()
            .contains("credentials are private and cannot appear in public expressions")
    );
}

#[test]
fn authentication_normalization_appends_replaces_clears_and_resolves_defaults() {
    let api = resolve_ok(quote! {
        client AuthApi {
            timeout 30s;
            base "https://example.test";
            credentials {
                secret key_value;
                secret bearer_value;
                credential key = api_key(secret.key_value);
                credential bearer = bearer(secret.bearer_value);
                credential rotating = custom crate::RotatingProvider;
                credential acquired = endpoint login;
                credential custom = custom crate::SessionProvider;
            }
            auth { header "X-Api-Key" = key; }

        GET login() -> Bytes via Binary;
        scope secured() {
            auth { bearer rotating challenge unauthorized; }
            GET inherited() -> Bytes via Binary;
            GET replaced() -> Bytes via Binary {
                auth replace { bearer bearer; }
            }
            GET custom() -> Bytes via Binary {
                auth replace { header "X-Session" = custom; }
            }
            GET public() -> Bytes via Binary { auth off; }
        }
    }});
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
fn custom_authorization_normalizes_source_compatibility_defaults_and_collisions() {
    let api = resolve_ok(quote! {
        client AuthApi {
            timeout 30s;
            base "https://example.test";
            credentials {
                secret key_value;
                secret token_value;
                credential key = api_key(secret.key_value);
                credential token = bearer(secret.token_value);
                credential acquired = endpoint login;
                credential custom = custom crate::SessionProvider;
            }

        GET login() -> Bytes via Binary;
        GET exact() -> Bytes via Binary {
            auth { authorization = key; }
        }
        GET scheme() -> Bytes via Binary {
            auth { authorization "tOkEn" = token; }
        }
        GET endpoint_backed() -> Bytes via Binary {
            auth { authorization "Session" = acquired; }
        }
        GET provider_backed() -> Bytes via Binary {
            auth { authorization = custom; }
        }
    }});

    for (name, expected_scheme) in [
        ("exact", None),
        ("scheme", Some("tOkEn")),
        ("endpoint_backed", Some("Session")),
        ("provider_backed", None),
    ] {
        let endpoint = test_support::endpoint(&api.client.children, name);
        assert_eq!(
            endpoint.config.auth[0].challenge,
            ResolvedChallengePolicy::Never
        );
        assert!(matches!(
            &endpoint.config.auth[0].kind,
            ResolvedAuthPlacement::Authorization(scheme)
                if scheme.as_ref().map(syn::LitStr::value).as_deref() == expected_scheme
        ));
    }

    fn semantic_error(tokens: proc_macro2::TokenStream) -> String {
        match parse::parse(tokens) {
            Ok(source) => resolve(source)
                .err()
                .expect("custom Authorization semantic error")
                .to_string(),
            Err(error) => error.to_string(),
        }
    }

    let basic = semantic_error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials {
                secret username;
                secret password;
                credential login = basic(secret.username, secret.password);
            }

        GET read() -> Bytes via Binary { auth { authorization = login; } }
    }});
    assert!(basic.contains("incompatible"));

    let custom_default = resolve_ok(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials { credential custom = custom crate::SessionProvider; }

        GET read() -> Bytes via Binary { auth { authorization "Token" = custom; } }
    }});
    let custom_default = test_support::endpoint(&custom_default.client.children, "read");
    assert_eq!(
        custom_default.config.auth[0].challenge,
        ResolvedChallengePolicy::Never
    );

    let collision = semantic_error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials {
                secret first_value;
                secret second_value;
                credential first = bearer(secret.first_value);
                credential second = api_key(secret.second_value);
            }
            auth { bearer first; }

        GET read() -> Bytes via Binary {
            auth { authorization "Token" = second; }
        }
    }});
    assert!(collision.contains("placements own `Authorization`"));
}

#[test]
fn generated_authentication_plan_accepts_exact_limit_and_rejects_first_excess_after_inheritance() {
    fn placements(start: usize, end: usize) -> Vec<proc_macro2::TokenStream> {
        (start..end)
            .map(|index| {
                let name =
                    syn::LitStr::new(&format!("auth-{index}"), proc_macro2::Span::call_site());
                quote!(query #name = key;)
            })
            .collect()
    }

    let exact = placements(0, MAX_AUTH_PLACEMENTS);
    let api = resolve_ok(quote! {
        client ExactAuthApi {
            timeout 30s;
            base "https://example.test";
            credentials {
                secret key_value;
                credential key = api_key(secret.key_value);
            }
            auth { #(#exact)* }

        GET read() -> Bytes via Binary;
    }});
    assert_eq!(api.client.config.auth.len(), MAX_AUTH_PLACEMENTS);

    let client = placements(0, MAX_AUTH_PLACEMENTS / 2);
    let scope = placements(MAX_AUTH_PLACEMENTS / 2, MAX_AUTH_PLACEMENTS);
    let error = resolve(
        parse::parse(quote! {
            client InheritedAuthApi {
                timeout 30s;
                base "https://example.test";
                credentials {
                    secret key_value;
                    credential key = api_key(secret.key_value);
                }
                auth { #(#client)* }

            scope secured() {
                auth { #(#scope)* }
                GET first_excess() -> Bytes via Binary {
                    auth { query "auth-first-excess" = key; }
                }
            }
        }})
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
            timeout 30s;
            base "https://example.test";

        GET read() -> Bytes via Binary { auth { bearer missing; } }
    }});
    assert!(unknown.contains("unknown credential `missing`"));

    let incompatible = error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials {
                secret key_value;
                credential key = api_key(secret.key_value);
            }

        GET read() -> Bytes via Binary { auth { bearer key; } }
    }});
    assert!(incompatible.contains("incompatible"));

    let custom = resolve_ok(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials { credential session = custom crate::SessionProvider; }

        GET read() -> Bytes via Binary { auth { bearer session; } }
    }});
    let custom = test_support::endpoint(&custom.client.children, "read");
    assert_eq!(
        custom.config.auth[0].challenge,
        ResolvedChallengePolicy::Never
    );

    let challengers = error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials {
                secret first_value;
                secret second_value;
                credential first = bearer(secret.first_value);
                credential second = api_key(secret.second_value);
            }

        GET read() -> Bytes via Binary {
            auth {
                bearer first challenge unauthorized;
                query "key" = second challenge unauthorized_or_forbidden;
            }
        }
    }});
    assert!(challengers.contains("at most one challenge-enabled"));

    let authorization = error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials {
                secret token;
                credential bearer = bearer(secret.token);
            }
            headers { "Authorization" = "public"; }

        GET read() -> Bytes via Binary { auth { bearer bearer; } }
    }});
    assert!(authorization.contains("reserved by transport, authentication"));

    let header = error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials {
                secret key_value;
                credential key = api_key(secret.key_value);
            }
            headers { "X-Key" = "public"; }

        GET read() -> Bytes via Binary { auth { header "x-key" = key; } }
    }});
    assert!(header.contains("already owns this authentication target"));

    let query = error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials {
                secret key_value;
                credential key = api_key(secret.key_value);
            }
            query { "key" = "public"; }

        GET read() -> Bytes via Binary { auth { query "key" = key; } }
    }});
    assert!(query.contains("already owns this authentication target"));

    let duplicate_auth = error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials {
                secret first_value;
                secret second_value;
                credential first = api_key(secret.first_value);
                credential second = api_key(secret.second_value);
            }

        GET read() -> Bytes via Binary {
            auth {
                header "X-Key" = first;
                header "x-key" = second;
            }
        }
    }});
    assert!(duplicate_auth.contains("already owns this authentication target"));
}

#[test]
fn optional_public_values_defer_authentication_material_collision_to_request_preparation() {
    resolve_ok(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials {
                secret key_value;
                credential key = api_key(secret.key_value);
            }

        GET optional(
            query_value?: String,
            header_value?: String,
        ) -> Bytes via Binary {
            query { "key" = endpoint.query_value; }
            headers { "X-Key" = endpoint.header_value; }
            auth {
                query "key" = key;
                header "X-Key" = key;
            }
        }
    }});
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
            timeout 30s;
            base "https://example.test";
            credentials { credential session = endpoint login; }

        GET login() -> Bytes via Binary {
            auth { bearer session; }
        }
    }});
    assert!(direct.contains("dependency or authentication cycle"));

    let mutual = error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials {
                credential first = endpoint first;
                credential second = endpoint second;
            }

        GET first() -> Bytes via Binary {
            auth { bearer second; }
        }
        GET second() -> Bytes via Binary {
            auth { bearer first; }
        }
    }});
    assert!(mutual.contains("dependency or authentication cycle"));
}

#[test]
fn generated_name_collisions_cover_request_types_and_inherent_methods() {
    let retry_method = resolve(
        parse::parse(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";

            GET r#retry() -> Bytes via Binary;
        }})
        .expect("AST"),
    )
    .err()
    .expect("generated retry method collision");
    assert!(retry_method.to_string().contains("retry"));
    assert!(retry_method.to_string().contains("reserved Concord method"));

    let trait_item = resolve(
        parse::parse(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";

            GET read(value?: String) -> Bytes via Binary;
            GET read_request() -> Bytes via Binary;
        }})
        .expect("AST"),
    )
    .err()
    .expect("request wrapper collision");
    assert!(trait_item.to_string().contains("collides"));

    let request_method = resolve_ok(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";

        GET read(timeout?: u64) -> Bytes via Binary;
    }});
    let request_method = test_support::endpoint(&request_method.client.children, "read");
    assert_eq!(request_method.surface.setters[0].setter, "set_timeout");

    let helper_names = resolve_ok(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";

        GET read(
            value?: String,
            clear_value?: String,
            limit: u32 = 25,
            reset_limit?: bool,
        ) -> Bytes via Binary;
    }});
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
                timeout 30s;
                base "https://example.test";

            scope resources(region?: String) {
                GET set_region() -> Bytes via Binary;
            }
        }})
        .expect("AST"),
    )
    .err()
    .expect("child method colliding with set-prefixed scope control");
    assert!(child_setter.to_string().contains("set_region"));

    for helper in ["omit_label", "reset_label"] {
        let helper = syn::Ident::new(helper, proc_macro2::Span::call_site());
        let collision = resolve(
            parse::parse(quote! {
                client Api {
                    timeout 30s;
                    base "https://example.test";

                scope resources(label?: Option<String> = None) {
                    GET #helper() -> Bytes via Binary;
                }
            }})
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
                timeout 30s;
                base "https://example.test";

            scope resources() {
                GET resources() -> Bytes via Binary;
            }
        }})
        .expect("AST"),
    )
    .err()
    .expect("scope facade type collision");
    assert!(scope_type.to_string().contains("Resources"));

    let endpoint_trait = resolve(
        parse::parse(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";

            GET endpoint() -> Bytes via Binary;
        }})
        .expect("AST"),
    )
    .err()
    .expect("endpoint trait collision");
    assert!(endpoint_trait.to_string().contains("Endpoint"));

    let storage_terminal = resolve_ok(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials {
                credential session = endpoint source;
            }

        GET source(store_as_session?: bool) -> Bytes via Binary;
    }});
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
            timeout 30s;
            base "https://example.test";
            credentials {
                secret build;
                credential key = api_key(secret.build);
            }
        }
    });
    assert!(reserved_secret.contains("build"));
    assert!(reserved_secret.contains("collides"));

    let helper_secret = error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            values { token?: String; }
            credentials {
                secret omit_token;
                credential key = api_key(secret.omit_token);
            }
        }
    });
    assert!(helper_secret.contains("omit_token"));
    assert!(helper_secret.contains("collides"));

    let reset_secret = error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            values { limit: u32 = 25; }
            credentials {
                secret reset_limit;
                credential key = api_key(secret.reset_limit);
            }
        }
    });
    assert!(reset_secret.contains("reset_limit"));
    assert!(reset_secret.contains("collides"));

    let defaulted_option_clear_secret = error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            values { label?: Option<String> = None; }
            credentials {
                secret omit_label;
                credential key = api_key(secret.omit_label);
            }
        }
    });
    assert!(defaulted_option_clear_secret.contains("omit_label"));
    assert!(defaulted_option_clear_secret.contains("collides"));

    let defaulted_option_reset_secret = error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            values { label?: Option<String> = None; }
            credentials {
                secret reset_label;
                credential key = api_key(secret.reset_label);
            }
        }
    });
    assert!(defaulted_option_reset_secret.contains("reset_label"));
    assert!(defaulted_option_reset_secret.contains("collides"));

    let internal_accessor = error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials {
                secret value;
                credential __concord_new = api_key(secret.value);
            }
        }
    });
    assert!(internal_accessor.contains("__concord_new"));
    assert!(internal_accessor.contains("collides"));

    let set_prefixed_secret = error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            values { region: String; }
            credentials {
                secret set_region;
                credential key = api_key(secret.set_region);
            }
        }
    });
    assert!(set_prefixed_secret.contains("set_region"));
    assert!(set_prefixed_secret.contains("collides"));

    let request_wrapper = error(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            credentials { credential session = endpoint read; }

        GET read() -> Bytes via Binary;
        GET read_request() -> Bytes via Binary;
    }});
    assert!(request_wrapper.contains("collides"));
}

#[test]
fn pagination_normalizes_one_controller_type_and_field_initializer_ir() {
    let api = resolve_ok(quote! {
        client Api {
            timeout 30s;
            base "https://example.test";
            values { tenant: String; }

        GET offset(offset: u64, limit: u32) -> Vec<u32> via PageSpec<Vec<u32>> {
            paginate via concord_core::contracts::OffsetController {
                offset -> endpoint.offset;
                page_size -> endpoint.limit;
            }
        }
        GET vendor(cursor?: String) -> Vec<u32> via PageSpec<Vec<u32>> {
            paginate via crate::Vendor<String> {
                cursor -> endpoint.cursor;
                tenant = client.tenant;
                attempts = 0;
            }
        }
    }});
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
            timeout 30s;
            base "https://example.test";

        GET read(offset: String, limit: bool) -> Vec<u32> via PageSpec<Vec<u32>> {
            paginate via crate::ArbitraryController {
                anything -> endpoint.offset;
                another -> endpoint.limit;
            }
        }
    }});
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
                timeout 30s;
                base "https://example.test";

            GET read(offset: u64) -> Vec<u32> via PageSpec<Vec<u32>> {
                paginate via crate::Controller {
                    state -> endpoint.missing;
                }
            }
        }})
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
                timeout 30s;
                base "https://example.test";

            POST read(
                offset: u64,
                body filter: String via Text,
            ) -> Vec<u32> via PageSpec<Vec<u32>> {
                paginate via crate::Controller {
                    state -> endpoint.filter;
                }
            }
        }})
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
        client HTTPApi {
            timeout 30s; base "https://example.test";
        scope http_status() {
            GET get_http_status(
                first: String,
                body payload: String via Text,
                optional?: std::option::Option<u32>,
                second: bool,
                alias_value: crate::MaybeString,
            ) -> Bytes via Binary;
            GET r#type() -> Bytes via Binary;
        }
    }});
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
fn endpoint_contract_ids_and_explicit_rate_policy_ids_are_independent() {
    let api = resolve_ok(quote! {
        client RiotApi {
            timeout 30s;
            base "https://example.test";
            rate_limit { id "concord.tests.9"; key [origin]; cost 1;  allow 100 per 1m; }

        scope match_v5() {
            rate_limit { id "concord.tests.10"; key [origin]; cost 1;  allow 50 per 1m; }
            GET match_by_id() -> Bytes via Binary {
                id "riot.match_v5.match_by_id";
                rate_limit { id "concord.tests.11"; key [origin]; cost 1;  allow 10 per 1m; }
            }
            GET timeline_by_id() -> Bytes via Binary;
        }
    }});
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
        ["concord.tests.9", "concord.tests.10", "concord.tests.11",]
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
            client OtherRustName {
                timeout 30s; base "https://example.test";
            GET #endpoint_name() -> Bytes via Binary {
                id "stable.remote.operation";
            }
        }});
        let Child::Endpoint(endpoint) = &renamed.client.children[0] else {
            panic!("renamed endpoint")
        };
        assert_eq!(endpoint.contract_id.value, "stable.remote.operation");
    }

    let duplicate = resolve(
        parse::parse(quote! {
            client Api {
                timeout 30s; base "https://example.test";
            GET first() -> Bytes via Binary { id "duplicate.operation"; }
            GET second() -> Bytes via Binary { id "duplicate.operation"; }
        }})
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
