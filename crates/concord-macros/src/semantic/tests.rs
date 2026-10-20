//! Semantic resolver tests: LANG-03 binding, placement normalization,
//! structural patch preconditions, liveness, auth, rates, pagination,
//! identities and generated-name collisions.

use proc_macro2::TokenStream;
use quote::quote;

use super::*;

fn resolve_tokens(tokens: TokenStream) -> syn::Result<Model> {
    crate::syntax::parse(tokens).and_then(resolve)
}

fn ok(tokens: TokenStream) -> Model {
    match resolve_tokens(tokens) {
        Ok(model) => model,
        Err(error) => panic!("expected a valid declaration: {error}"),
    }
}

fn err(tokens: TokenStream) -> String {
    match resolve_tokens(tokens) {
        Ok(_) => panic!("expected a diagnostic"),
        Err(error) => error.into_compile_error().to_string(),
    }
}

fn assert_err(tokens: TokenStream, needles: &[&str]) {
    let message = err(tokens);
    for needle in needles {
        assert!(
            message.contains(needle),
            "diagnostic `{message}` lacks `{needle}`"
        );
    }
}

fn endpoint<'a>(model: &'a Model, name: &str) -> &'a Endpoint {
    model
        .endpoints
        .iter()
        .find(|endpoint| endpoint.logical == name)
        .unwrap_or_else(|| panic!("endpoint {name}"))
}

#[test]
fn bare_names_bind_only_the_current_owner() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                values { query tenant: String; }
                scope s(id: String) {
                    GET e() -> T { path "x/{id}"; }
                }
            }
        },
        &[
            "[C3-NAME]",
            "`id` is not a value of endpoint `A::s::e`",
            "s.id",
        ],
    );
    let model = ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            values { query tenant: String; }
            scope s(id: String) {
                path "s/{id}";
                GET e(id: String) -> T { path "x/{id}/{s.id}"; }
            }
        }
    });
    let e = endpoint(&model, "e");
    assert!(matches!(e.path, Some(LocalPath::Append(_))));
}

#[test]
fn redundant_qualification_of_the_current_owner_is_a_migration_error() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e(id: String) -> T { path "x/{endpoint.id}"; }
            }
        },
        &["[C3-MIGRATE]", "redundant qualification"],
    );
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                scope s(id: String) {
                    path "{s.id}";
                    GET e() -> T;
                }
            }
        },
        &["[C3-MIGRATE]", "redundantly qualifies the current owner"],
    );
}

#[test]
fn direct_placement_is_a_normal_introduction_and_values_can_have_several_uses() {
    let model = ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            GET inspect(query locale?: String) -> T {
                path "inspect";
                headers { "Accept-Language" = locale; }
                query { "language" = locale; }
            }
        }
    });
    let e = endpoint(&model, "inspect");
    let keys: Vec<(&str, Option<StatementKind>)> = e
        .program
        .ops
        .iter()
        .map(|op| (op.key.as_str(), op.kind))
        .collect();
    assert_eq!(
        keys,
        [
            ("locale", Some(StatementKind::Introduce)),
            ("language", Some(StatementKind::Introduce)),
            ("Accept-Language", Some(StatementKind::Introduce)),
        ]
    );
    assert!(e.program.ops.iter().all(|op| op.presence.is_some()));
}

#[test]
fn introduction_conflicts_name_both_writers() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                values { query locale?: String; }
                GET e(query locale?: String) -> T;
            }
        },
        &[
            "[C3-OVERRIDE]",
            "already has a potential writer",
            "A direct placement `locale`",
            "A::e direct placement",
        ],
    );
}

#[test]
fn replace_when_present_and_remove_require_an_earlier_writer() {
    for statement in [quote!(replace "missing" = "x";), quote!(remove "missing";)] {
        assert_err(
            quote! {
                client A {
                    base "https://a.test/";
                    response Json;
                    GET e() -> T { query { #statement } }
                }
            },
            &["[C3-OVERRIDE]", "has no earlier potential writer"],
        );
    }
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                values { query locale?: String; }
                GET e(total: String) -> T { query { when_present "locale" = total; } }
            }
        },
        &["[C3-OVERRIDE]", "requires an outer-optional source"],
    );
    ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            values { query locale?: String; }
            GET e(other?: String) -> T { query { when_present "locale" = other; append "extra" = "1"; } }
        }
    });
}

