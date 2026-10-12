//! Code generation from the resolved semantic model.
//!
//! Every generated item targets the facade's versioned `__private` bridge.
//! Generated support modules mirror the owner tree: the client module holds
//! the builder, root state, credential facade, contract and root endpoints;
//! each scope module holds its handle, state, unbound handle and endpoints.
//! Hidden storage fields stay private: navigation methods are emitted in the
//! child module (a descendant may access its ancestors' private fields).

mod client;
mod contract;
mod docs;
mod endpoint;
mod head;
mod owner;
mod rust_paths;

use std::time::Duration;

use proc_macro2::{Span, TokenStream};
use quote::quote;

use crate::semantic::{Model, Site, VRef};

/// Generated-code contract version shared with core.
pub(crate) const GENERATED_CONTRACT_VERSION: u32 = 34;

pub(crate) fn generate(model: &Model, facade: &TokenStream) -> TokenStream {
    let context = Context {
        model,
        f: facade,
        rust_depth: 1,
    };
    client::generate(&context)
}

pub(crate) struct Context<'a> {
    pub(crate) model: &'a Model,
    pub(crate) f: &'a TokenStream,
    /// Generated module distance from the `api!` invocation.
    pub(crate) rust_depth: usize,
}

impl Context<'_> {
    pub(crate) fn rust_type(&self, ty: &syn::Type) -> syn::Type {
        rust_paths::ty(ty, self.rust_depth)
    }

    pub(crate) fn rust_path(&self, path: &syn::Path) -> syn::Path {
        rust_paths::path(path, self.rust_depth)
    }

    /// `F::__private`.
    pub(crate) fn p(&self) -> TokenStream {
        let f = self.f;
        quote!(#f::__private)
    }
}

/// `super::super::...` repeated `count` times (empty for zero).
pub(crate) fn supers(count: usize) -> TokenStream {
    let supers = (0..count).map(|_| quote!(super::));
    quote!(#(#supers)*)
}

pub(crate) fn duration_tokens(duration: Duration) -> TokenStream {
    let seconds = duration.as_secs();
    let nanos = duration.subsec_nanos();
    quote!(::std::time::Duration::new(#seconds, #nanos))
}

pub(crate) fn option_duration(duration: Option<Duration>) -> TokenStream {
    match duration {
        Some(duration) => {
            let duration = duration_tokens(duration);
            quote!(::std::option::Option::Some(#duration))
        }
        None => quote!(::std::option::Option::None),
    }
}

/// How `self` reaches values in a generated method body.
#[derive(Clone, Copy)]
pub(crate) enum Access {
    /// `self` is the `__State` of owner `owner`.
    OwnerState(usize),
    /// `self` has `__owner` (the state of the endpoint's owner) and
    /// `__local` (the endpoint's value storage).
    Endpoint(usize),
}

impl Access {
    fn owner(self, model: &Model) -> usize {
        match self {
            Self::OwnerState(owner) => owner,
            Self::Endpoint(endpoint) => model.endpoints[endpoint].owner,
        }
    }

    fn base(self) -> TokenStream {
        match self {
            Self::OwnerState(_) => quote!(self),
            Self::Endpoint(_) => quote!(self.__owner),
        }
    }
}

/// Expression reaching the state of `target` (an ancestor or the owner
/// itself) from `access`.
pub(crate) fn owner_state_expr(model: &Model, access: Access, target: usize) -> TokenStream {
    let owner = access.owner(model);
    let hops = model.owners[owner].depth - model.owners[target].depth;
    let base = access.base();
    let parents = (0..hops).map(|_| quote!(.__parent));
    quote!(#base #(#parents)*)
}

/// Expression of the storage field of `key` (`Arc<T>` or `Option<Arc<T>>`).
pub(crate) fn value_field_expr(model: &Model, access: Access, key: VRef) -> TokenStream {
    let value = model.value(key);
    let field = &value.field;
    match key.site {
        Site::Endpoint(_) => quote!(self.__local.#field),
        Site::Owner(0) => {
            let state = owner_state_expr(model, access, 0);
            quote!(#state.#field)
        }
        Site::Owner(owner) => {
            let state = owner_state_expr(model, access, owner);
            quote!(#state.__local.#field)
        }
    }
}

/// Expression of a total value's payload reference `&T`.
pub(crate) fn total_ref(model: &Model, access: Access, key: VRef) -> TokenStream {
    let field = value_field_expr(model, access, key);
    quote!(&*#field)
}

/// Expression of the root state from `access`.
pub(crate) fn root_expr(model: &Model, access: Access) -> TokenStream {
    owner_state_expr(model, access, 0)
}

pub(crate) fn lit(text: &str) -> syn::LitStr {
    syn::LitStr::new(text, Span::call_site())
}

/// Relocates every token to `span` for diagnostics while keeping each token's
/// own name resolution (`Span::located_at`).
///
/// DIAG-01: an obligation that fails on generated tokens is then reported at
/// the declaration clause that selected them rather than across the whole
/// invocation. Only the location changes; hygiene is untouched.
pub(crate) fn located_at(tokens: TokenStream, span: Span) -> TokenStream {
    tokens
        .into_iter()
        .map(|tree| match tree {
            proc_macro2::TokenTree::Group(group) => {
                let mut relocated =
                    proc_macro2::Group::new(group.delimiter(), located_at(group.stream(), span));
                relocated.set_span(group.span().located_at(span));
                proc_macro2::TokenTree::Group(relocated)
            }
            mut other => {
                other.set_span(other.span().located_at(span));
                other
            }
        })
        .collect()
}

/// `#[allow(deprecated)]` for generated support items when the declaration
/// deprecates a client, scope or endpoint (§3.5).
///
/// Deprecation must fire where user code names or calls a deprecated
/// generated item, never inside the generated code that implements it:
/// without this, the request/intent/handle impls and the fields of a
/// deprecated generated type report deprecation at the declaration, which
/// breaks `#![deny(deprecated)]` crates. It is emitted only when needed, so an
/// ordinary declaration keeps every deprecation warning for user types it
/// names. Deprecated values need nothing: their controls are never called
/// internally.
pub(crate) fn internal_deprecation_allowance(model: &Model) -> TokenStream {
    let deprecated = model
        .owners
        .iter()
        .map(|owner| &owner.meta)
        .chain(model.endpoints.iter().map(|endpoint| &endpoint.meta))
        .any(|meta| meta.deprecated.is_some());
    if deprecated {
        quote!(#[allow(deprecated)])
    } else {
        TokenStream::new()
    }
}

pub(crate) fn contract_assertion(context: &Context<'_>) -> TokenStream {
    let p = context.p();
    let version = GENERATED_CONTRACT_VERSION;
    quote!(const _: () = #p::assert_generated_contract(#version);)
}
