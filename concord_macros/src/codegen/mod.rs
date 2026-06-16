//! Code generation for resolved Concord APIs.
//!
//! This layer receives `ResolvedApi` and emits client wrappers, facade methods,
//! auth state, endpoint structs, and endpoint `plan()` implementations. It must
//! not inspect raw parser structs or raw scope stacks.

use crate::emit_helpers;
use crate::model::SetOp;
use crate::model::facade::{
    FacadeCredentialMethods, FacadeDoc, FacadeEndpoint, FacadeIr, FacadeMethod, FacadeScope,
    FacadeSetter, build_facade_ir,
};
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
        &format!("{}AcquireAs{}Ext", client, pascal),
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
    let facade_ir = build_facade_ir(&resolved_api);
    emit_resolved(resolved_api, &facade_ir)
}

fn emit_resolved(resolved_api: ResolvedApi, facade_ir: &FacadeIr) -> TokenStream2 {
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
    let client_wrapper =
        emit_client_wrapper(&resolved_api, facade_ir, &vars_ty, &auth_vars_ty, &cx_ty);
    let internal_mod = emit_internal(&resolved_api, &vars_ty, &auth_vars_ty, &cx_ty);
    let endpoints_mod = emit_endpoints(&resolved_api, facade_ir, &cx_ty);
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
    use crate::model::facade::SetterForm;
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

    fn generated_doc_attrs(expanded: &str) -> Vec<&str> {
        let mut docs = Vec::new();
        let mut rest = expanded;
        while let Some(start) = rest.find("#[doc=\"") {
            let after_start = &rest[start + "#[doc=\"".len()..];
            let Some(end) = after_start.find("\"]") else {
                break;
            };
            docs.push(&after_start[..end]);
            rest = &after_start[end + 2..];
        }
        docs
    }

    fn assert_generated_doc_attrs_do_not_contain(expanded: &str, needle: &str) {
        for doc in generated_doc_attrs(expanded) {
            assert!(
                !doc.contains(needle),
                "generated rustdoc `{doc}` must not contain `{needle}`"
            );
        }
    }

    fn assert_generated_doc_attrs_do_not_expose_hidden_names(expanded: &str) {
        for needle in ["__", "EpSearch", "EpCreate"] {
            assert_generated_doc_attrs_do_not_contain(expanded, needle);
        }
    }

    #[test]
    fn codegen_does_not_import_raw_ast_or_removed_part_models() {
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
            ["crate", "::", "parse"].concat(),
            ["use", "crate", "::", "ast"].concat(),
            ["use", "crate", "::", "parse"].concat(),
            ["Raw", "Api"].concat(),
            ["Raw", "Endpoint"].concat(),
            ["Raw", "Scope"].concat(),
            ["Raw", "Item"].concat(),
            ["Client", "Def"].concat(),
            ["Layer", "Def"].concat(),
            ["Endpoint", "Def"].concat(),
            ["Auth", "Block"].concat(),
            ["Retry", "Profiles", "Block"].concat(),
            ["Cache", "Profiles", "Block"].concat(),
            ["Rate", "Limit", "Profiles", "Block"].concat(),
            ["Route", "Part"].concat(),
            ["Policy", "Part"].concat(),
            ["Auth", "Part"].concat(),
            ["Body", "Part"].concat(),
            ["Pagination", "Part"].concat(),
            ["Route", "Part"].concat(),
            ["Policy", "Part"].concat(),
            ["Auth", "Part"].concat(),
            ["Body", "Part"].concat(),
            ["Pagination", "Part"].concat(),
            ["with", "_", "configure"].concat(),
        ];

        for (path, body) in files {
            for needle in &forbidden {
                assert!(
                    !body.contains(needle.as_str()),
                    "codegen file `{path}` must not contain raw/removed symbol `{needle}`"
                );
            }
        }
    }

    #[test]
    fn facade_ir_contains_endpoint_target_metadata() {
        let resolved = crate::sema::analyze_tokens_for_test(quote! {
            client FacadeMeta {
                base "https://example.com"
            }

            scope teams(team_id: u64) {
                path ["teams", team_id]

                POST Create(name: String, tag?: String, body: Json<CreateBody>)
                    as create_team
                    path ["items", name]
                    query {
                        tag
                    }
                    -> Json<CreateResponse>
            }
        });
        let ir = build_facade_ir(&resolved);

        assert_eq!(ir.client_name, "FacadeMeta");
        assert_eq!(ir.endpoints.len(), 1);

        let endpoint = &ir.endpoints[0];
        assert_eq!(endpoint.target_endpoint, "teams::Create");
        assert_eq!(endpoint.public_method, "create_team");
        assert_eq!(endpoint.scope_path, vec!["teams"]);
        assert_eq!(
            endpoint
                .required_args
                .iter()
                .map(|arg| (arg.name.as_str(), arg.ty.as_str()))
                .collect::<Vec<_>>(),
            vec![("name", "String"), ("body", "CreateBody")]
        );
        assert!(
            !endpoint
                .required_args
                .iter()
                .any(|arg| arg.name == "team_id"),
            "captured scope params must not appear in endpoint facade args"
        );

        let tag = endpoint
            .setters
            .iter()
            .find(|setter| setter.field == "tag")
            .expect("tag setter metadata");
        assert_eq!(tag.ty, "String");
        assert_eq!(
            tag.forms,
            vec![SetterForm::Set, SetterForm::SetOptional, SetterForm::Clear]
        );
    }

    #[test]
    fn facade_ir_contains_scope_method_metadata() {
        let resolved = crate::sema::analyze_tokens_for_test(quote! {
            client ScopeMeta {
                base "https://example.com"
            }

            scope regional(region: String, locale?: String) {
                path ["regional", region]

                scope teams(team_id: u64) {
                    path ["teams", team_id]

                    GET Show
                        as show
                        path ["show"]
                        -> Json<String>
                }
            }
        });
        let ir = build_facade_ir(&resolved);
        let regional = ir
            .scopes
            .iter()
            .find(|scope| scope.path == ["regional"])
            .expect("regional scope metadata");
        assert_eq!(regional.public_method, "regional");
        assert_eq!(regional.rust_type_name, "ScopeMetaRegionalScope");
        assert_eq!(regional.parent_path, Vec::<String>::new());
        assert_eq!(
            regional
                .decls
                .iter()
                .map(|var| var.rust.to_string())
                .collect::<Vec<_>>(),
            vec!["region", "locale"]
        );
        let locale = regional
            .setters
            .iter()
            .find(|setter| setter.field == "locale")
            .expect("scope setter metadata");
        assert_eq!(locale.set_name, "locale");
        assert_eq!(locale.clear_name, "clear_locale");
        assert!(locale.set_doc.contains("scope parameter"));
        assert_eq!(regional.methods.len(), 1);
        assert_eq!(regional.methods[0].public_name, "teams");
        assert_eq!(
            regional.methods[0].target_scope_path,
            vec!["regional".to_string(), "teams".to_string()]
        );
        assert_eq!(
            regional.methods[0].target_scope_type_name,
            "ScopeMetaRegionalTeamsScope"
        );
        assert!(
            regional.methods[0].docs[0]
                .summary
                .contains("regional::teams")
        );
    }

    #[test]
    fn facade_ir_contains_endpoint_setter_metadata() {
        let resolved = crate::sema::analyze_tokens_for_test(quote! {
            client SetterMeta {
                base "https://example.com"
            }

            GET Search(filter?: String, count: u64 = 20)
                as search
                path ["search"]
                query { filter, count }
                -> Json<Vec<String>>
        });
        let ir = build_facade_ir(&resolved);
        let endpoint = ir
            .endpoints
            .iter()
            .find(|endpoint| endpoint.target_endpoint == "Search")
            .expect("search endpoint metadata");
        let filter = endpoint
            .setters
            .iter()
            .find(|setter| setter.field == "filter")
            .expect("filter setter metadata");
        assert_eq!(filter.set_name, "filter");
        assert_eq!(filter.set_optional_name, "filter_opt");
        assert_eq!(filter.clear_name, "clear_filter");
        assert!(filter.set_doc.contains("optional query parameter"));

        let count = endpoint
            .setters
            .iter()
            .find(|setter| setter.field == "count")
            .expect("count setter metadata");
        assert_eq!(count.set_optional_name, "count_opt");
        assert_eq!(count.clear_name, "clear_count");
        assert!(count.clear_doc.contains("default `20`"));
    }

    #[test]
    fn generated_setter_names_and_docs_come_from_facade_ir() {
        let resolved = crate::sema::analyze_tokens_for_test(quote! {
            client SetterSource {
                base "https://example.com"
            }

            GET Search(filter?: String)
                as search
                path ["search"]
                query { filter }
                -> Json<Vec<String>>
        });
        let mut ir = build_facade_ir(&resolved);
        let setter = ir.endpoints[0]
            .setters
            .iter_mut()
            .find(|setter| setter.field == "filter")
            .expect("filter setter metadata");
        setter.set_name = "with_filter_from_ir".to_string();
        setter.set_optional_name = "with_filter_opt_from_ir".to_string();
        setter.clear_name = "without_filter_from_ir".to_string();
        setter.set_doc = "IR supplied set doc.".to_string();

        let out = emit_resolved(resolved, &ir)
            .to_string()
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>();
        assert_contains_all(
            &out,
            &[
                "pub fn with_filter_from_ir ( mut self , v : String ) -> Self",
                "fn with_filter_opt_from_ir ( self , value : :: core :: option :: Option < String > ) -> Self",
                "fn without_filter_from_ir ( self ) -> Self",
                "#[doc = \"IR supplied set doc.\"]",
            ],
        );
        assert!(!out.contains("filter_opt(self"));
        assert!(!out.contains("clear_filter(self"));
    }

    #[test]
    fn generated_scope_default_capture_is_applied_without_public_endpoint_setter() {
        let out = expanded(quote! {
            client ScopeDefaultCapture {
                base "https://example.com"
            }

            scope localized(locale: String = "en_US".to_string()) {
                path ["data", locale]

                GET List
                    as list
                    path ["list.json"]
                    -> Json<Vec<String>>
            }
        })
        .to_string();

        assert!(
            out.contains("__ep . locale = self . locale")
                || out.contains("__ep.locale=self.locale"),
            "captured defaulted scope params must be assigned internally"
        );
        assert!(
            !out.contains("__ep = __ep . locale") && !out.contains("__ep=__ep.locale"),
            "captured defaulted scope params must not call public endpoint setters"
        );
    }

    #[test]
    fn codegen_does_not_recompute_endpoint_setter_names() {
        let endpoint_codegen =
            std::fs::read_to_string("src/codegen/endpoints/endpoint.rs").expect("endpoint codegen");
        let wrapper_codegen =
            std::fs::read_to_string("src/codegen/endpoints/wrapper.rs").expect("wrapper codegen");
        let facade_codegen = format!("{endpoint_codegen}\n{wrapper_codegen}");
        for forbidden in [
            "format!(\"{}_opt\"",
            "format!(\"{f}_opt\"",
            "format!(\"clear_{}\"",
            "format!(\"clear_{f}\"",
        ] {
            assert!(
                !facade_codegen.contains(forbidden),
                "facade codegen must get public setter names from FacadeIr, found `{forbidden}`"
            );
        }
    }

    #[test]
    fn generated_client_construction_snapshot_contains_current_api_only() {
        let out = expanded(quote! {
            client ConstructApi {
                base "https://example.com"
                var tenant: String
                secret api_key: String
                credential key = api_key(secret.api_key)
            }

            GET Ping
                path ["ping"]
                auth header "X-Api-Key" = key
                -> Json<String>
        });

        assert_contains_all(
            &out,
            &[
                "pub struct ConstructApi < T : :: concord_core :: advanced :: Transport = :: concord_core :: advanced :: ReqwestTransport >",
                "pub fn new ( tenant : String , api_key : String ) -> Self",
                "pub fn builder () -> ConstructApiBuilder",
                "pub struct ConstructApiBuilder",
                "tenant : :: core :: option :: Option < String >",
                "api_key : :: core :: option :: Option < String >",
                "pub fn build ( self ) -> :: core :: result :: Result < ConstructApi < :: concord_core :: advanced :: ReqwestTransport > , :: concord_core :: prelude :: ApiClientError >",
                "ApiClientError :: invalid_param (__ctx . clone () , \"builder.tenant\")",
                "ApiClientError :: invalid_param (__ctx . clone () , \"builder.api_key\")",
                "pub fn configure ( mut self , f : impl FnOnce (& mut :: concord_core :: advanced :: RuntimeConfig)) -> Self",
                "pub fn configure_mut (& mut self , f : impl FnOnce (& mut :: concord_core :: advanced :: RuntimeConfig)) -> & mut Self",
            ],
        );
        assert!(
            !out.contains("with_configure"),
            "generated client must not expose with_configure"
        );
    }

    #[test]
    fn generated_facade_scopes_use_clean_public_names_and_rustdoc() {
        let out = expanded(quote! {
            client CleanFacadeApi {
                base "https://example.com"
            }

            scope regional(region: String) {
                host [region, "api"]

                scope match_api_matches {
                    path ["lol", "match", "api", "matches"]

                    GET GetMatchIdsByPuuid(puuid: String)
                        as ids_by_puuid
                        path ["by-puuid", puuid, "ids"]
                        -> Json<Vec<String>>
                }
            }
        });

        assert_contains_all(
            &out,
            &[
                "#[doc = \"Enter the `regional` facade scope.\"]",
                "pub fn regional (& self , region : String) -> CleanFacadeApiRegionalScope",
                "#[doc = \"Facade handle for the `regional` scope.\"]",
                "pub struct CleanFacadeApiRegionalScope",
                "#[doc = \"Enter the `regional::match_api_matches` facade scope.\"]",
                "pub fn match_api_matches ( self",
                "CleanFacadeApiRegionalMatchApiMatchesScope",
                "#[doc = \"Facade handle for the `regional::match_api_matches` scope.\"]",
                "pub fn ids_by_puuid ( self , puuid : String )",
            ],
        );
        let hidden_facade = out.contains("__Facade");
        let hidden_scope = out.contains("__Scope");
        let hidden_context = out
            .find("__Facade")
            .map(|idx| &out[idx.saturating_sub(80)..out.len().min(idx + 160)])
            .unwrap_or("");
        assert!(
            !hidden_facade && !hidden_scope,
            "generated facade scope surface must not expose hidden facade/scope names: __Facade={hidden_facade}, __Scope={hidden_scope}; context={hidden_context}"
        );
    }

    #[test]
    fn generated_endpoint_setters_use_field_opt_and_clear_names() {
        let out = expanded(quote! {
            client SetterApi {
                base "https://example.com"
            }

            GET Search(q: String, filter?: String, count: u32 = 20)
                as search
                path ["search"]
                query {
                    q
                    filter
                    count
                }
                -> Json<Vec<String>>
        });

        assert_contains_all(
            &out,
            &[
                "pub fn search (& self , q : String)",
                "pub fn filter ( mut self , v : String ) -> Self",
                "pub fn filter_opt ( mut self , v : :: core :: option :: Option < String > ) -> Self",
                "pub fn clear_filter ( mut self ) -> Self",
                "pub fn count ( mut self , v : u32 ) -> Self",
                "pub fn count_opt ( mut self , v : :: core :: option :: Option < u32 > ) -> Self",
                "v . unwrap_or_else (|| 20)",
                "pub fn clear_count ( mut self ) -> Self",
                "fn filter_opt ( self , value : :: core :: option :: Option < String > ) -> Self",
                "fn count_opt ( self , value : :: core :: option :: Option < u32 > ) -> Self",
                "#[doc = \"Set defaulted query parameter `count` from an Option; None resets to the default `20`.\"]",
            ],
        );
        assert!(
            !out.contains("maybe_filter") && !out.contains("reset_count"),
            "generated endpoint setters must not expose removed maybe_/reset_ names"
        );
    }

    #[test]
    fn generated_explicit_endpoint_api_is_clean_and_matches_facade_target() {
        let out = expanded(quote! {
            client ExplicitApi {
                base "https://example.com"
            }

            GET Get(id: u64, filter?: String, count: u32 = 20)
                as get
                path ["items", id]
                query {
                    filter
                    count
                }
                -> Json<String>
        });

        assert_contains_all(
            &out,
            &[
                "pub mod endpoints",
                "as Get",
                "#[doc = \"Advanced explicit endpoint request. Prefer facade methods for normal use.\"]",
                "pub fn new ( id : u64 ) -> Self",
                "pub fn filter ( mut self , v : String ) -> Self",
                "pub fn filter_opt ( mut self , v : :: core :: option :: Option < String > ) -> Self",
                "pub fn clear_filter ( mut self ) -> Self",
                "pub fn count ( mut self , v : u32 ) -> Self",
                "pub fn count_opt ( mut self , v : :: core :: option :: Option < u32 > ) -> Self",
                "pub fn clear_count ( mut self ) -> Self",
                "pub fn get (& self , id : u64)",
                "let mut __ep = endpoints :: Get :: new (id)",
                "self . request (__ep)",
            ],
        );
        assert!(
            !out.contains("pubfnid(mutself"),
            "explicit endpoints must not expose setters for required direct args"
        );
    }

    #[test]
    fn generated_facade_methods_return_core_pending_request_surface() {
        let out = expanded(quote! {
            client PendingApi {
                base "https://example.com"
            }

            GET Ping
                as ping
                path ["ping"]
                -> Json<String>

            GET List(count: u64 = 20)
                as list
                path ["items"]
                query {
                    count
                }
                paginate OffsetLimitPagination {
                    offset = 0,
                    limit = count
                }
                -> Json<Vec<String>>
        });

        assert_contains_all(
            &out,
            &[
                "pub fn ping (& self ,) -> :: concord_core :: prelude :: PendingRequest",
                "pub fn list (& self ,) -> :: concord_core :: prelude :: PendingRequest",
                "self . request (__ep)",
                "pub mod endpoints",
                "as Ping",
                "as List",
                "pub use pending_api :: PingRequestExt",
                "pub use pending_api :: ListRequestExt",
            ],
        );
    }

    #[test]
    fn generated_minimal_api_snapshot_contains_facade_and_endpoint_plan() {
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
                ":: concord_core :: internal :: ResponsePlan",
            ],
        );
    }

    #[test]
    fn generated_endpoint_plan_snapshot_contains_plan_based_core_contract() {
        let out = expanded(quote! {
            client PlanApi {
                base "https://example.com"
                var tenant: String
                secret token: String
                credential key = api_key(secret.token)
            }

            POST Create(id: String, limit: u64 = 20, body: Json<CreateBody>)
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
                ":: concord_core :: internal :: BodyPlan :: Encoded",
                ":: concord_core :: internal :: RequestArgs { body : :: core :: option :: Option :: Some",
                ":: concord_core :: internal :: ResponsePlan",
                ":: concord_core :: internal :: PaginationPlan :: from (ctrl)",
            ],
        );
    }

    #[test]
    fn generated_route_snapshot_builds_resolved_route_from_semantic_pieces() {
        let out = expanded(quote! {
            client RoutePlanApi {
                base "https://example.com"
                var region: String
            }

            scope regional {
                host [vars.region, "api"]
                path ["v1"]

                GET Show(id: String)
                    path ["items", id]
                    -> Json<String>
            }
        });

        assert_contains_all(
            &out,
            &[
                "let mut route = < super :: RoutePlanApiCx as :: concord_core :: prelude :: ClientContext > :: base_route (vars , auth)",
                "route.host_mut().push",
                "route.path_mut().push_raw(\"v1\")",
                "route.path_mut().push_raw(\"items\")",
                "route.path_mut().push_segment_encoded(&__segment)",
                "route.host().validate(ctx_err.clone())",
                "scheme : < super :: RoutePlanApiCx as :: concord_core :: prelude :: ClientContext > :: SCHEME",
                "host : route.host().join",
                "path : route.path().as_str().to_string()",
            ],
        );
    }

    #[test]
    fn generated_policy_snapshot_materializes_resolved_policy() {
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
                "policy . set_layer (:: concord_core :: internal :: PolicyLayer :: Endpoint)",
                "policy.set_query(\"q\"",
                "policy.insert_header",
                "HeaderName :: from_bytes (\"X-Endpoint\" . as_bytes ())",
                "HeaderValue :: from_static (\"search\")",
                ":: concord_core :: advanced :: AuthRequirement",
                "policy.ensure_accept",
                "let (headers , query , timeout , cache , retry , mut rate_limit) = policy.into_parts()",
                "rate_limit.canonicalize()",
                "let __resolved_policy = :: concord_core :: internal :: ResolvedPolicy",
                "auth : __auth_plan",
            ],
        );
    }

    #[test]
    fn generated_response_snapshot_contains_decode_and_body_plan() {
        let out = expanded(quote! {
            client ResponsePlanApi {
                base "https://example.com"
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
                "fn __decode_",
                "< Json < LoginResponse > as :: concord_core :: advanced :: ResponseCodec > :: decode",
                "let r : LoginResponse = decoded",
                "let value : AccessToken = (AccessToken :: new (r . access_token))",
                "let __body_value = self . body . lock ()",
                "< Json < LoginRequest > as :: concord_core :: advanced :: BodyCodec > :: encode (__body_value",
                "BodyPlan :: Encoded",
                "let (__body_bytes , __body_format) = __encoded_body . into_parts ()",
                "content_type : < Json < LoginRequest > as :: concord_core :: advanced :: BodyCodec > :: content_type ()",
                "format : __body_format",
                "ResponsePlan",
                "decode : __decode_",
            ],
        );
    }

    #[test]
    fn generated_pagination_plan_snapshot_contains_all_controller_shapes() {
        let out = expanded(quote! {
            client PaginationPlanApi {
                base "https://example.com"
            }

            GET Offset(start: u64 = 0, count: u64 = 20)
                path ["offset"]
                query {
                    start
                    count
                }
                paginate OffsetLimitPagination {
                    offset = start,
                    limit = count
                }
                -> Json<Vec<String>>

            GET Cursor(cursor?: String, count: u64 = 20)
                path ["cursor"]
                query {
                    cursor
                    count
                }
                paginate CursorPagination {
                    cursor = cursor,
                    per_page = count
                }
                -> Json<Vec<String>>

            GET Paged(page: u64 = 1, count: u64 = 20)
                path ["paged"]
                query {
                    page
                    count
                }
                paginate PagedPagination {
                    page = page,
                    per_page = count
                }
                -> Json<Vec<String>>
        });

        assert_contains_all(
            &out,
            &[
                "let mut ctrl : OffsetLimitPagination = :: core :: default :: Default :: default ()",
                "ctrl . offset_key = :: std :: borrow :: Cow :: from (\"start\")",
                "ctrl . limit_key = :: std :: borrow :: Cow :: from (\"count\")",
                "let mut ctrl : CursorPagination = :: core :: default :: Default :: default ()",
                "ctrl . cursor_key = :: std :: borrow :: Cow :: from (\"cursor\")",
                "ctrl . per_page_key = :: std :: borrow :: Cow :: from (\"count\")",
                "let mut ctrl : PagedPagination = :: core :: default :: Default :: default ()",
                "ctrl . page_key = :: std :: borrow :: Cow :: from (\"page\")",
                ":: concord_core :: internal :: PaginationPlan :: from (ctrl)",
            ],
        );
    }

    #[test]
    fn generated_rustdoc_snapshot_covers_client_endpoint_and_request_builder() {
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
                "#[doc=\"Query params: `count`\"]",
                "#[doc=\"Response: Json<String>\"]",
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
    fn behavior_doc_line_formats_labels_in_order() {
        assert_eq!(
            behavior_doc_line(&["client_read".to_string(), "endpoint_read".to_string()]),
            Some("Behavior: `client_read`, `endpoint_read`".to_string())
        );
        assert_eq!(behavior_doc_line(&[]), None);
    }

    #[test]
    fn generated_rustdoc_snapshot_includes_behavior_names() {
        let out = expanded(quote! {
            client BehaviorDocs {
                base "https://example.com"

                behavior client_read {
                    retry off
                }

                behavior scope_read {
                    retry off
                }

                behavior endpoint_read {
                    retry off
                }

                defaults {
                    behavior client_read
                }
            }

            scope users {
                path ["users"]
                behavior scope_read

                GET Me
                    path ["me"]
                    behavior endpoint_read
                    -> Json<()>
            }
        });

        assert_contains_all(
            &out,
            &["#[doc=\"Behavior: `client_read`, `scope_read`, `endpoint_read`\"]"],
        );
    }

    #[test]
    fn generated_rustdoc_snapshot_includes_endpoint_contract_without_secret_values() {
        let out = expanded(quote! {
            client SnapshotRichDocs {
                base "https://example.com"
                var tenant: String
                secret api_key: String
                credential key = api_key(secret.api_key)

                default {
                    retry read
                    cache standard
                    rate_limit app
                }

                retry read {
                    max_attempts 2
                    methods [GET, POST]
                }

                cache standard {
                    ttl 30s
                    revalidate
                }

                rate_limit app {
                    bucket application by [host] {
                        10 / 1s
                    }
                }
            }

            POST Create(id: String, filter?: String, count: u64 = 20, body: Json<CreateBody>)
                path ["items", id]
                query {
                    filter
                    count
                }
                headers {
                    "X-Tenant" = vars.tenant
                }
                auth header "X-Api-Key" = key
                paginate OffsetLimitPagination {
                    offset = 0,
                    limit = count
                }
                -> Json<CreateResponse>
        });

        assert_contains_all(
            &out,
            &[
                "#[doc=\"POST / items / {id}\"]",
                "#[doc=\"Required params: `id`\"]",
                "#[doc=\"Query params: `count`, `filter`\"]",
                "#[doc=\"Headers: `X-Tenant`\"]",
                "#[doc=\"Auth:\"]",
                "#[doc=\"- header `X-Api-Key` = `key`\"]",
                "#[doc=\"Cache: configured\"]",
                "#[doc=\"Retry: configured\"]",
                "#[doc=\"Rate limit: configured\"]",
                "#[doc=\"Pagination: OffsetLimitPagination\"]",
                "#[doc=\"Body: Json<CreateBody>\"]",
                "#[doc=\"Response: Json<CreateResponse>\"]",
                "#[doc=\"Set optional query parameter `filter`.\"]",
                "#[doc=\"Set defaulted query parameter `count` (default: `20`).\"]",
                "#[doc=\"Reset defaulted query parameter `count` to its default `20`.\"]",
            ],
        );
        assert_generated_doc_attrs_do_not_expose_hidden_names(&out);
        assert_generated_doc_attrs_do_not_contain(&out, "api_key");
    }

    #[test]
    fn generated_auth_session_snapshot_contains_auth_state_and_acquire_sugar() {
        let out = expanded(quote! {
            client SnapshotAuth {
                base "https://example.com"
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
                "pub fn session (& self) -> SnapshotAuthSessionAuth",
                "pub fn auth_state (& self) -> SnapshotAuthAuth",
                "pub async fn acquire < R >",
                "pub async fn set (& self , value : AccessToken)",
                "pub async fn clear (& self)",
                "pub async fn is_set (& self) -> bool",
                "pub async fn acquire_auth_session",
                "pub async fn set_auth_session_value",
                "pub async fn clear_auth_session",
                "pub async fn has_auth_session",
                "pub trait SnapshotAuthAcquireAsSessionExt",
                "fn acquire_as_session (self,) -> :: core :: pin :: Pin",
                ". with_missing_hint (\"client.acquire_auth_session(...)\")",
                ":: concord_core :: advanced :: AuthPlacement :: Bearer",
                ":: concord_core :: advanced :: AuthPlacement :: Header (\"X-Upstream-Key\")",
            ],
        );
    }

    #[test]
    fn generated_pagination_snapshot_contains_pagination_plan() {
        let out = expanded(quote! {
            client SnapshotPagination {
                base "https://example.com"
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
    fn generated_route_snapshot_rejects_dynamic_slash_segments() {
        let out = expanded(quote! {
            client SnapshotRouteGuard {
                base "https://example.com"
            }

            GET Show(id: String, prefix?: String)
                as show
                path ["users", id, fmt["p-", prefix]]
                -> Json<String>
        });

        assert_contains_all(
            &out,
            &[
                "__segment.contains('/')",
                "__segment.contains('\\\\')",
                "ApiClientError::invalid_param(ctx.clone()",
                "route.path_mut().push_segment_encoded(&__segment)",
            ],
        );
    }

    #[test]
    fn generated_query_snapshot_contains_optional_and_empty_string_semantics() {
        let out = expanded(quote! {
            client SnapshotQueryPolicy {
                base "https://example.com"
            }

            GET Search(maybe?: String)
                as search
                path ["search"]
                query {
                    "maybe" = maybe,
                    "empty" = ""
                }
                -> Json<String>
        });

        assert_contains_all(
            &out,
            &[
                "if let ::core::option::Option::Some(__v) = ep.maybe.as_ref()",
                "policy.remove_query(\"maybe\")",
                "policy.set_query(\"empty\",(\"\").to_string())",
            ],
        );
    }

    #[test]
    fn generated_rate_limit_snapshot_contains_runtime_plan() {
        let out = expanded(quote! {
            client SnapshotRateLimit {
                base "https://example.com"

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
                base "https://example.com"
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