#[test]
fn family_off_cannot_coexist_with_direct_placements_but_replace_can() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e(query q?: String) -> T { query off; }
            }
        },
        &["[C3-PLACE]", "cannot coexist"],
    );
    let model = ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            values { query tenant: String; }
            GET e(query q?: String) -> T { query replace { "fixed" = "1"; } }
        }
    });
    let e = endpoint(&model, "e");
    assert!(e.program.ops[0].kind.is_none(), "the fence clears first");
    assert_eq!(e.program.ops[1].key, "q");
    assert_eq!(e.program.ops[2].key, "fixed");
}

#[test]
fn header_names_compare_case_insensitively_and_singletons_cannot_repeat() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                headers { "X-Trace" = "a"; }
                GET e() -> T { headers { "x-trace" = "b"; } }
            }
        },
        &["[C3-OVERRIDE]"],
    );
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e(agents: Vec<String>) -> T { headers { "User-Agent" = each agents; } }
            }
        },
        &["singleton request field"],
    );
}

#[test]
fn optional_sources_are_rejected_inside_concat_and_routes() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e(tag?: String) -> T { headers { "X" = concat["a", tag]; } }
            }
        },
        &["[C3-ATOM]", "outer-optional"],
    );
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e(id?: String) -> T { path "items/{id}"; }
            }
        },
        &["[C3-PATH]", "outer-optional"],
    );
}

#[test]
fn every_input_needs_a_reachable_use() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e(unused: String) -> T;
            }
        },
        &["required value `unused`", "used nowhere"],
    );
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e(unused?: String) -> T;
            }
        },
        &["no reachable semantic use"],
    );
    let model = ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            GET e(#[deprecated] unused?: String) -> T;
        }
    });
    assert!(!endpoint(&model, "e").values[0].live);
    // An emission removed later in the same site is dead on every branch.
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e(q: String) -> T { query { "q" = q; remove "q"; } }
            }
        },
        &["used nowhere"],
    );
    // An owner-level use suppressed by an endpoint remains live through the
    // owner's runtime projection.
    ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            scope s(query region: String) {
                GET e() -> T { query { remove "region"; } }
            }
        }
    });
}

#[test]
fn value_controls_follow_the_structural_matrix() {
    let model = ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            values {
                query required: String;
                query optional?: String;
                query initialized: u32 = 5;
                #[default] query defaulted?: String;
            }
            GET e(query r: String, query o?: String, query i: u32 = 1, query("t") each tags: Vec<String>) -> T;
        }
    });
    let names = |value: &Value| {
        value
            .controls
            .names()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    };
    let root = model.root();
    assert_eq!(
        names(&root.values[0]),
        ["set_required", "set_required_if_some"]
    );
    assert_eq!(
        names(&root.values[1]),
        [
            "set_optional",
            "replace_optional",
            "set_optional_if_some",
            "omit_optional"
        ]
    );
    assert_eq!(
        names(&root.values[2]),
        [
            "set_initialized",
            "set_initialized_if_some",
            "reset_initialized"
        ]
    );
    assert_eq!(
        names(&root.values[3]),
        [
            "set_defaulted",
            "replace_defaulted",
            "set_defaulted_if_some",
            "omit_defaulted",
            "reset_defaulted"
        ]
    );
    let e = endpoint(&model, "e");
    assert!(
        names(&e.values[0]).is_empty(),
        "required endpoint input is positional"
    );
    assert_eq!(
        names(&e.values[1]),
        ["set_o", "replace_o", "set_o_if_some", "omit_o"]
    );
    assert_eq!(names(&e.values[2]), ["set_i", "set_i_if_some", "reset_i"]);
    assert_eq!(e.positional, [Positional::Value(0), Positional::Value(3)]);
}

