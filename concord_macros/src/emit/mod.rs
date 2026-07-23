use std::collections::{HashMap, HashSet};

use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{PathArguments, Type};

use crate::semantic::model::{
    Api, Child, Client, CloneEligibility, Credential, CredentialSource, Endpoint, NormalizedConfig,
    PaginationInitializer, PaginationValue, RawResponseCapability, RequestBody, RequiredArgument,
    ResolvedExpr, Scope, Value, ValueReference, ValueShape,
};
use crate::semantic::surface::{canonical_ident, public_ident, to_snake_case};
use crate::syntax::ast::{
    AuthChallenge, AuthPlacementKind, DefaultExpr, MethodKind, RetrySelection, ValueKind,
};

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
    use crate::{semantic, syntax};

    pub(super) fn api() -> Api {
        let source = syntax::parse(quote! {
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
        .expect("artifact syntax");
        semantic::resolve(source).expect("artifact model")
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
    use quote::quote;

    use super::*;
    use crate::{semantic as resolve, syntax as parse};

    #[test]
    fn complete_generation_consumes_normalized_singular_client_ir() {
        let source = parse::parse(quote! {
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
        })
        .expect("AST");
        let api = resolve::resolve(source).expect("IR");
        let output = generate(&api, &quote!(::concord_core)).to_string();
        assert!(!output.contains("FirstApiRegions"));
        assert!(output.contains("RootRequest"));
        assert!(output.contains("resources"));
        assert!(output.contains("map_endpoint"));
        assert!(!output.contains("RequestExt"));
        assert!(!output.contains("pub mod facades"));
        assert!(!output.contains("pub mod endpoints"));
    }
}
