use super::super::*;
use super::helpers::*;
use quote::quote;

#[test]
fn generated_minimal_api_contains_facade_and_endpoint_plan() {
    let out = expanded(quote! {
        client SnapshotMinimal {
            base "https://example.com"
        }

        GET Ping
            as ping
            path ["ping"]
            -> Json<String>;
    });

    assert_contains_all(
        &out,
        &[
            "pub fn ping (& self",
            "-> :: concord_core :: prelude :: PendingRequest",
            "impl :: concord_core :: prelude :: Endpoint < super :: SnapshotMinimalCx > for EpPing",
            "type Response = String",
            "fn plan (& self, plan_ctx : & :: concord_core :: __private :: ClientPlanContext",
            ":: concord_core :: __private :: RequestPlan",
            ":: concord_core :: __private :: EndpointPlan",
            "ResponseEntity",
            "BufferedResponse",
            "__response_entity_plan",
            "response: __response_plan",
        ],
    );
}

#[test]
fn generated_endpoint_plan_contains_plan_based_core_contract() {
    let out = expanded(quote! {
        client PlanApi {
            base "https://example.com"
            var tenant: String
            secret token: String
            credential key = api_key(secret.token)
        }

        GET Create(id: String, limit: u64 = 20)
            as create
            path ["items", id]
            headers {
                "X-Tenant" = vars.tenant
            }
            query {
                limit
            }
            auth header "X-Api-Key" = key
            paginate OffsetLimitPagination {
                offset = 0,
                limit = limit
            }
            -> Json<CreateResponse>
    });

    assert_contains_all(
        &out,
        &[
            "impl :: concord_core :: prelude :: Endpoint < super :: PlanApiCx >",
            "fn plan (& self , plan_ctx : & :: concord_core :: __private :: ClientPlanContext",
            ":: concord_core :: __private :: RequestPlan",
            ":: concord_core :: __private :: EndpointPlan",
            ":: concord_core :: __private :: EndpointMeta",
            ":: concord_core :: __private :: ResolvedRoute",
            ":: concord_core :: __private :: ResolvedPolicy",
            "ResponseEntity",
            "BufferedResponse",
            "__response_entity_plan",
            "response: __response_plan",
            "let __pagination_plan = :: core :: option :: Option :: Some",
            "PaginationMarker",
        ],
    );
}

#[test]
fn codegen_uses_resolved_ir() {
    let resolved = crate::sema::analyze_tokens_for_test(quote! {
        client ResolvedIrApi {
            base "https://example.com"
            secret token: String
            credential session = bearer(secret.token)

            policies {
                rate_limit app {
                    bucket application by [host] {
                        10 / 1s
                    }
                }
            }

            profiles {
                profile shared {
                    auth bearer session
                    rate_limit app
                }

                profile endpoint_override {}
            }

            default {
                profile shared
            }
        }

        GET Ping(page?: u64 = 0)
            path ["ping"]
            profile endpoint_override
            -> Json<String>
    });

    match &resolved.client_policy.rate_limit {
        Some(RateLimitResolved::Add(plan)) => {
            assert_eq!(plan.buckets.len(), 1);
            let bucket = &plan.buckets[0];
            assert_eq!(bucket.kind, "application");
            assert_eq!(bucket.name, "app_0");
            assert_eq!(bucket.cost, 1);
            assert_eq!(bucket.key.len(), 1);
        }
        other => panic!(
            "expected resolved client rate limit from profile/default lowering, got {other:?}"
        ),
    }

    let endpoint = resolved.endpoints.iter().find(|ep| ep.name == "Ping");
    assert!(endpoint.is_some(), "resolved ping endpoint missing");
    let endpoint = endpoint.expect("ping endpoint missing");
    assert_eq!(
        endpoint.behavior_doc.names,
        vec!["shared".to_string(), "endpoint_override".to_string()]
    );
    let out = emit(resolved)
        .to_string()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();

    assert_contains_all(
        &out,
        &[
            "policy.add_rate_limit(::concord_core::advanced::RateLimitPlan::from_buckets",
            "RateLimitBucketUse::new(\"application\",\"app_0\"",
        ],
    );
    for removed in [
        "RetryConfig",
        "RetryPolicy",
        "set_retry",
        "clear_retry",
        "max_attempts",
    ] {
        assert!(
            !out.contains(removed),
            "generated source retained {removed}"
        );
    }
}

#[test]
fn codegen_rejects_over_deep_scope_modules_with_a_controlled_diagnostic() {
    let mut resolved = crate::sema::analyze_tokens_for_test(quote! {
        client DeepCodegenApi {
            base "https://example.com"
        }

        GET Ping
            path ["ping"]
            -> Json<String>
    });

    resolved.endpoints[0].scope_modules = (0..=crate::limits::MAX_DSL_SCOPE_DEPTH)
        .map(|idx| quote::format_ident!("scope_{idx}"))
        .collect();

    let out = emit(resolved)
        .to_string()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();

    assert!(
        out.contains("DSLscopenestingexceedsmaximumsupporteddepthof64"),
        "{out}"
    );
}