#[test]
fn generated_name_collisions_are_rejected() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                scope item_request() {
                    GET item() -> () via Empty;
                }
            }
        },
        &[
            "[C3-GENERATED]",
            "ItemRequest",
            "request type of endpoint `item`",
            "handle of scope `item_request`",
            "is declared here",
        ],
    );
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e(query timeout: u32 = 5) -> T;
            }
        },
        &["[C3-GENERATED]", "reset_timeout"],
    );
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                scope s(query locale?: String) {
                    GET set_locale() -> T;
                }
            }
        },
        &["[C3-GENERATED]", "set_locale"],
    );
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET item_list() -> T;
                GET item__list() -> T { id "a.other"; }
            }
        },
        &["[C3-GENERATED]", "ItemListEndpoint"],
    );
    // A root endpoint `generated` would emit a `GeneratedEndpoint` type that
    // collides with the client module's generated intent trait.
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET generated() -> T;
            }
        },
        &[
            "[C3-GENERATED]",
            "GeneratedEndpoint",
            "generated intent trait",
        ],
    );
    // A scope module does not define the trait, so its endpoint may use it.
    ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            scope s() {
                GET generated() -> T;
            }
        }
    });
}

#[test]
fn identities_are_derived_explicit_and_unique() {
    let model = ok(quote! {
        client CatalogApi {
            base "https://a.test/";
            response Json;
            scope regional() {
                GET item_summaries() -> T;
                GET pinned() -> T { id "catalog.pinned"; }
            }
        }
    });
    assert_eq!(
        endpoint(&model, "item_summaries").id,
        "catalog_api.regional.item_summaries"
    );
    assert_eq!(endpoint(&model, "pinned").id, "catalog.pinned");
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET one() -> T { id "same.id"; }
                GET two() -> T { id "same.id"; }
            }
        },
        &[
            "endpoint ID `same.id` of endpoint `A::two` is used by two endpoints of client `A`",
            "declare a distinct `id",
        ],
    );
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET données() -> T;
            }
        },
        &["navigation-derived endpoint ID", "explicit `id"],
    );
}

#[test]
fn auth_targets_are_exclusive_and_challenges_disjoint_from_statuses() {
    let base = quote! {
        base "https://a.test/";
        response Json;
        credentials {
            secret key;
            credential k = api_key(secret.key);
            credential b = bearer(secret.key);
        }
    };
    assert_err(
        quote! {
            client A {
                #base
                auth { bearer b; }
                GET e() -> T { auth { authorization = k; } }
            }
        },
        &["[C3-AUTH]", "already claimed"],
    );
    assert_err(
        quote! {
            client A {
                #base
                auth { bearer b challenge unauthorized; }
                GET e() -> T { status [200, 401]; }
            }
        },
        &["overlaps the effective auth challenge"],
    );
    assert_err(
        quote! {
            client A {
                #base
                headers { "X-Key" = "public"; }
                GET e() -> T { auth { header "X-Key" = k; } }
            }
        },
        &["collides with a public header writer"],
    );
    let model = ok(quote! {
        client A {
            #base
            auth { bearer b; }
            GET e() -> T { auth replace { header "X-Key" = k; } }
            GET f() -> T { auth off; }
        }
    });
    assert_eq!(endpoint(&model, "e").effective_auth.len(), 1);
    assert!(endpoint(&model, "f").effective_auth.is_empty());
}

