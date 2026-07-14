use super::super::*;
use super::helpers::*;
use crate::model::facade::{FacadeArgKind, FacadeConstructorArg, SetterForm};
use quote::quote;

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
fn facade_ir_uses_multipart_body_for_multipart_request_endpoints() {
    let resolved = crate::sema::analyze_tokens_for_test(quote! {
        client MultipartMeta {
            base "https://example.com"
        }

        POST Upload(body: Multipart<()>)
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
fn facade_ir_contains_defaulted_scope_setter_metadata_without_rendering_default_values() {
    let resolved = crate::sema::analyze_tokens_for_test(quote! {
        client ScopeDefaultDocs {
            base "https://example.com"
        }

        scope localized(locale: String = { let _ = "LEAK_SENTINEL_DEFAULT"; "en_US".to_string() }) {
            path ["data", locale]

            GET List
                as list
                path ["list.json"]
                -> Json<Vec<String>>
        }
    });
    let ir = build_facade_ir(&resolved);
    let localized = ir
        .scopes
        .iter()
        .find(|scope| ident_vec_text(&scope.path) == vec!["localized".to_string()])
        .expect("localized scope metadata");
    let locale = localized
        .setters
        .iter()
        .find(|setter| setter.field == quote::format_ident!("locale"))
        .expect("locale scope setter metadata");
    assert_eq!(locale.set_doc, "Set defaulted scope parameter `locale`.");
    assert_eq!(
        locale.set_optional_doc,
        "Set defaulted scope parameter `locale` from an Option; None resets to the declared default."
    );
    assert_eq!(
        locale.clear_doc,
        "Reset defaulted scope parameter `locale` to its declared default."
    );
    assert!(!locale.set_doc.contains("LEAK_SENTINEL_DEFAULT"));
    assert!(!locale.set_optional_doc.contains("LEAK_SENTINEL_DEFAULT"));
    assert!(!locale.clear_doc.contains("LEAK_SENTINEL_DEFAULT"));
    assert!(!locale.set_doc.contains("en_US"));
}

#[test]
fn facade_ir_contains_endpoint_setter_metadata() {
    let resolved = crate::sema::analyze_tokens_for_test(quote! {
        client SetterMeta {
            base "https://example.com"
        }

        GET Search(filter?: String, count: u64 = { let _ = "LEAK_SENTINEL_DEFAULT"; 20 })
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
    assert_eq!(count.set_doc, "Set defaulted query parameter `count`.");
    assert_eq!(
        count.set_optional_doc,
        "Set defaulted query parameter `count` from an Option; None resets to the declared default."
    );
    assert_eq!(
        count.clear_doc,
        "Reset defaulted query parameter `count` to its declared default."
    );
    assert!(!count.set_doc.contains("20"));
    assert!(!count.set_optional_doc.contains("20"));
    assert!(!count.clear_doc.contains("20"));
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

        scope localized(locale: String = { let _ = "LEAK_SENTINEL_DEFAULT"; "en_US".to_string() }) {
            path ["data", locale]

            GET List
                as list
                path ["list.json"]
                -> Json<Vec<String>>
        }
    })
    .to_string();

    assert!(
        out.contains("__ep . locale = self . locale") || out.contains("__ep.locale=self.locale"),
        "captured defaulted scope params must be assigned internally"
    );
    assert!(
        !out.contains("__ep = __ep . locale") && !out.contains("__ep=__ep.locale"),
        "captured defaulted scope params must not call public endpoint setters"
    );
    assert_generated_doc_attrs_do_not_contain(&out, "LEAK_SENTINEL_DEFAULT");
    assert_generated_doc_attrs_do_not_contain(&out, "`en_US`");
}

#[test]
fn generated_client_construction_contains_current_api_only() {
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
            "pub struct ConstructApi",
            "pub fn new ( tenant : String , api_key : String ) -> Self",
            "pub fn new_with_safe_reqwest_builder ( tenant : String , api_key : String , configure : impl FnOnce (:: concord_core :: advanced :: SafeReqwestBuilder) -> :: concord_core :: advanced :: SafeReqwestBuilder , ) -> :: core :: result :: Result < Self , :: concord_core :: advanced :: ReqwestClientBuildError >",
            "pub fn new_with_safe_reqwest_builder_fallible ( tenant : String , api_key : String , configure : impl FnOnce (:: concord_core :: advanced :: SafeReqwestBuilder,) -> :: core :: result :: Result < :: concord_core :: advanced :: SafeReqwestBuilder , :: concord_core :: advanced :: ReqwestClientBuildError , > , ) -> :: core :: result :: Result < Self , :: concord_core :: advanced :: ReqwestClientBuildError >",
            "pub fn builder () -> ConstructApiBuilder",
            "pub struct ConstructApiBuilder",
            "tenant : :: core :: option :: Option < String >",
            "api_key : :: core :: option :: Option < String >",
            "pub fn build ( self ) -> :: core :: result :: Result < ConstructApi , :: concord_core :: prelude :: ApiClientError >",
            "ApiClientError :: invalid_param (__ctx . clone () , \"builder.tenant\")",
            "ApiClientError :: invalid_param (__ctx . clone () , \"builder.api_key\")",
            "pub fn configure ( mut self , f : impl FnOnce (& mut :: concord_core :: advanced :: RuntimeConfig)) -> Self",
            "pub fn configure_mut (& mut self , f : impl FnOnce (& mut :: concord_core :: advanced :: RuntimeConfig)) -> & mut Self",
            "pub fn api_headers (& self) -> & :: http :: HeaderMap",
            "pub fn set_api_headers (& mut self , headers : :: http :: HeaderMap) -> :: core :: result :: Result < () , :: concord_core :: prelude :: HeaderOwnershipError >",
            "pub fn with_api_headers ( mut self , headers : :: http :: HeaderMap) -> :: core :: result :: Result < Self , :: concord_core :: prelude :: HeaderOwnershipError >",
            "pub static API_DESCRIPTOR : :: concord_core :: __private :: GeneratedApiDescriptor",
            ":: concord_core :: __private :: GeneratedPreparedCall",
            ":: concord_core :: __private :: prepare_generated_endpoint",
        ],
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
fn generated_safe_reqwest_facade_never_leaks_raw_reqwest_types() {
    let out = expanded(quote! {
        client SafeFacadeApi { base "https://example.com" }
        GET Ping path ["ping"] -> Json<String>
    });
    assert!(out.contains("SafeReqwestBuilder"));
    for forbidden in [
        "reqwest :: ClientBuilder",
        "reqwest :: Client",
        "reqwest :: Proxy",
    ] {
        assert!(
            !out.contains(forbidden),
            "generated API leaked {forbidden}: {out}"
        );
    }
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
            "#[doc = \"Set defaulted query parameter `count` from an Option; None resets to the declared default.\"]",
        ],
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
