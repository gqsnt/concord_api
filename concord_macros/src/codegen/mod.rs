//! Code generation for resolved Concord APIs.
//!
//! This layer receives `ResolvedApi` and emits client wrappers, facade methods,
//! auth state, endpoint structs, and endpoint `plan()` implementations. It must
//! not inspect raw parser structs or raw scope stacks.

use crate::emit_helpers;
use crate::model::facade::{
    FacadeConstructorArg, FacadeCredentialMethods, FacadeDoc, FacadeEndpoint, FacadeEndpointTarget,
    FacadeIr, FacadeMethod, FacadeScope, FacadeSetter, client_prefixed_type_name,
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

#[allow(dead_code)]
pub fn emit(resolved_api: ResolvedApi) -> TokenStream2 {
    let facade_ir = crate::model::facade::build_facade_ir(&resolved_api);
    emit_with_facade(resolved_api, &facade_ir)
}

pub(crate) fn emit_with_facade(resolved_api: ResolvedApi, facade_ir: &FacadeIr) -> TokenStream2 {
    emit_resolved(resolved_api, facade_ir)
}

#[cfg(test)]
pub(crate) use crate::model::facade::build_facade_ir;

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
    let api_descriptor = emit_api_descriptor(&resolved_api);
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

            const _: ::concord_core::__private::v1::MacroAbi<1> =
                ::concord_core::__private::v1::MACRO_ABI;

            #vars_struct
            #auth_vars_struct
            #auth_state_struct
            #cx_struct

            #client_wrapper

            #endpoints_mod
            #api_descriptor
            #internal_mod
        }

        #( #acquire_trait_imports )*
        #( #pending_request_trait_imports )*
    }
}

fn emit_api_descriptor(api: &ResolvedApi) -> TokenStream2 {
    let api_name = LitStr::new(&api.client_name.to_string(), api.client_name.span());
    let origin = emit_origin_descriptor(&api.descriptor.origin);
    let endpoint_refs = api.endpoints.iter().map(|endpoint| {
        let descriptor = endpoint_descriptor_ident(endpoint);
        quote! { &__endpoints::#descriptor }
    });
    quote! {
        #[doc(hidden)]
        pub static API_DESCRIPTOR: ::concord_core::__private::v1::ApiDescriptor =
            ::concord_core::__private::v1::ApiDescriptor {
                name: #api_name,
                origin: #origin,
                endpoints: &[ #( #endpoint_refs ),* ],
            };
    }
}

fn emit_origin_descriptor(origin: &ApiOriginIr) -> TokenStream2 {
    match origin {
        ApiOriginIr::FixedSingle(origin) => {
            let fixed = emit_fixed_origin(origin);
            quote! { ::concord_core::__private::v1::ApiOriginDescriptor::FixedSingleOrigin(#fixed) }
        }
        ApiOriginIr::Dynamic => {
            quote! { ::concord_core::__private::v1::ApiOriginDescriptor::DynamicOrigin }
        }
        ApiOriginIr::Multi => {
            quote! { ::concord_core::__private::v1::ApiOriginDescriptor::MultiOrigin }
        }
    }
}

fn emit_fixed_origin(origin: &FixedOriginIr) -> TokenStream2 {
    let authority = LitStr::new(&origin.authority, Span::call_site());
    let scheme = match origin.scheme {
        OriginSchemeIr::Http => quote! { ::concord_core::__private::v1::OriginScheme::Http },
        OriginSchemeIr::Https => quote! { ::concord_core::__private::v1::OriginScheme::Https },
    };
    quote! {
        ::concord_core::__private::v1::FixedOriginDescriptor {
            scheme: #scheme,
            authority: #authority,
        }
    }
}

// Keep feature-domain macro chunks in separate files without widening helper visibility.
include!("client.rs");
include!("endpoints/mod.rs");
include!("policy/mod.rs");

#[cfg(test)]
mod tests;