#[test]
fn public_writers_cannot_target_protected_fields_even_when_the_source_is_optional() {
    // HEAD-01 / V3-S08 (static negative): public, private-auth, body-media,
    // negotiation and transport targets have distinct owners. Potential
    // ownership is decided before presence is known, so an outer-optional
    // (absent-capable) source never makes an overlapping public writer legal.
    let reserved_needles = ["[C3-PLACE]", "is not a generic public-header target"];
    for family_member in ["content-type", "content-length", "accept", "host"] {
        assert!(
            crate::syntax::diagnostics::RESERVED_PUBLIC_HEADERS.contains(&family_member),
            "{family_member} is reserved"
        );
    }
    for reserved in crate::syntax::diagnostics::RESERVED_PUBLIC_HEADERS {
        let upper = reserved.to_ascii_uppercase();
        // Optional direct placement (either case).
        for name in [(*reserved).to_owned(), upper.clone()] {
            assert_err(
                quote! {
                    client A {
                        base "https://a.test/";
                        response Json;
                        GET e(header(#name) hint?: String) -> T;
                    }
                },
                &reserved_needles,
            );
        }
        // Optional sources in explicit statements: append, repeated
        // introduction of an optional sequence, and a conditional override.
        assert_err(
            quote! {
                client A {
                    base "https://a.test/";
                    response Json;
                    GET e(hint?: String) -> T { headers { append #reserved = hint; } }
                }
            },
            &reserved_needles,
        );
        assert_err(
            quote! {
                client A {
                    base "https://a.test/";
                    response Json;
                    GET e(hints?: Vec<String>) -> T { headers { #upper = each hints; } }
                }
            },
            &reserved_needles,
        );
        assert_err(
            quote! {
                client A {
                    base "https://a.test/";
                    response Json;
                    scope s(hint?: String) {
                        headers { when_present #reserved = hint; }
                        GET e() -> T;
                    }
                }
            },
            &reserved_needles,
        );
    }

    // Private auth: the only potential public writer of the target has an
    // optional source; the collision is still rejected.
    let base = quote! {
        base "https://a.test/";
        response Json;
        credentials {
            secret key;
            credential k = api_key(secret.key);
        }
    };
    let auth_needles = ["[C3-AUTH]", "collides with a public"];
    // An optional scope parameter placed directly against the scope's own
    // private header placement.
    assert_err(
        quote! {
            client A {
                #base
                scope s(header("X-Key") key_hint?: String) {
                    auth { header "x-key" = k; }
                    GET e() -> T;
                }
            }
        },
        &auth_needles,
    );
    // An optional repeated query placement against the endpoint's private
    // query placement.
    assert_err(
        quote! {
            client A {
                #base
                GET e(query("api_key") each keys?: Vec<String>) -> T {
                    auth { query "api_key" = k; }
                }
            }
        },
        &auth_needles,
    );
    // The inherited literal writer is removed, so the only remaining
    // potential writer is an optional append ...
    assert_err(
        quote! {
            client A {
                #base
                headers { "X-Key" = "public-fallback"; }
                GET e(hint?: String) -> T {
                    headers { remove "X-Key"; append "x-key" = hint; }
                    auth { header "X-Key" = k; }
                }
            }
        },
        &auth_needles,
    );
    // ... control: without that optional append the removal frees the
    // target and the private placement is valid.
    let model = ok(quote! {
        client A {
            #base
            headers { "X-Key" = "public-fallback"; }
            GET e() -> T {
                headers { remove "X-Key"; }
                auth { header "X-Key" = k; }
            }
        }
    });
    assert_eq!(endpoint(&model, "e").effective_auth.len(), 1);
}

#[test]
fn credential_sources_resolve_forward_paths_and_reject_cycles() {
    let model = ok(quote! {
        client A {
            base "https://a.test/";
            request Json;
            response Json;
            credentials { credential session = endpoint accounts::login; }
            scope accounts() {
                POST login(body input: Login) -> Token;
            }
            scope secured() {
                auth { bearer session; }
                GET e() -> T;
            }
        }
    });
    assert_eq!(endpoint(&model, "login").storage, [0]);
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                request Json;
                response Json;
                credentials { credential session = endpoint login; }
                auth { bearer session; }
                POST login(body input: Login) -> Token;
            }
        },
        &["[C3-AUTH]", "cycle"],
    );
}

#[test]
fn rate_attachments_bind_exactly_deduplicate_identical_repeats_and_reject_conflicts() {
    let registry = quote! {
        rate_limits {
            windows standard { allow 10 per 1s; }
            policy per_tenant(tenant) {
                id "a.tenant";
                key [origin, "tenant" = tenant];
                using standard;
            }
        }
    };
    let model = ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            values { query tenant: String; }
            #registry
            rate_limit per_tenant(tenant = tenant);
            GET e() -> T { rate_limit per_tenant(tenant = client.tenant); }
        }
    });
    assert_eq!(
        endpoint(&model, "e").effective_rates.len(),
        1,
        "identical repeat is deduplicated"
    );
    assert!(endpoint(&model, "e").rates.is_empty());
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                values { query tenant: String; }
                #registry
                rate_limit per_tenant(tenant = tenant);
                GET e() -> T { rate_limit per_tenant(tenant = client.tenant) { cost 2; } }
            }
        },
        &["[C3-RATE]", "twice with different"],
    );
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                values { query tenant: String; }
                #registry
                GET e() -> T { rate_limit per_tenant(); }
            }
        },
        &["unbound"],
    );
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e() -> T {
                    rate_limit { id "a.endpoint"; key [endpoint]; allow 1 per 1s; }
                }
            }
        },
        &["explicit `id"],
    );
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                rate_limits {
                    policy p(unused) { id "a.p"; key [origin]; allow 1 per 1s; }
                }
                GET e() -> T;
            }
        },
        &["formal `unused`", "unused"],
    );
    let model = ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            GET e() -> T;
        }
    });
    assert!(!model.client.requires_limiter);
}

