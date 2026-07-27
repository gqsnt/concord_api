use std::collections::{HashMap, HashSet};

use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{PathArguments, Type};

use crate::semantic::model::{
    Api, Child, Client, CloneEligibility, Credential, CredentialSource, Endpoint, EndpointDocHost,
    EndpointDocInput, EndpointDocInputKind, EndpointDocModel, EndpointDocNamespace,
    EndpointDocRouteAtom, EndpointDocSetter, EndpointDocTimeout, NormalizedConfig,
    NormalizedRateLimitDimension, PaginationInitializer, PaginationValue, RequiredArgument,
    ResolvedAuthPlacement, ResolvedChallengePolicy, ResolvedDefault, ResolvedExpr, ResolvedMethod,
    ResolvedValueKind, Scope, Value, ValueReference, ValueShape,
};
use crate::semantic::surface::{canonical_ident, public_ident, to_snake_case, value_setter_ident};

mod assertions;
mod bridge;
mod client;
mod credentials;
mod descriptions;
mod docs;
mod endpoints;
mod helpers;
mod requests;
mod scopes;
mod shaping;

use assertions::*;
use bridge::*;
use client::*;
use credentials::*;
use descriptions::*;
use docs::*;
use endpoints::*;
use helpers::*;
use requests::*;
use scopes::*;
use shaping::*;

pub(crate) fn generate(api: &Api, core_path: &TokenStream) -> TokenStream {
    generate_client(&api.client, core_path)
}

#[cfg(test)]
mod test_support {
    use quote::quote;

    use super::*;
    use crate::semantic;

    pub(super) fn api() -> Api {
        semantic::test_support::resolve_tokens(quote! {
            client ArtifactApi {
                base "https://example.test";
                vars { tenant: String; }
                credentials {
                    secret bearer_value: String;
                    credential bearer = bearer(secret.bearer_value);
                }
            }
            GET fetch_item(id: String, verbose: Option<bool>) -> Bytes {
                path [vars.tenant, ep.id];
                auth bearer bearer;
            }
            scope resources(region: String) {
                GET list() -> NoContent;
            }
        })
    }

    pub(super) fn endpoint(api: &Api) -> &Endpoint {
        api.client
            .children
            .iter()
            .find_map(|child| match child {
                Child::Endpoint(endpoint) => Some(endpoint),
                Child::Scope(_) => None,
            })
            .expect("root endpoint")
    }

    pub(super) fn scope(api: &Api) -> &Scope {
        api.client
            .children
            .iter()
            .find_map(|child| match child {
                Child::Scope(scope) => Some(scope.as_ref()),
                Child::Endpoint(_) => None,
            })
            .expect("root scope")
    }

