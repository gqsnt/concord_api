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
            "fn plan (& self, plan_ctx : & :: concord_core :: internal :: ClientPlanContext",
            ":: concord_core :: internal :: RequestPlan",
            ":: concord_core :: internal :: EndpointPlan",
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
            "fn plan (& self , plan_ctx : & :: concord_core :: internal :: ClientPlanContext",
            ":: concord_core :: internal :: RequestPlan",
            ":: concord_core :: internal :: EndpointPlan",
            ":: concord_core :: internal :: EndpointMeta",
            ":: concord_core :: internal :: ResolvedRoute",
            ":: concord_core :: internal :: ResolvedPolicy",
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
                retry read {
                    max_attempts 2
                    methods [GET]
                    on [401, 403]
                    retry_after
                }

                rate_limit app {
                    bucket application by [host] {
                        10 / 1s
                    }
                }
            }

            behaviors {
                behavior shared {
                    auth bearer session
                    retry read
                    rate_limit app
                }

                behavior endpoint_override {
                    retry off
                }
            }

            defaults {
                behavior shared
            }
        }

        GET Ping(page?: u64 = 0)
            path ["ping"]
            behavior endpoint_override
            -> Json<String>
    });

    match &resolved.client_policy.retry {
        Some(RetryResolved::Set(config)) => {
            let expected_methods: Vec<syn::Ident> = vec![syn::parse_quote!(GET)];
            assert_eq!(config.max_attempts, 2);
            assert_eq!(config.methods, expected_methods);
            assert_eq!(config.statuses, vec![401, 403]);
            assert!(config.respect_retry_after);
        }
        other => {
            panic!("expected resolved client retry from behavior/default lowering, got {other:?}")
        }
    }
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
            "expected resolved client rate limit from behavior/default lowering, got {other:?}"
        ),
    }

    let endpoint = resolved.endpoints.iter().find(|ep| ep.name == "Ping");
    assert!(endpoint.is_some(), "resolved ping endpoint missing");
    let endpoint = endpoint.unwrap();
    assert_eq!(
        endpoint.behavior_doc.names,
        vec!["shared".to_string(), "endpoint_override".to_string()]
    );
    match &endpoint.policy.endpoint.retry {
        Some(RetryResolved::Clear) => {}
        other => {
            panic!("expected endpoint retry override to clear inherited retry, got {other:?}")
        }
    }

    let out = emit(resolved)
        .to_string()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();

    assert_contains_all(
        &out,
        &[
            "policy.set_retry(::concord_core::advanced::RetryConfig{max_attempts:2u32",
            "::http::Method::GET",
            "::http::StatusCode::from_u16(401u16)",
            "::http::StatusCode::from_u16(403u16)",
            "policy.clear_retry();",
            "policy.add_rate_limit(::concord_core::advanced::RateLimitPlan::from_buckets",
            "RateLimitBucketUse::new(\"application\",\"app_0\"",
        ],
    );
    assert!(!out.contains("policy.retry().cloned().unwrap_or_default()"));
    assert!(!out.contains("__retry.max_attempts"));
    assert!(!out.contains("__retry.methods"));
}
