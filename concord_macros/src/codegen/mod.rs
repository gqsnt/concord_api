//! Code generation for resolved v5 APIs.
//!
//! This layer receives `ResolvedApi` and emits client wrappers, facade methods,
//! auth state, endpoint structs, and endpoint `plan()` implementations. It must
//! not inspect raw parser structs or raw scope stacks.

use crate::emit_helpers;
use crate::model::SetOp;
use crate::sema::*;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Ident, LitStr};

#[inline]
fn client_prefixed_ident(client: &Ident, suffix: &str) -> Ident {
    // Example: RiotClient + "Vars" => RiotClientVars
    emit_helpers::ident(&format!("{}{}", client, suffix), client.span())
}

fn acquire_as_trait_ident(client: &Ident, credential: &Ident) -> Ident {
    let mut pascal = String::new();
    let mut upper_next = true;
    for ch in credential.to_string().chars() {
        if ch == '_' || ch == '-' {
            upper_next = true;
            continue;
        }
        if upper_next {
            pascal.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            pascal.push(ch);
        }
    }
    emit_helpers::ident(
        &format!("__{}AcquireAs{}Ext", client, pascal),
        credential.span(),
    )
}

#[inline]
fn value_uses_auth(v: &ValueKind) -> bool {
    match v {
        ValueKind::AuthField(_) => true,
        ValueKind::Fmt(fmt) => fmt.pieces.iter().any(|p| {
            matches!(
                p,
                FmtResolvedPiece::Var {
                    source: FmtVarSource::Auth,
                    ..
                }
            )
        }),
        _ => false,
    }
}

#[inline]
fn policy_uses_auth(policy: &PolicyBlocksResolved) -> bool {
    let ops_use = |op: &PolicyOp| match op {
        PolicyOp::Set { value, .. } => value_uses_auth(value),
        _ => false,
    };
    policy.headers.iter().any(ops_use)
        || policy.query.iter().any(ops_use)
        || policy.timeout.as_ref().is_some_and(value_uses_auth)
}

fn ep_optionals(ep: &ResolvedEndpoint) -> std::collections::BTreeMap<String, bool> {
    ep.vars
        .iter()
        .map(|v| (v.rust.to_string(), v.optional))
        .collect()
}

pub fn emit(resolved_api: ResolvedApi) -> TokenStream2 {
    let mod_name = resolved_api.mod_name.clone();
    let scheme = emit_scheme(resolved_api.scheme);
    let domain = resolved_api.domain.clone();

    let vars_ty = client_prefixed_ident(&resolved_api.client_name, "Vars");
    let auth_inner_ty = client_prefixed_ident(&resolved_api.client_name, "AuthInner");
    let auth_vars_ty = client_prefixed_ident(&resolved_api.client_name, "AuthVars");
    let auth_state_ty = client_prefixed_ident(&resolved_api.client_name, "AuthState");
    let cx_ty = client_prefixed_ident(&resolved_api.client_name, "Cx");

    let vars_struct = emit_client_vars(&resolved_api.client_vars, &vars_ty);
    let auth_vars_struct = emit_client_auth_vars(
        &resolved_api.client_auth_vars,
        &auth_inner_ty,
        &auth_vars_ty,
    );
    let auth_state_struct = emit_client_auth_state(&resolved_api, &auth_state_ty, &cx_ty);
    let cx_struct = emit_client_context(ClientContextEmit {
        scheme: &scheme,
        domain: &domain,
        resolved_api: &resolved_api,
        policy: &resolved_api.client_policy,
        vars_ty: &vars_ty,
        auth_vars_ty: &auth_vars_ty,
        auth_state_ty: &auth_state_ty,
        cx_ty: &cx_ty,
    });
    let client_wrapper = emit_client_wrapper(&resolved_api, &vars_ty, &auth_vars_ty, &cx_ty);
    let internal_mod = emit_internal(&resolved_api, &vars_ty, &auth_vars_ty, &cx_ty);
    let endpoints_mod = emit_endpoints(&resolved_api, &cx_ty);
    let acquire_trait_imports =
        resolved_api
            .client_auth_credentials
            .iter()
            .filter_map(|credential| {
                let AuthCredentialKindIr::Endpoint { .. } = &credential.kind else {
                    return None;
                };
                let trait_name =
                    acquire_as_trait_ident(&resolved_api.client_name, &credential.name);
                Some(quote! {
                    pub use #mod_name::#trait_name;
                })
            });
    let pending_request_trait_imports = resolved_api.endpoints.iter().map(|ep| {
        let trait_name = endpoint_pending_ext_trait_ident(ep);
        quote! {
            pub use #mod_name::#trait_name;
        }
    });

    quote! {
        mod #mod_name {
            use super::*;

            #vars_struct
            #auth_vars_struct
            #auth_state_struct
            #cx_struct

            #client_wrapper

            #endpoints_mod
            #internal_mod
        }

        #( #acquire_trait_imports )*
        #( #pending_request_trait_imports )*
    }
}

