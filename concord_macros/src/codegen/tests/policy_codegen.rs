use super::helpers::*;
use crate::model::facade::profile_doc_line;
use quote::quote;

#[test]
fn generated_policy_materializes_resolved_policy() {
    let out = expanded(quote! {
        client PolicyPlanApi {
            base "https://example.com"
            var tenant: String
            secret token: String
            credential key = api_key(secret.token)

            headers {
                "X-Client" = vars.tenant
            }
        }

        GET Search(q: String)
            path ["search"]
            query {
                q
            }
            headers {
                "X-Endpoint" = "search"
            }
            auth header "X-Api-Key" = key
            -> Json<String>
    });

    assert_contains_all(
        &out,
        &[
            "let mut policy = < super :: PolicyPlanApiCx as :: concord_core :: prelude :: ClientContext > :: base_policy",
            "policy . set_layer (:: concord_core :: __private :: PolicyLayer :: Endpoint)",
            "policy.set_query(\"q\"",
            "policy.insert_header",
            "HeaderName :: from_bytes (\"X-Endpoint\" . as_bytes ())",
            "HeaderValue :: from_static (\"search\")",
            ":: concord_core :: __private :: AuthRequirement",
            "policy.ensure_accept",
            "let (headers , query , timeout , mut rate_limit) = policy.into_parts()",
            "rate_limit.canonicalize()",
            "let __resolved_policy = :: concord_core :: __private :: ResolvedPolicy",
            "auth : __auth_plan",
        ],
    );
}

#[test]
fn profile_doc_line_formats_labels_in_order() {
    assert_eq!(
        profile_doc_line(&["client_read".to_string(), "endpoint_read".to_string()]),
        Some("Profile: `client_read`, `endpoint_read`".to_string())
    );
    assert_eq!(profile_doc_line(&[]), None);
}

#[test]
fn behavior_profiles_do_not_reach_runtime_codegen() {
    let alpha = expanded(quote! {
        client BehaviorCodegen {
            base "https://example.com"
            secret token: String
            credential session = api_key(secret.token)

            rate_limit app {
                bucket application by [host] {
                    1 / 1s
                }
            }

            profiles {
                    profile alpha {
                        auth header "X-Profile-Token" = session
                        rate_limit app
                    }
            }

            default {
                profile alpha
            }
        }

            GET Ping
                path ["ping"]
                -> Json<()>
    });
    let beta = expanded(quote! {
        client BehaviorCodegen {
            base "https://example.com"
            secret token: String
            credential session = api_key(secret.token)

            rate_limit app {
                bucket application by [host] {
                    1 / 1s
                }
            }

            profiles {
                    profile beta {
                        auth header "X-Profile-Token" = session
                        rate_limit app
                    }
            }

            default {
                profile beta
            }
        }

            GET Ping
                path ["ping"]
                -> Json<()>
    });

    assert_contains_all(
        &alpha,
        &["#[doc=\"Profile: `alpha`\"]", "policy.add_rate_limit"],
    );
    assert_contains_all(
        &beta,
        &["#[doc=\"Profile: `beta`\"]", "policy.add_rate_limit"],
    );
    assert_eq!(without_doc_attrs(&alpha), without_doc_attrs(&beta));
}

#[test]
fn rustdoc_behavior_label_dedup_does_not_affect_policy() {
    let out = expanded(quote! {
        client LabelDedup {
            base "https://example.com"

            rate_limit read_limit {
                bucket read by [host] {
                    1 / 1s
                }
            }

            profiles {
                profile read {
                    rate_limit read_limit
                }
            }

            default {
                profile read
            }
        }

        scope users {
            path ["users"]
            profile read

            GET Me
                path ["me"]
                profile read
                -> Json<()>
        }
    });

    assert_contains_all(&out, &["#[doc=\"Profile: `read`\"]"]);
    assert_contains_all(&out, &["policy.add_rate_limit"]);
    let profile_doc_lines = generated_doc_attrs(&out)
        .into_iter()
        .filter(|doc| doc.contains("Profile:`"))
        .collect::<Vec<_>>();
    assert_eq!(profile_doc_lines.len(), 2);
    assert_eq!(
        out.match_indices("RateLimitBucketUse::new(\"read\",\"read_limit_0\"")
            .count(),
        3
    );
}
