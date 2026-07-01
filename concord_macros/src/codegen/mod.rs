//! Code generation for resolved Concord APIs.
//!
//! This layer receives `ResolvedApi` and emits client wrappers, facade methods,
//! auth state, endpoint structs, and endpoint `plan()` implementations. It must
//! not inspect raw parser structs or raw scope stacks.

use crate::emit_helpers;
use crate::model::SetOp;
use crate::model::facade::{
    FacadeConstructorArg, FacadeCredentialMethods, FacadeDoc, FacadeEndpoint, FacadeEndpointTarget,
    FacadeIr, FacadeMethod, FacadeScope, FacadeSetter, build_facade_ir, client_prefixed_type_name,
    generated_acquire_as_trait_type_name,
};
use crate::sema::*;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Ident, LitStr};

#[inline]
fn client_prefixed_ident(client: &Ident, suffix: &str) -> Ident {
    // Example: RiotClient + "Vars" => RiotClientVars
    emit_helpers::ident(&client_prefixed_type_name(client, suffix), client.span())
}

fn acquire_as_trait_ident(client: &Ident, credential: &Ident) -> Ident {
    emit_helpers::ident(
        &generated_acquire_as_trait_type_name(client, credential),
        credential.span(),
    )
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
    let pending_request_trait_imports = resolved_api
        .endpoints
        .iter()
        .zip(facade_ir.endpoints.iter())
        .filter_map(|(ep, facade_ep)| {
            if facade_ep.setters.is_empty() {
                return None;
            }
            let trait_name = endpoint_pending_ext_trait_ident(ep);
            Some(quote! {
                pub use #mod_name::#trait_name;
            })
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
    use crate::model::facade::{FacadeArgKind, FacadeConstructorArg, SetterForm};
    use quote::quote;

    fn expanded(input: TokenStream2) -> String {
        let resolved = crate::sema::analyze_tokens_for_test(input);
        emit(resolved)
            .to_string()
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect()
    }

    fn ident_text(ident: &syn::Ident) -> String {
        ident.to_string()
    }

    fn ident_vec_text(idents: &[syn::Ident]) -> Vec<String> {
        idents.iter().map(ToString::to_string).collect()
    }

    fn type_text(ty: &syn::Type) -> String {
        quote::quote!(#ty).to_string()
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

    fn without_doc_attrs(expanded: &str) -> String {
        let mut out = String::new();
        let mut rest = expanded;
        while let Some(start) = rest.find("#[doc=\"") {
            out.push_str(&rest[..start]);
            let after_start = &rest[start + "#[doc=\"".len()..];
            let Some(end) = after_start.find("\"]") else {
                break;
            };
            rest = &after_start[end + 2..];
        }
        out.push_str(rest);
        out
    }

    #[test]
    fn facade_ir_contains_endpoint_target_metadata() {
        let resolved = crate::sema::analyze_tokens_for_test(quote! {
            client FacadeMeta {
                base "https://example.com"
            }

            scope teams(team_id: u64, locale?: String) {
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

        assert_eq!(ident_text(&ir.client_name), "FacadeMeta");
        assert_eq!(ir.endpoints.len(), 1);

        let endpoint = &ir.endpoints[0];
        assert_eq!(ident_text(&endpoint.target.endpoint), "Create");
        assert_eq!(
            ident_vec_text(&endpoint.target.scope_path),
            vec!["teams".to_string()]
        );
        assert_eq!(ident_text(&endpoint.public_method), "create_team");
        assert_eq!(
            ident_vec_text(&endpoint.scope_path),
            vec!["teams".to_string()]
        );
        assert_eq!(
            endpoint
                .required_args
                .iter()
                .map(|arg| {
                    let ty = &arg.ty;
                    (arg.name.to_string(), type_text(ty), arg.kind)
                })
                .collect::<Vec<_>>(),
            vec![
                (
                    "name".to_string(),
                    "String".to_string(),
                    FacadeArgKind::Value
                ),
                (
                    "body".to_string(),
                    "CreateBody".to_string(),
                    FacadeArgKind::Body
                ),
            ]
        );
        assert_eq!(
            endpoint
                .constructor
                .args
                .iter()
                .map(|arg| match arg {
                    FacadeConstructorArg::PublicArg { name } => name.to_string(),
                    FacadeConstructorArg::CapturedScopeField { name } => {
                        format!("captured:{name}")
                    }
                })
                .collect::<Vec<_>>(),
            vec![
                "captured:team_id".to_string(),
                "name".to_string(),
                "body".to_string(),
            ]
        );
        assert_eq!(
            endpoint
                .captured_setters
                .iter()
                .map(|setter| (setter.field.to_string(), setter.optional))
                .collect::<Vec<_>>(),
            vec![("locale".to_string(), true)]
        );
        assert!(
            !endpoint
                .required_args
                .iter()
                .any(|arg| arg.name == quote::format_ident!("team_id")),
            "captured scope params must not appear in endpoint facade args"
        );

        let tag = endpoint
            .setters
            .iter()
            .find(|setter| setter.field == quote::format_ident!("tag"))
            .expect("tag setter metadata");
        assert_eq!(type_text(&tag.ty), "String");
        assert_eq!(
            tag.forms,
            vec![SetterForm::Set, SetterForm::SetOptional, SetterForm::Clear]
        );
    }

    #[test]
    fn facade_ir_uses_stream_body_for_stream_request_endpoints() {
        let resolved = crate::sema::analyze_tokens_for_test(quote! {
            client StreamMeta {
                base "https://example.com"
            }

            POST Upload(body: Stream<OctetStream>)
                path ["upload"]
                -> Json<String>
        });
        let ir = build_facade_ir(&resolved);

        let endpoint = &ir.endpoints[0];
        assert_eq!(
            endpoint
                .required_args
                .iter()
                .map(|arg| {
                    let ty = &arg.ty;
                    (arg.name.to_string(), type_text(ty))
                })
                .collect::<Vec<_>>(),
            vec![("body".to_string(), "StreamBody".to_string())]
        );
    }

    #[test]
    fn facade_ir_uses_record_body_for_record_request_endpoints() {
        let resolved = crate::sema::analyze_tokens_for_test(quote! {
            client RecordMeta {
                base "https://example.com"
            }

            POST Upload(body: Records<LogEntry, NdJson>)
                path ["upload"]
                -> Json<String>
        });
        let ir = build_facade_ir(&resolved);

        let endpoint = &ir.endpoints[0];
        assert_eq!(
            endpoint
                .required_args
                .iter()
                .map(|arg| {
                    let ty = &arg.ty;
                    (arg.name.to_string(), type_text(ty))
                })
                .collect::<Vec<_>>(),
            vec![(
                "body".to_string(),
                ":: concord_core :: advanced :: RecordBody < LogEntry >".to_string()
            )]
        );
    }

    #[test]
    fn facade_ir_uses_multipart_body_for_multipart_request_endpoints() {
        let resolved = crate::sema::analyze_tokens_for_test(quote! {
            client MultipartMeta {
                base "https://example.com"
            }

            POST Upload(body: Multipart<RawResponsePart>)
                path ["upload"]
                -> Json<String>
        });
        let ir = build_facade_ir(&resolved);

        let endpoint = &ir.endpoints[0];
        assert_eq!(
            endpoint
                .required_args
                .iter()
                .map(|arg| {
                    let ty = &arg.ty;
                    (arg.name.to_string(), type_text(ty))
                })
                .collect::<Vec<_>>(),
            vec![(
                "body".to_string(),
                ":: concord_core :: advanced :: MultipartBody".to_string()
            )]
        );
    }

    #[test]
    fn emit_uses_stream_request_and_response_codegen() {
        let expanded = expanded(quote! {
            api! {
                client StreamCodegen {
                    base "https://example.com"
                }

                POST Upload(body: Stream<OctetStream>)
                    path ["upload"]
                    -> Stream<OctetStream>
            }
        });

        assert_contains_all(
            &expanded,
            &[
                "StreamBody",
                "BodyPlan::RawStream",
                "RequestArgs::with_stream_body",
                "StreamResponse<OctetStream>",
                "execute_plan_stream::<OctetStream>",
                "StreamResponseEndpoint",
            ],
        );
    }

    #[test]
    fn emit_uses_record_request_and_response_codegen() {
        let expanded = expanded(quote! {
            api! {
                client RecordCodegen {
                    base "https://example.com"
                }

                POST Upload(body: Records<LogEntry, NdJson>)
                    path ["upload"]
                    -> Records<LogEntry, NdJson>
            }
        });

        assert_contains_all(
            &expanded,
            &[
                "RecordBody < LogEntry >",
                "BodyPlan::Records",
                "RequestArgs::with_record_body::< LogEntry , NdJson >",
                "RecordStream < LogEntry >",
                "execute_plan_records::< LogEntry , NdJson >",
                "RecordResponseEndpoint",
            ],
        );
    }

    #[test]
    fn emit_uses_multipart_request_and_response_codegen() {
        let expanded = expanded(quote! {
            api! {
                client MultipartCodegen {
                    base "https://example.com"
                }

                POST Upload(body: Multipart<RawResponsePart>)
                    path ["upload"]
                    -> Multipart<RawResponsePart, Mixed>
            }
        });

        assert_contains_all(
            &expanded,
            &[
                "MultipartBody",
                "BodyPlan::Multipart",
                "RequestArgs::with_multipart_body::< ::concord_core::advanced::FormData >",
                "MultipartStream < RawResponsePart >",
                "execute_plan_multipart::< RawResponsePart , Mixed >",
                "MultipartResponseEndpoint",
            ],
        );
    }

    #[test]
    fn emit_uses_sse_response_codegen() {
        let expanded = expanded(quote! {
            api! {
                client SseCodegen {
                    base "https://example.com"
                }

                GET Events
                    path ["events"]
                    -> Sse<MyEvent>
            }
        });

        assert_contains_all(
            &expanded,
            &[
                "SseStream < MyEvent >",
                "execute_plan_sse::< MyEvent , ::concord_core::advanced::JsonSse >",
                "SseResponseEndpoint",
                "EventStream",
                "try_header_value",
                "Format::Text",
            ],
        );
    }

    #[test]
    fn emit_uses_explicit_sse_codec_codegen() {
        let expanded = expanded(quote! {
            api! {
                client ExplicitSseCodegen {
                    base "https://example.com"
                }

                GET Events
                    path ["events"]
                    -> Sse<MyEvent, MyCodec>
            }
        });

        assert_contains_all(
            &expanded,
            &[
                "SseStream < MyEvent >",
                "execute_plan_sse::< MyEvent , MyCodec >",
                "SseResponseEndpoint",
                "EventStream",
                "try_header_value",
                "Format::Text",
            ],
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
            .find(|scope| ident_vec_text(&scope.path) == vec!["regional".to_string()])
            .expect("regional scope metadata");
        assert_eq!(ident_text(&regional.public_method), "regional");
        assert_eq!(
            ident_text(&regional.rust_type_name),
            "ScopeMetaRegionalScope"
        );
        assert_eq!(ident_vec_text(&regional.parent_path), Vec::<String>::new());
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
            .find(|setter| setter.field == quote::format_ident!("locale"))
            .expect("scope setter metadata");
        assert_eq!(ident_text(&locale.set_name), "locale");
        assert_eq!(ident_text(&locale.clear_name), "clear_locale");
        assert!(locale.set_doc.contains("scope parameter"));
        assert_eq!(regional.methods.len(), 1);
        assert_eq!(ident_text(&regional.methods[0].public_name), "teams");
        assert_eq!(
            ident_vec_text(&regional.methods[0].target_scope_path),
            vec!["regional".to_string(), "teams".to_string()]
        );
        assert_eq!(
            ident_text(&regional.methods[0].target_scope_type_name),
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
            .find(|endpoint| ident_text(&endpoint.target.endpoint) == "Search")
            .expect("search endpoint metadata");
        let filter = endpoint
            .setters
            .iter()
            .find(|setter| setter.field == quote::format_ident!("filter"))
            .expect("filter setter metadata");
        assert_eq!(ident_text(&filter.set_name), "filter");
        assert_eq!(ident_text(&filter.set_optional_name), "filter_opt");
        assert_eq!(ident_text(&filter.clear_name), "clear_filter");
        assert!(filter.set_doc.contains("optional query parameter"));

        let count = endpoint
            .setters
            .iter()
            .find(|setter| setter.field == quote::format_ident!("count"))
            .expect("count setter metadata");
        assert_eq!(ident_text(&count.set_optional_name), "count_opt");
        assert_eq!(ident_text(&count.clear_name), "clear_count");
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
            .find(|setter| setter.field == quote::format_ident!("filter"))
            .expect("filter setter metadata");
        setter.set_name = quote::format_ident!("with_filter_from_ir");
        setter.set_optional_name = quote::format_ident!("with_filter_opt_from_ir");
        setter.clear_name = quote::format_ident!("without_filter_from_ir");
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
                "pub use pending_api :: ListRequestExt",
            ],
        );
        assert!(
            !out.contains("pub use pending_api :: PingRequestExt"),
            "empty request-extension traits should not be reexported"
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
                "let mut route = < super :: RoutePlanApiCx as :: concord_core :: prelude :: ClientContext > :: base_route (vars , __concord_auth_vars)",
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
    fn static_path_slash_behavior_characterized() {
        let out = expanded(quote! {
            client StaticPathSlashApi {
                base "https://example.com"
            }

            GET Show
                path ["a/b"]
                -> Json<String>
        });

        assert_contains_all(&out, &["route.path_mut().push_raw(\"a/b\")"]);
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
                "let (headers , query , timeout , retry , mut rate_limit) = policy.into_parts()",
                "rate_limit.canonicalize()",
                "let __resolved_policy = :: concord_core :: internal :: ResolvedPolicy",
                "auth : __auth_plan",
            ],
        );
    }

    #[test]
    fn generated_auth_plan_uses_resolved_requirements() {
        let out = expanded(quote! {
            client AuthPlanApi {
                base "https://example.com"
                secret token: String
                credential key = api_key(secret.token)
            }

            GET Search
                path ["search"]
                auth header "X-Api-Key" = key
                -> Json<String>
        });

        assert_contains_all(
            &out,
            &[
                "::concord_core::advanced::AuthRequirement",
                "::concord_core::advanced::AuthPlacement::Header",
                "::concord_core::advanced::AuthUsageId::new(\"header\")",
                "AuthProvenance::new(\"endpoint\")",
                "step_id: ::core::option::Option::Some(\"Search:0:key\")",
            ],
        );
        assert!(
            !out.contains("auth_use_credential_ident_ir"),
            "generated code should not call old auth-use helpers"
        );
        assert!(
            !out.contains("emit_auth_usage_id"),
            "generated code should not call old auth-use helpers"
        );
        assert!(
            !out.contains("endpoint_qualified_name(ep)"),
            "generated code should not reconstruct endpoint names"
        );
    }

    #[test]
    fn generated_oauth2_client_credentials_provider_is_typed() {
        let out = expanded(quote! {
            client OAuthProviderApi {
                base "https://example.com"
                secret client_id: String
                secret client_secret: String

                credential oauth = oauth2_client {
                    token_url: "https://auth.example.com/oauth/token",
                    client_id: secret.client_id,
                    client_secret: secret.client_secret,
                    scope: "read:me",
                }
            }

            GET OAuthMe
                path ["oauth-me"]
                auth bearer oauth
                -> Json<String>
        });

        assert_contains_all(
            &out,
            &[
                "::concord_core::advanced::OAuth2ClientCredentialsProvider::from_validated_token_url",
                ".scope(\"read:me\")",
                "CredentialId::new(\"OAuthProviderApi\",\"oauth\")",
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
                "content_type : < Json < LoginRequest > as :: concord_core :: advanced :: BodyCodec > :: try_content_type ()",
                "format : __body_format",
                "ResponsePlan",
                "decode : __decode_",
            ],
        );
    }

    #[test]
    fn generated_invalid_codec_headers_return_typed_errors() {
        let out = expanded(quote! {
            client CodecErrorApi {
                base "https://example.com"
            }

            POST Upload(body: Json<UploadBody>)
                path ["upload"]
                -> Json<UploadResponse>
        });

        assert_contains_all(
            &out,
            &[
                "BodyCodec>::try_content_type()",
                "ResponseCodec>::try_accept()",
                "::concord_core::prelude::ApiClientError::invalid_param",
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
                "let mut ctrl : :: concord_core :: internal :: OffsetLimitPagination = :: core :: default :: Default :: default ()",
                ":: concord_core :: advanced :: OffsetLimitBindings",
                "endpoint_state_pagination",
                "EndpointPaginationRuntimeAdapter",
                ":: concord_core :: advanced :: PagedBindings",
                "endpoint_state_pagination",
                "EndpointPaginationRuntimeAdapter",
                "ctrl . offset_key = :: std :: borrow :: Cow :: from (\"start\")",
                "ctrl . limit_key = :: std :: borrow :: Cow :: from (\"count\")",
                "let mut ctrl : :: concord_core :: internal :: CursorPagination = :: core :: default :: Default :: default ()",
                "ctrl . cursor_key = :: std :: borrow :: Cow :: from (\"cursor\")",
                "ctrl . per_page_key = :: std :: borrow :: Cow :: from (\"count\")",
                "let mut ctrl : :: concord_core :: internal :: PagedPagination = :: core :: default :: Default :: default ()",
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
    fn rustdoc_behavior_label_dedup_does_not_affect_policy() {
        let out = expanded(quote! {
            client LabelDedup {
                base "https://example.com"
                secret token: String
                credential read_auth = bearer(secret.token)

                retry read {
                    max_attempts 2
                    methods [GET]
                }

                rate_limit read_limit {
                    bucket read by [host] {
                        1 / 1s
                    }
                }

                behaviors {
                    behavior read {
                        auth bearer read_auth
                        retry read
                        rate_limit read_limit
                    }
                }

                defaults {
                    behavior read
                }
            }

            scope users {
                path ["users"]
                behavior read

                GET Me
                    path ["me"]
                    behavior read
                    -> Json<()>
            }
        });

        assert_contains_all(&out, &["#[doc=\"Behavior: `read`\"]", "policy.set_retry"]);
        assert_contains_all(&out, &["policy.add_rate_limit"]);
        let behavior_doc_lines = generated_doc_attrs(&out)
            .into_iter()
            .filter(|doc| doc.contains("Behavior:`"))
            .collect::<Vec<_>>();
        assert_eq!(behavior_doc_lines.len(), 1);
        for idx in 0..3 {
            assert!(
                out.contains(&format!("users::Me:{idx}:read_auth")),
                "missing repeated auth step {idx}\n{out}"
            );
        }
        assert_eq!(
            out.match_indices("users::Me:0:read_auth")
                .chain(out.match_indices("users::Me:1:read_auth"))
                .chain(out.match_indices("users::Me:2:read_auth"))
                .count(),
            3
        );
        assert_eq!(
            out.match_indices("RateLimitBucketUse::new(\"read\",\"read_limit_0\"")
                .count(),
            3
        );
    }

    #[test]
    fn behavior_profiles_do_not_reach_runtime_codegen() {
        let alpha = expanded(quote! {
            client BehaviorCodegen {
                base "https://example.com"
                secret token: String
                credential session = bearer(secret.token)

                retry read {
                    max_attempts 2
                    methods [GET]
                    on [401, 403]
                    retry_after
                }

                rate_limit app {
                    bucket application by [host] {
                        1 / 1s
                    }
                }

                behaviors {
                        behavior alpha {
                            auth bearer session
                            retry read
                            rate_limit app
                        }
                }

                defaults {
                    behavior alpha
                }
            }

            GET Ping
                path ["ping"]
                behavior alpha
                -> Json<()>
        });
        let beta = expanded(quote! {
            client BehaviorCodegen {
                base "https://example.com"
                secret token: String
                credential session = bearer(secret.token)

                retry read {
                    max_attempts 2
                    methods [GET]
                    on [401, 403]
                    retry_after
                }

                rate_limit app {
                    bucket application by [host] {
                        1 / 1s
                    }
                }

                behaviors {
                        behavior beta {
                            auth bearer session
                            retry read
                            rate_limit app
                        }
                }

                defaults {
                    behavior beta
                }
            }

            GET Ping
                path ["ping"]
                behavior beta
                -> Json<()>
        });

        assert_contains_all(
            &alpha,
            &[
                "#[doc=\"Behavior: `alpha`\"]",
                "policy.set_retry",
                "policy.add_rate_limit",
            ],
        );
        assert_contains_all(
            &beta,
            &[
                "#[doc=\"Behavior: `beta`\"]",
                "policy.set_retry",
                "policy.add_rate_limit",
            ],
        );
        assert_eq!(without_doc_attrs(&alpha), without_doc_attrs(&beta));
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
                "#[doc=\"Required params: `id`\"]",
                "#[doc=\"Query params: `count`, `filter`\"]",
                "#[doc=\"Headers: `X-Tenant`\"]",
                "#[doc=\"Auth:\"]",
                "#[doc=\"- header `X-Api-Key` = `key`\"]",
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

                behaviors {
                    behavior protected_read {
                        auth bearer session
                    }
                }
            }

            GET GetBearerDoc
                path ["bearer"]
                behavior protected_read
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
                "#[doc=\"Behavior: `protected_read`\"]",
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
                "pub async fn set (& self , value : AccessToken ,) -> :: core :: result :: Result < () , :: concord_core :: advanced :: AuthError >",
                "pub async fn clear (& self) -> :: core :: result :: Result < () , :: concord_core :: advanced :: AuthError >",
                "pub async fn is_set (& self) -> :: core :: result :: Result < bool , :: concord_core :: advanced :: AuthError >",
                "pub async fn acquire_auth_session",
                "pub async fn set_auth_session_value (& self , value : AccessToken ,) -> :: core :: result :: Result < () , :: concord_core :: advanced :: AuthError >",
                "pub async fn clear_auth_session (& self) -> :: core :: result :: Result < () , :: concord_core :: advanced :: AuthError >",
                "pub async fn has_auth_session (& self) -> :: core :: result :: Result < bool , :: concord_core :: advanced :: AuthError >",
                "pub trait SnapshotAuthAcquireAsSessionExt",
                "fn acquire_as_session (self,) -> :: core :: pin :: Pin",
                ". with_missing_hint (\"client.acquire_auth_session(...)\")",
                ":: concord_core :: advanced :: AuthPlacement :: Bearer",
                ":: concord_core :: advanced :: AuthPlacement :: Header (\"X-Upstream-Key\")",
            ],
        );
    }

    #[test]
    fn generated_endpoint_backed_auth_helpers_use_structured_endpoint_target() {
        let out = expanded(quote! {
            client EndpointAuthTarget {
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
        });

        assert_contains_all(
            &out,
            &[
                "pub async fn acquire_auth_session",
                "pub trait EndpointAuthTargetAcquireAsSessionExt",
                "fn acquire_as_session (self,) -> :: core :: pin :: Pin",
                "endpoints :: auth_api :: LoginForSession",
                ". with_missing_hint (\"client.acquire_auth_session(...)\")",
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
    fn generated_pagination_endpoint_state_bindings_for_offset_limit() {
        let out = expanded(quote! {
            client SnapshotPaginationBindings {
                base "https://example.com"
            }

            GET List(start: u64 = 0, count: u64 = 20)
                query {
                    "from" = start
                    "pageSize" = count
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
                "EndpointField :: new",
                ":: concord_core :: advanced :: OffsetLimitBindings",
                "endpoint_state_pagination",
                "EndpointPaginationRuntimeAdapter",
                "ep . start . clone ()",
                "ep . count . clone ()",
                ":: concord_core :: internal :: PaginationPlan :: from (ctrl)",
                "ctrl . offset_key = :: std :: borrow :: Cow :: from (\"from\")",
                "ctrl . limit_key = :: std :: borrow :: Cow :: from (\"pageSize\")",
            ],
        );
    }

    #[test]
    fn generated_offset_limit_uses_endpoint_state_pagination_runtime() {
        let out = expanded(quote! {
            client SnapshotOffsetLimitRuntime {
                base "https://example.com"
            }

            GET List(start: u64 = 0, count: u64 = 20)
                headers {
                    "X-Start" = start,
                    "X-Count" = count,
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
                ":: concord_core :: advanced :: OffsetLimitBindings",
                "endpoint_state_pagination",
                "EndpointPaginationRuntimeAdapter",
                "EndpointField :: new",
                "ep . start . clone ()",
                "ep . count . clone ()",
                ":: concord_core :: internal :: PaginationPlan :: from (ctrl)",
            ],
        );
        assert!(
            !out.contains("offset_key ="),
            "endpoint-state binding helper must not use offset query keys"
        );
        assert!(
            !out.contains("limit_key ="),
            "endpoint-state binding helper must not use limit query keys"
        );
        assert!(
            !out.contains("offset_limit_pagination_bindings"),
            "obsolete per-controller hook must not appear in generated output"
        );
        assert!(
            !out.contains("paged_pagination_bindings"),
            "obsolete per-controller hook must not appear in generated output"
        );
    }

    #[test]
    fn generated_paged_uses_endpoint_state_pagination_runtime() {
        let out = expanded(quote! {
            client SnapshotPagedRuntime {
                base "https://example.com"
            }

            GET List(page: u64 = 1, count: u64 = 2)
                headers {
                    "X-Page" = page,
                    "X-Count" = count,
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
                ":: concord_core :: advanced :: PagedBindings",
                "endpoint_state_pagination",
                "EndpointPaginationRuntimeAdapter",
                "EndpointField :: new",
                "ep . page . clone ()",
                "ep . count . clone ()",
                ":: concord_core :: internal :: PaginationPlan :: from (ctrl)",
            ],
        );
        assert!(
            !out.contains("offset_limit_pagination_bindings"),
            "obsolete per-controller hook must not appear in generated output"
        );
        assert!(
            !out.contains("paged_pagination_bindings"),
            "obsolete per-controller hook must not appear in generated output"
        );
    }

    #[test]
    fn generated_custom_pagination_remains_on_old_fallback_path() {
        let out = expanded(quote! {
            client SnapshotCustomPagination {
                base "https://example.com"
            }

            GET List
                as list
                path ["items"]
                paginate HeaderCursorPagination
                -> Json<Vec<String>>
        });

        assert_contains_all(
            &out,
            &[
                ":: concord_core :: internal :: PaginationPlan :: custom :: < HeaderCursorPagination , Vec < String > > ()",
            ],
        );
        assert!(
            !out.contains("endpoint_state_pagination"),
            "custom pagination must remain on the old fallback path"
        );
        assert!(
            !out.contains("EndpointPaginationRuntimeAdapter"),
            "custom pagination must not require endpoint-state runtime"
        );
    }

    #[test]
    fn generated_cursor_uses_endpoint_state_pagination_runtime() {
        let out = expanded(quote! {
            client SnapshotCursorRuntime {
                base "https://example.com"
            }

            GET List(cursor?: String, count: u64 = 2)
                headers {
                    "X-Cursor" = cursor,
                    "X-Count" = count,
                }
                paginate CursorPagination {
                    cursor = cursor,
                    per_page = count
                }
                -> Json<Vec<String>>
        });

        assert_contains_all(
            &out,
            &[
                ":: concord_core :: advanced :: CursorBindings",
                "endpoint_state_pagination",
                "EndpointPaginationRuntimeAdapter",
                "EndpointField :: new",
                "HasNextCursor",
                "ep . cursor . clone ()",
                "ep . count . clone ()",
                "ep . cursor = value",
                ":: concord_core :: advanced :: CursorPagination {",
            ],
        );
        assert!(
            !out.contains("offset_limit_pagination_bindings"),
            "obsolete per-controller hook must not appear in generated output"
        );
        assert!(
            !out.contains("paged_pagination_bindings"),
            "obsolete per-controller hook must not appear in generated output"
        );
    }

    #[test]
    fn generated_cursor_endpoint_state_preserves_cursor_controller_flags() {
        let out = expanded(quote! {
            client SnapshotCursorFlags {
                base "https://example.com"
            }

            GET List(cursor?: String, count: u64 = 2)
                headers {
                    "X-Cursor" = cursor,
                    "X-Count" = count,
                }
                paginate CursorPagination {
                    cursor = cursor,
                    per_page = count,
                    send_cursor_on_first = true,
                    stop_when_cursor_missing = false
                }
                -> Json<Vec<String>>
        });

        assert_contains_all(
            &out,
            &[
                "endpoint_state_pagination",
                "EndpointPaginationRuntimeAdapter",
                "CursorBindings",
                "EndpointField :: new",
                "send_cursor_on_first:true",
                "stop_when_cursor_missing:false",
                "CursorPagination{",
            ],
        );
    }

    #[test]
    fn generated_pagination_endpoint_state_bindings_clone_non_copy_cursor() {
        let out = expanded(quote! {
            client SnapshotPaginationCursorBindings {
                base "https://example.com"
            }

            GET List(cursor?: String, count: u64 = 20)
                query {
                    cursor
                    count
                }
                paginate CursorPagination {
                    cursor = cursor,
                    per_page = count
                }
                -> Json<Vec<String>>
        });

        assert_contains_all(
            &out,
            &[
                ":: concord_core :: advanced :: CursorBindings",
                "EndpointField :: new",
                "ep . cursor . clone ()",
                "ep . cursor = value",
                ":: concord_core :: advanced :: CursorPagination {",
                "HasNextCursor",
            ],
        );
    }

    #[test]
    fn generated_pagination_public_surface_exposes_collect_not_for_each_page() {
        let out = expanded(quote! {
            client SnapshotPaginationSurface {
                base "https://example.com"
            }

            GET List(start: u64 = 0, count: u64 = 20)
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

        assert_contains_all(&out, &["#[doc=\"Pagination: OffsetLimitPagination\"]"]);
        assert!(
            !out.contains("for_each_page"),
            "generated public pagination surface should not expose for_each_page"
        );
    }

    #[test]
    fn generated_request_surface_has_no_behaviorless_extension_traits() {
        let out = expanded(quote! {
            client RequestSurfaceApi {
                base "https://example.com"
            }

            GET Ping
                as ping
                path ["ping"]
                -> Json<String>
        });

        assert_contains_all(
            &out,
            &["pub fn ping (& self ,) -> :: concord_core :: prelude :: PendingRequest"],
        );
        assert!(
            !out.contains("pub trait PingRequestExt"),
            "endpoints without request setters must not generate empty request-extension traits"
        );
        assert!(
            !out.contains("pub use pending_api :: PingRequestExt"),
            "endpoints without request setters must not reexport empty request-extension traits"
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
                "ApiClientError :: rate_limit",
                "RateLimitErrorKind :: InvalidConfiguration",
            ],
        );
        assert!(!out.contains("compile_error!(concat!(\"unresolvedrate_limitkey"));
        assert!(!out.contains("endpoint/scoperate_limitkeycannotbeusedinclientbasepolicy"));
    }

    #[test]
    fn generated_retry_and_rate_limit_snapshot_contains_resolved_policy() {
        let out = expanded(quote! {
            client SnapshotPolicy {
                base "https://example.com"

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

                default {
                    retry read
                    rate_limit app
                }
            }

            GET Ping
                as ping
                path ["ping"]
                -> Json<String>;
        });

        assert_contains_all(
            &out,
            &[
                "::http::StatusCode::from_u16(401u16)",
                "::http::StatusCode::from_u16(403u16)",
                "RateLimitWindow::new(::std::num::NonZeroU32::new(10u32).ok_or_else",
                "RateLimitBucketUse::new(\"application\",\"app_0\"",
                "policy.set_retry(::concord_core::advanced::RetryConfig",
                "policy.add_rate_limit(::concord_core::advanced::RateLimitPlan::from_buckets",
                "ApiClientError :: rate_limit",
            ],
        );
    }

    #[test]
    fn codegen_snapshot_uses_resolved_ir() {
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
            other => panic!(
                "expected resolved client retry from behavior/default lowering, got {other:?}"
            ),
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

        let endpoint = resolved
            .endpoints
            .iter()
            .find(|ep| ep.name == "Ping")
            .expect("resolved ping endpoint");
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
