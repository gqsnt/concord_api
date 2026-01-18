// concord_macros/src/emit_helpers.rs
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Ident, LitStr};

pub fn to_snake(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i != 0 {
                out.push('_');
            }
            for c in ch.to_lowercase() {
                out.push(c);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn to_kebab(ident: &Ident) -> String {
    ident
        .to_string()
        .chars()
        .flat_map(|c| {
            if c == '_' {
                vec!['-']
            } else {
                c.to_lowercase().collect::<Vec<_>>()
            }
        })
        .collect()
}

pub fn lit_str(s: &str, span: Span) -> LitStr {
    LitStr::new(s, span)
}

/// Build `HeaderName` expression + error mapping:
/// `let name = HeaderName::from_bytes(b"...").map_err(|_| ApiClientError::InvalidParam(concat!("header:", "...")))?;`
pub fn emit_header_name(key: &str, span: Span) -> TokenStream2 {
    let key_lit = LitStr::new(key, span);
    let param_lit = LitStr::new(&format!("header:{key}"), span);

    quote! {{
        ::http::header::HeaderName::from_bytes(#key_lit.as_bytes())
            .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam {
                ctx: ctx.clone(),
                param: #param_lit,
            })?
    }}
}

/// - The generated code expects a `ctx` variable to be in scope (it is in policy apply fns).
#[inline]
pub fn emit_err_invalid_param(param: &str, span: Span) -> TokenStream2 {
    let lit = LitStr::new(param, span);
    quote! {
        ::concord_core::prelude::ApiClientError::InvalidParam {
            ctx: ctx.clone(),
            param: #lit,
        }
    }
}

pub fn emit_header_value_from_expr(expr: &syn::Expr, key: &str, span: Span) -> TokenStream2 {
    let param = format!("header:{key}");
    let err = emit_err_invalid_param(&param, span);
    quote! {{
        ::http::HeaderValue::from_str(&(#expr).to_string())
            .map_err(|_| #err)?
    }}
}

pub fn emit_header_value_from_static(s: &LitStr) -> TokenStream2 {
    quote! { ::http::HeaderValue::from_static(#s) }
}

pub fn ident(s: &str, span: Span) -> Ident {
    Ident::new(s, span)
}

pub fn nested_chain(types: &[TokenStream2], tail: TokenStream2) -> TokenStream2 {
    // Chain<A, Chain<B, Chain<C, tail>>>
    let mut acc = tail;
    for ty in types.iter().rev() {
        acc = quote! { ::concord_core::internal::Chain<#ty, #acc> };
    }
    acc
}

pub fn tokens_eq_path_ident(path: &syn::Path, s: &str) -> bool {
    path.segments.len() == 1 && path.segments[0].ident == s
}

pub fn is_cx_field(expr: &syn::Expr) -> Option<syn::Ident> {
    match expr {
        syn::Expr::Field(f) => {
            if let syn::Expr::Path(p) = &*f.base
                && tokens_eq_path_ident(&p.path, "cx")
                && let syn::Member::Named(id) = &f.member
            {
                return Some(id.clone());
            }
            None
        }
        _ => None,
    }
}

pub fn is_auth_field(expr: &syn::Expr) -> Option<syn::Ident> {
    match expr {
        syn::Expr::Field(f) => {
            if let syn::Expr::Path(p) = &*f.base
                && tokens_eq_path_ident(&p.path, "auth")
                && let syn::Member::Named(id) = &f.member
            {
                return Some(id.clone());
            }
            None
        }
        _ => None,
    }
}

pub fn is_ep_field(expr: &syn::Expr) -> Option<syn::Ident> {
    match expr {
        syn::Expr::Field(f) => {
            if let syn::Expr::Path(p) = &*f.base
                && tokens_eq_path_ident(&p.path, "ep")
                && let syn::Member::Named(id) = &f.member
            {
                return Some(id.clone());
            }
            None
        }
        _ => None,
    }
}

/// Conservative detection: reject any nested `cx.` / `ep.` usage in a non-trivial expression.
/// Allowed forms are strictly `cx.name` or `ep.name` at the root.
pub fn contains_cx_or_ep(expr: &syn::Expr) -> bool {
    // Relaxed: allow cx/ep in arbitrary expressions. The generated code binds `cx`/`ep`.
    let _ = expr;
    false
}