#[test]
fn pagination_requires_safe_effect_and_scalar_endpoint_destinations() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                request Json;
                response Json;
                POST e(body b: B, query page: u32 = 1) -> T {
                    paginate via C { init I { page: page }; cursor -> page; }
                }
            }
        },
        &["[C3-PAGE]", "effect safe"],
    );
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                values { query page: u32 = 1; }
                GET e() -> T {
                    paginate via C { init I { page: client.page }; cursor -> page; }
                }
            }
        },
        &["[C3-PAGE]", "not an endpoint-owned value"],
    );
    let model = ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            GET e(query after?: String) -> T {
                paginate via C { init I { after: after }; cursor -> after; }
            }
        }
    });
    assert!(endpoint(&model, "e").values[0].page_live);
}

#[test]
fn absence_is_disjoint_and_exclusive_with_pagination_and_accepted_bytes() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e() -> Option<T> { status [200, 404]; absent on 404; }
            }
        },
        &["overlaps the accepted payload statuses"],
    );
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e() -> Option<T> { absent on 404; expose accepted_bytes; }
            }
        },
        &["NotFound"],
    );
}

#[test]
fn representations_are_selected_by_direction_with_explicit_fences() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                scope s() {
                    response explicit;
                    GET e() -> T;
                }
            }
        },
        &["[C3-REP]", "response explicit"],
    );
    let model = ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            scope s() {
                response explicit;
                GET e() -> T via Text;
            }
        }
    });
    assert_eq!(endpoint(&model, "e").response_origin, "inline `via`");
}

#[test]
fn source_identifiers_keep_the_baseline_snake_case_rule() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET getItem() -> T;
            }
        },
        &["[C3-GENERATED]", "must be snake_case", "get_item"],
    );
}

#[test]
fn scope_names_are_unique_along_one_ancestor_chain() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                scope s() {
                    scope s() { GET e() -> T; }
                }
            }
        },
        &[
            "scope `A::s::s` repeats the name of its enclosing scope `A::s`",
            "rename one of the scopes",
        ],
    );
}

#[test]
fn user_types_shadowed_by_generated_types_are_rejected() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                request Json;
                response Json;
                POST create_item(body input: CreateItemRequest) -> T;
            }
        },
        &["[C3-GENERATED]", "CreateItemRequest", "absolute path"],
    );
    ok(quote! {
        client A {
            base "https://a.test/";
            request Json;
            response Json;
            POST create_item(body input: crate::models::CreateItemRequest) -> T;
        }
    });
}

