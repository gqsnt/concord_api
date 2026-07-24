//! Declaration compiler for Concord APIs.
//!
//! Applications use the [`api`] reexport from the `concord` façade. This
//! implementation package parses and validates declarations and emits against
//! the façade's versioned generated-code bridge.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod crate_path;
mod emit;
mod semantic;
mod syntax;

use proc_macro::TokenStream;
use proc_macro2::{Ident, TokenStream as TokenStream2};
use syn::parse::{Parse, ParseStream};
use syn::{Token, parenthesized};

// Core owns these public structural limits. The documentation xtask
// mechanically checks these parser/resolver mirrors against core literals.
const MAX_RATE_LIMIT_ID_BYTES: usize = 256;
const MAX_RATE_LIMIT_WINDOWS_PER_RULE: usize = 16;
const MAX_RATE_LIMIT_RULES_PER_PLAN: usize = 64;

/// Declares a generated Concord client and its typed endpoints.
#[proc_macro]
pub fn api(input: TokenStream) -> TokenStream {
    expand(input.into(), emit::Capabilities::default())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Internal façade entry point carrying façade-selected capability tokens.
#[doc(hidden)]
#[proc_macro]
pub fn __api(input: TokenStream) -> TokenStream {
    syn::parse::<FacadeInput>(input)
        .and_then(|input| {
            expand_with_runtime(input.declaration, input.capabilities, &input.runtime_path)
        })
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand(input: TokenStream2, capabilities: emit::Capabilities) -> syn::Result<TokenStream2> {
    let runtime_path = crate_path::resolve_runtime_path()?;
    expand_with_runtime(input, capabilities, &runtime_path)
}

fn expand_with_runtime(
    input: TokenStream2,
    capabilities: emit::Capabilities,
    runtime_path: &TokenStream2,
) -> syn::Result<TokenStream2> {
    syntax::parse(input)
        .and_then(semantic::resolve)
        .map(|model| emit::generate(&model, runtime_path, capabilities))
}

struct FacadeInput {
    capabilities: emit::Capabilities,
    runtime_path: TokenStream2,
    declaration: TokenStream2,
}

impl Parse for FacadeInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<Token![@]>()?;
        let prefix: Ident = input.parse()?;
        if prefix != "concord_capabilities" {
            return Err(syn::Error::new(
                prefix.span(),
                "invalid internal Concord macro capability prefix",
            ));
        }
        let content;
        parenthesized!(content in input);
        let mut capabilities = emit::Capabilities::default();
        while !content.is_empty() {
            let capability: Ident = content.parse()?;
            match capability.to_string().as_str() {
                "raw_response" if !capabilities.raw_response => {
                    capabilities.raw_response = true;
                }
                "raw_response" => {
                    return Err(syn::Error::new(
                        capability.span(),
                        "duplicate internal Concord macro capability",
                    ));
                }
                _ => {
                    return Err(syn::Error::new(
                        capability.span(),
                        "unknown internal Concord macro capability",
                    ));
                }
            }
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }
        input.parse::<Token![@]>()?;
        let prefix: Ident = input.parse()?;
        if prefix != "concord_facade" {
            return Err(syn::Error::new(
                prefix.span(),
                "invalid internal Concord macro façade prefix",
            ));
        }
        let content;
        parenthesized!(content in input);
        let runtime_path = content.parse()?;
        Ok(Self {
            capabilities,
            runtime_path,
            declaration: input.parse()?,
        })
    }
}
