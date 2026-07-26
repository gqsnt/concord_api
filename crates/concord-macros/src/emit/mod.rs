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
    ResolvedRetry, ResolvedValueKind, Scope, Value, ValueReference, ValueShape,
};
use crate::semantic::surface::{canonical_ident, public_ident, to_snake_case};

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
    use crate::semantic::surface::SurfaceCapabilities;

    pub(super) fn api() -> Api {
        api_with_capabilities(SurfaceCapabilities::default())
    }

    pub(super) fn api_with_capabilities(capabilities: SurfaceCapabilities) -> Api {
        semantic::test_support::resolve_tokens_with_capabilities(
            quote! {
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
            },
            capabilities,
        )
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
                retry reqwest_default;
            }
            METHOD("PROPFIND") inspect() -> NoContent {
                auth bearer bearer challenge unauthorized_or_forbidden;
            }
        });
        let endpoint = test_support::endpoint(&api);

        assert_eq!(api.client.retry, ResolvedRetry::ReqwestDefault);
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
        for expected in [
            "retry_reqwest_default",
            "PROPFIND",
            "UnauthorizedOrForbidden",
            "\"eu\"",
            "true",
        ] {
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
            "RetrySelection",
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