#[test]
fn rate_attachments_cannot_be_turned_off_and_the_plan_limit_names_the_excess() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                rate_limit { id "a.all"; key [origin]; allow 1 per 1s; }
                GET e() -> T { rate_limit off; }
            }
        },
        &["[C3-RATE]", "there is no `rate_limit off`", "RATE-01"],
    );
    let attachments = |count: usize| {
        let ids = (0..count).map(|index| format!("a.p{index}"));
        quote!(#(rate_limit { id #ids; key [origin]; allow 1 per 1s; })*)
    };
    let at_limit = attachments(64);
    ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            #at_limit
            GET e() -> T;
        }
    });
    let over = attachments(65);
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                #over
                GET e() -> T;
            }
        },
        &["[C3-RATE]", "more than 64 effective rate policies"],
    );
}

#[test]
fn a_secret_no_credential_source_reads_is_a_dead_required_input() {
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                credentials {
                    secret used;
                    secret unused;
                    credential key = api_key(secret.used);
                }
                GET e() -> T { auth { header "X-Key" = key; } }
            }
        },
        &[
            "[C3-AUTH]",
            "secret `unused` of client `A` is never used by a credential source",
        ],
    );
    // Every secret read by some source, including both Basic halves, is live.
    ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            credentials {
                secret user;
                secret pass;
                secret token;
                credential login = basic(secret.user, secret.pass);
                credential bearer_token = bearer(secret.token);
            }
            GET e() -> T { auth { basic login; } }
            GET f() -> T { auth { bearer bearer_token; } }
        }
    });
}

/// Constant paths, pagination Init paths and atom adapters are evaluated
/// inside the generated support modules too: a leading segment naming a
/// generated module/type, or a relative `self::`/`super::` prefix, would
/// silently re-resolve there, so it is rejected before emission.
#[test]
fn constant_paths_and_adapters_shadowed_by_generated_names_are_rejected() {
    // A scope named `defaults` shadows the user's `defaults` module.
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                scope defaults(tier: u32 = defaults::TIER) {
                    path "tiers/{tier}";
                    GET items() -> T;
                }
            }
        },
        &[
            "[C3-GENERATED]",
            "constant path `defaults::TIER`",
            "the support module of scope",
        ],
    );
    // R7 rebases explicit lexical paths; only bare generated-name collisions
    // remain rejected. Native emitter controls verify the resulting bindings.
    ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            GET items(query count: u32 = super::PAGE) -> T;
        }
    });
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET item(id: u32) -> T {
                    path "items/{id via ItemRequest}";
                }
            }
        },
        &["[C3-GENERATED]", "ItemRequest"],
    );
    ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            GET feed(query offset: u64 = 0) -> T {
                paginate via crate::Pages {
                    init self::Start { offset: offset, size: crate::SIZE };
                    cursor -> offset;
                }
            }
        }
    });
    // Absolute and crate-rooted spellings of the same items are accepted.
    ok(quote! {
        client A {
            base "https://a.test/";
            response Json;
            scope defaults(tier: u32 = crate::defaults::TIER) {
                path "tiers/{tier via crate::Adapter}";
                GET items(query count: u32 = crate::PAGE) -> T;
            }
        }
    });
}