    pub(super) fn core_path() -> TokenStream {
        quote!(::concord)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use quote::quote;

    use super::*;
    use crate::semantic;

    #[test]
    fn complete_generation_consumes_normalized_singular_client_ir() {
        let api = semantic::test_support::resolve_tokens(quote! {
            client FirstApi {
                base "https://example.test";
                vars { tenant: String; }
            }
            GET root(id: String, verbose: Option<bool>) -> Bytes {
                path [vars.tenant, ep.id];
            }
            scope resources(region: String) {
                GET list() -> NoContent;
            }
        });
        let output = generate(&api, &quote!(::concord)).to_string();
        assert!(!output.contains("FirstApiRegions"));
        assert!(output.contains("RootRequest"));
        assert!(output.contains("resources"));
        assert!(output.contains("map_endpoint"));
        assert!(!output.contains("RequestExt"));
        assert!(!output.contains("pub mod facades"));
        assert!(!output.contains("pub mod endpoints"));
    }

    #[test]
    fn generated_value_mutations_use_the_exact_set_clear_reset_matrix() {
        let api = semantic::test_support::resolve_tokens(quote! {
            client ControlApi {
                base "https://example.test";
                vars {
                    required_client_value: String;
                    optional_client_value: Option<String>;
                    defaulted_client_value: u32 = 10;
                    defaulted_optional_client_value: Option<String> = None;
                    build: String;
                    timeout: Option<u64>;
                    response: bool = true;
                    debug: String;
                }
            }
            scope group(
                required_scope_value: String,
                optional_scope_value: Option<String>,
                defaulted_scope_value: u32 = 20,
                defaulted_optional_scope_value: Option<String> = None,
            ) {
                GET item(
                    required_endpoint_value: String,
                    optional_endpoint_value: Option<String>,
                    defaulted_endpoint_value: u32 = 30,
                    defaulted_optional_endpoint_value: Option<String> = None,
                    defaulted_optional_vector_endpoint_value: Option<Vec<String>> = None,
                ) -> Bytes;
            }
        });
        let output = generate(&api, &quote!(::concord)).to_string();

        for method in [
            "set_required_client_value",
            "set_optional_client_value",
            "clear_optional_client_value",
            "set_defaulted_client_value",
            "reset_defaulted_client_value",
            "set_defaulted_optional_client_value",
            "clear_defaulted_optional_client_value",
            "reset_defaulted_optional_client_value",
            "set_build",
            "set_timeout",
            "clear_timeout",
            "set_response",
            "reset_response",
            "set_debug",
            "set_optional_scope_value",
            "clear_optional_scope_value",
            "set_defaulted_scope_value",
            "reset_defaulted_scope_value",
            "set_defaulted_optional_scope_value",
            "clear_defaulted_optional_scope_value",
            "reset_defaulted_optional_scope_value",
            "set_optional_endpoint_value",
            "clear_optional_endpoint_value",
            "set_defaulted_endpoint_value",
            "reset_defaulted_endpoint_value",
            "set_defaulted_optional_endpoint_value",
            "clear_defaulted_optional_endpoint_value",
            "reset_defaulted_optional_endpoint_value",
            "set_defaulted_optional_vector_endpoint_value",
            "clear_defaulted_optional_vector_endpoint_value",
            "reset_defaulted_optional_vector_endpoint_value",
        ] {
            assert!(output.contains(&format!("pub fn {method}")), "{method}");
        }
        for absent in [
            "set_required_scope_value",
            "set_required_endpoint_value",
            "clear_required_client_value",
            "reset_required_client_value",
            "reset_optional_client_value",
            "clear_defaulted_client_value",
            "clear_required_scope_value",
            "reset_required_scope_value",
            "reset_optional_scope_value",
            "clear_defaulted_scope_value",
            "clear_required_endpoint_value",
            "reset_required_endpoint_value",
            "reset_optional_endpoint_value",
            "clear_defaulted_endpoint_value",
            "required_client_value",
            "optional_client_value",
            "defaulted_client_value",
            "optional_scope_value",
            "defaulted_scope_value",
            "optional_endpoint_value",
            "defaulted_endpoint_value",
        ] {
            assert!(
                !output.contains(&format!("pub fn {absent}")),
                "unexpected generated method {absent}"
            );
        }
        assert!(output.contains("pub fn group"));
        assert!(output.contains("pub fn item"));
        assert!(
            output
                .contains("pub fn set_defaulted_optional_client_value (mut self , value : String)")
        );
        assert!(
            output
                .contains("pub fn set_defaulted_optional_scope_value (mut self , value : String)")
        );
        assert!(
            output.contains(
                "pub fn set_defaulted_optional_endpoint_value (mut self , value : String)"
            )
        );
        assert!(output.contains(
            "pub fn set_defaulted_optional_vector_endpoint_value (mut self , value : Vec < String >)"
        ));

        let endpoint = semantic::test_support::endpoint(&api.client.children, "item");
        let request = generate_request_wrapper(
            &api.client,
            endpoint,
            RequestBridge {
                support_path: &quote!(super),
                generated_core: &quote!(::concord),
            },
            &quote!(::concord),
            &quote!(),
        )
        .to_string();
        assert!(request.contains("endpoint . set_optional_endpoint_value (value)"));
        assert!(request.contains("endpoint . reset_defaulted_endpoint_value ()"));
        assert!(request.contains("endpoint . set_defaulted_optional_endpoint_value (value)"));
        assert!(request.contains("endpoint . clear_defaulted_optional_endpoint_value ()"));
        assert!(request.contains("endpoint . reset_defaulted_optional_endpoint_value ()"));
        assert!(
            request.contains("endpoint . set_defaulted_optional_vector_endpoint_value (value)")
        );
        assert!(request.contains("endpoint . clear_defaulted_optional_vector_endpoint_value ()"));
        assert!(request.contains("endpoint . reset_defaulted_optional_vector_endpoint_value ()"));
        assert!(!request.contains("set_optional_scope_value"));
        assert!(!request.contains("set_defaulted_scope_value"));
    }

    #[test]
    fn normalized_semantic_enums_preserve_emitted_contract_behavior() {
        let api = semantic::test_support::resolve_tokens(quote! {
            client NormalizedApi {
                base "https://example.test";
                vars {
                    region: String = "eu";
                    enabled: bool = true;
                }
                credentials {
                    secret token: String;
                    credential bearer = bearer(secret.token);
                }
            }
            METHOD("PROPFIND") inspect() -> NoContent {
                auth bearer bearer challenge unauthorized_or_forbidden;
            }
        });
        let endpoint = test_support::endpoint(&api);

        assert!(matches!(
            api.client.values[0].kind,
            ResolvedValueKind::Defaulted(ResolvedDefault::String(_))
        ));
        assert!(matches!(
            api.client.values[1].kind,
            ResolvedValueKind::Defaulted(ResolvedDefault::Boolean(_))
        ));
        assert!(
            matches!(&endpoint.method, ResolvedMethod::Custom(value) if value.value() == "PROPFIND")
        );
        assert!(matches!(
            endpoint.config.auth[0].kind,
            ResolvedAuthPlacement::Bearer
        ));
        assert_eq!(
            endpoint.config.auth[0].challenge,
            ResolvedChallengePolicy::UnauthorizedOrForbidden
        );

        let output = generate(&api, &quote!(::concord)).to_string();
        for expected in ["PROPFIND", "UnauthorizedOrForbidden", "\"eu\"", "true"] {
            assert!(
                output.contains(expected),
                "normalized emission omitted `{expected}`"
            );
        }
    }

    #[test]
    fn production_emit_is_grammar_independent_and_uses_facade_feature_terms() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let emit_dir = manifest.join("src/emit");
        let syntax_path = ["crate", "::", "syntax"].concat();
        let implementation_feature_names = [
            ["concord", "_core/"].concat(),
            ["concord", "-core/"].concat(),
        ];

        for entry in fs::read_dir(&emit_dir).expect("emit source directory") {
            let path = entry.expect("emit source entry").path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                continue;
            }
            let source = fs::read_to_string(&path).expect("emit source");
            assert!(
                !source.contains(&syntax_path),
                "{} imports source grammar",
                path.display()
            );
            for forbidden in &implementation_feature_names {
                assert!(
                    !source.contains(forbidden.as_str()),
                    "{} names implementation-crate feature `{forbidden}`",
                    path.display()
                );
            }
        }

        let model =
            fs::read_to_string(manifest.join("src/semantic/model.rs")).expect("semantic model");
        assert!(
            !model.contains(&syntax_path),
            "semantic model imports source grammar"
        );
        let model_tokens = model
            .split(|character: char| !character.is_alphanumeric() && character != '_')
            .collect::<HashSet<_>>();
        for source_enum in [
            "AuthChallenge",
            "AuthPlacementKind",
            "DefaultExpr",
            "MethodKind",
            "ValueKind",
        ] {
            assert!(
                !model_tokens.contains(source_enum),
                "semantic model exposes source enum `{source_enum}`"
            );
        }

        let output = generate(&test_support::api(), &quote!(::concord)).to_string();
        for forbidden in &implementation_feature_names {
            assert!(
                !output.contains(forbidden.as_str()),
                "generated output names implementation-crate feature `{forbidden}`"
            );
        }
    }
}