// Keep feature-domain macro chunks in separate files without widening helper visibility.
include!("client.rs");
include!("endpoints/mod.rs");
include!("policy/mod.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;
    use std::path::Path;

    fn expanded(input: TokenStream2) -> String {
        let resolved = crate::sema::analyze_tokens_for_test(input);
        emit(resolved)
            .to_string()
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect()
    }

    fn assert_contains_all(expanded: &str, snippets: &[&str]) {
        for snippet in snippets {
            let compact: String = snippet.chars().filter(|ch| !ch.is_whitespace()).collect();
            assert!(
                expanded.contains(&compact),
                "expanded code did not contain `{snippet}`\n\nexpanded:\n{expanded}"
            );
        }
    }

    #[test]
    fn codegen_does_not_import_raw_ast_or_legacy_part_models() {
        fn visit(path: &Path, contents: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(path).expect("read codegen dir") {
                let entry = entry.expect("read codegen entry");
                let path = entry.path();
                if path.is_dir() {
                    visit(&path, contents);
                } else if path.extension().and_then(|v| v.to_str()) == Some("rs") {
                    let mut body = std::fs::read_to_string(&path).expect("read codegen source");
                    if path.file_name().and_then(|v| v.to_str()) == Some("mod.rs")
                        && let Some((production, _tests)) = body.split_once("#[cfg(test)]")
                    {
                        body = production.to_string();
                    }
                    contents.push((path.display().to_string(), body));
                }
            }
        }

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/codegen");
        let mut files = Vec::new();
        visit(&root, &mut files);

        let forbidden = [
            ["crate", "::", "ast"].concat(),
            ["Client", "Def"].concat(),
            ["Layer", "Def"].concat(),
            ["Endpoint", "Def"].concat(),
            ["Auth", "Block"].concat(),
            ["Retry", "Profiles", "Block"].concat(),
            ["Cache", "Profiles", "Block"].concat(),
            ["Rate", "Limit", "Profiles", "Block"].concat(),
            ["Legacy", "Syntax"].concat(),
            ["Legacy", "Endpoint"].concat(),
            ["Route", "Part"].concat(),
            ["Policy", "Part"].concat(),
            ["Auth", "Part"].concat(),
            ["Body", "Part"].concat(),
            ["Pagination", "Part"].concat(),
        ];

        for (path, body) in files {
            for needle in &forbidden {
                assert!(
                    !body.contains(needle.as_str()),
                    "codegen file `{path}` must not contain raw/legacy symbol `{needle}`"
                );
            }
        }
    }

    #[test]
    fn generated_minimal_api_snapshot_contains_facade_and_endpoint_plan() {
        let out = expanded(quote! {
            client SnapshotMinimal {
                base https "example.com"
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
                ":: concord_core :: internal :: ResponsePlan",
            ],
        );
    }

    #[test]
    fn generated_rustdoc_snapshot_covers_client_endpoint_and_request_builder() {
        let out = expanded(quote! {
            client SnapshotDocs {
                base https "example.com"
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
                "#[doc=\"Create this explicit endpoint request.\"]",
                "#[doc=\"Set this optional request parameter.\"]",
                "#[doc=\"Request-builder extension methods for this endpoint.\"]",
            ],
        );
    }

    #[test]
    fn generated_auth_session_snapshot_contains_auth_state_and_acquire_sugar() {
        let out = expanded(quote! {
            client SnapshotAuth {
                base https "example.com"
                secret upstream_key: String

                credential upstream = api_key(secret.upstream_key)
                credential session = endpoint auth_api::LoginForSession
            }

            scope auth_api {
                POST LoginForSession(body: Json<LoginRequest>)
                    path ["login"]
                    auth header "X-Upstream-Key" = upstream
                    -> Json<LoginResponse>
                    map AccessToken {
                        AccessToken::new(r.access_token)
                    }
            }

            scope protected {
                auth bearer session

                GET Me
                    as me
                    path ["me"]
                    -> Json<User>
            }
        });

        assert_contains_all(
            &out,
            &[
                "pub struct SnapshotAuthAuthState",
                "pub fn session (& self) -> __SnapshotAuthAuthsession",
                "pub trait __SnapshotAuthAcquireAsSessionExt",
                "fn acquire_as_session (self,) -> :: core :: pin :: Pin",
                ":: concord_core :: advanced :: AuthPlacement :: Bearer",
                ":: concord_core :: advanced :: AuthPlacement :: Header (\"X-Upstream-Key\")",
            ],
        );
    }

    #[test]
    fn generated_pagination_snapshot_contains_pagination_plan() {
        let out = expanded(quote! {
            client SnapshotPagination {
                base https "example.com"
            }

            GET List(start: u64 = 0, count: u64 = 20)
                as list
                path ["items"]
                query {
                    start
                    count
                }
                paginate OffsetLimitPagination {
                    offset = start,
                    limit = count
                }
                -> Json<Vec<String>>
        });

        assert_contains_all(
            &out,
            &[
                "let __pagination_plan = :: core :: option :: Option :: Some",
                ":: concord_core :: internal :: PaginationPlan :: from (ctrl)",
                "ctrl . offset_key = :: std :: borrow :: Cow :: from (\"start\")",
                "ctrl . limit_key = :: std :: borrow :: Cow :: from (\"count\")",
            ],
        );
    }

    #[test]
    fn generated_rate_limit_snapshot_contains_runtime_plan() {
        let out = expanded(quote! {
            client SnapshotRateLimit {
                base https "example.com"

                default {
                    rate_limit app
                }

                rate_limit app {
                    bucket application by [host] {
                        10 / 1s
                    }
                }
            }

            GET Ping
                as ping
                path ["ping"]
                -> Json<()>;
        });

        assert_contains_all(
            &out,
            &[
                "policy . add_rate_limit (:: concord_core :: advanced :: RateLimitPlan :: from_buckets",
                "RateLimitBucketUse :: new (\"application\" , \"app_0\"",
                "RateLimitBucketUse :: new",
            ],
        );
    }

    #[test]
    fn generated_mapping_snapshot_contains_final_response_type_and_transform() {
        let out = expanded(quote! {
            client SnapshotMapping {
                base https "example.com"
            }

            POST Login(body: Json<LoginRequest>)
                path ["login"]
                -> Json<LoginResponse>
                map AccessToken {
                    AccessToken::new(r.access_token)
                }
        });

        assert_contains_all(
            &out,
            &[
                "type Response = AccessToken",
                "let value : AccessToken = (AccessToken :: new (r . access_token))",
            ],
        );
    }
}