/// DIAG-01 (§19.1): coded diagnostics of every family name their owning
/// client, scope or endpoint by its owner label and carry a correction.
#[test]
fn coded_diagnostics_name_their_owner_and_a_correction() {
    // C3-PLACE: a reserved public header in a scope's block.
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                scope s() {
                    headers { "Authorization" = "x"; }
                    GET e() -> T;
                }
            }
        },
        &[
            "[C3-PLACE]",
            "of scope `A::s`",
            "place credentials with `auth",
        ],
    );
    // C3-ATOM: an empty concatenation in an endpoint header.
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e() -> T { headers { "X-Trace" = concat []; } }
            }
        },
        &["[C3-ATOM]", "in endpoint `A::e`", "write at least one part"],
    );
    // C3-AUTH: two challenge-attributing placements on one endpoint.
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                credentials {
                    secret one;
                    secret two;
                    credential first = api_key(secret.one);
                    credential second = api_key(secret.two);
                }
                GET e() -> T {
                    auth {
                        header "X-One" = first challenge unauthorized;
                        header "X-Two" = second challenge unauthorized;
                    }
                }
            }
        },
        &["[C3-AUTH]", "of endpoint `A::e`", "write `challenge never`"],
    );
    // C3-RATE: a zero cost in a scope's inline policy.
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                scope s() {
                    rate_limit { id "a.s"; key [origin]; cost 0; allow 1 per 1s; }
                    GET e() -> T;
                }
            }
        },
        &[
            "[C3-RATE]",
            "cost must be positive",
            "scope `A::s`",
            "at least 1",
        ],
    );
    // C3-PAGE: a duplicate Init field.
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET feed(query offset: u64 = 0) -> T {
                    paginate via crate::Pages {
                        init crate::Start { offset: offset, offset: offset };
                        cursor -> offset;
                    }
                }
            }
        },
        &[
            "[C3-PAGE]",
            "duplicate Init field `offset`",
            "endpoint `A::feed`",
            "assign each field once",
        ],
    );
    // C3-NAME: an unknown bare name with no ancestor candidate.
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e() -> T { path "x/{missing}"; }
            }
        },
        &[
            "[C3-NAME]",
            "not a value of endpoint `A::e`",
            "declare `missing`",
        ],
    );
    // C3-REP: a client-level `explicit` fence.
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response explicit;
                GET e() -> T;
            }
        },
        &["[C3-REP]", "client `A` simply omits the `response` default"],
    );
    // C3-OVERRIDE: a misspelled override target names its owner.
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                query { "locale" = "en"; }
                GET e() -> T { query { replace "lcoale" = "fr"; } }
            }
        },
        &[
            "[C3-OVERRIDE]",
            "in endpoint `A::e`",
            "check the key spelling",
        ],
    );
    // C3-OWNER: a singleton duplicate names the full endpoint path.
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                scope s() {
                    GET e() -> T { timeout 1s; timeout 2s; }
                }
            }
        },
        &[
            "[C3-OWNER]",
            "duplicate `timeout` clause for endpoint `A::s::e`",
            "keep exactly one",
        ],
    );
    // C3-PATH: a template error names the owner and the byte range.
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e() -> T { path "items/"; }
            }
        },
        &[
            "[C3-PATH]",
            "path template of endpoint `A::e`",
            "template bytes 5..6",
        ],
    );
    // C3-MIGRATE: a retired form names the owner and the v3 spelling.
    assert_err(
        quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e() -> T { query { "a" += "b"; } }
            }
        },
        &[
            "[C3-MIGRATE]",
            "`+=` was removed; write `append",
            "= value;` in endpoint `A::e`",
        ],
    );
}

#[test]
fn head_literals_are_checked_in_every_owner_and_statement_form() {
    for family in ["query", "headers"] {
        for block in ["", "replace"] {
            for statement in ["", "replace", "when_present", "append"] {
                for owner in ["client", "scope", "endpoint"] {
                    let literal = if family == "headers" {
                        "\"bad\\n\"".to_owned()
                    } else {
                        format!(
                            "{:?}",
                            "x".repeat(crate::MAX_FORMATTED_PUBLIC_VALUE_BYTES + 1)
                        )
                    };
                    for atom in [
                        literal.clone(),
                        format!("concat [true, concat [{literal}, 42]]"),
                    ] {
                        let clause =
                            format!("{family} {block} {{ {statement} \"X-Test\" = {atom}; }}");
                        let (body, label) = match owner {
                            "client" => (clause, "client `A`"),
                            "scope" => (format!("scope s() {{ {clause} }}"), "scope `A::s`"),
                            _ => (format!("GET e() -> T {{ {clause} }}"), "endpoint `A::e`"),
                        };
                        let source = format!("client A {{ base \"http://a.test/\"; {body} }}");
                        assert_err(source.parse().unwrap(), &["[C3-PLACE]", label]);
                    }
                }
            }
        }
    }
}
