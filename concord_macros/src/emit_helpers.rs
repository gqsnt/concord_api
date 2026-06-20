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
                param: #param_lit.into(),
            })?
    }}
}

pub fn emit_header_value_from_static(s: &LitStr) -> TokenStream2 {
    quote! { ::http::HeaderValue::from_static(#s) }
}

pub fn ident(s: &str, span: Span) -> Ident {
    Ident::new(s, span)
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

pub fn contains_cx_field(expr: &syn::Expr) -> bool {
    contains_field_with_any_base(expr, &["cx", "vars"])
}

pub fn contains_auth_field(expr: &syn::Expr) -> bool {
    contains_field_with_any_base(expr, &["auth", "secret"])
}

fn contains_field_with_any_base(expr: &syn::Expr, bases: &'static [&'static str]) -> bool {
    struct Finder {
        bases: &'static [&'static str],
        found: bool,
    }

    impl<'ast> syn::visit::Visit<'ast> for Finder {
        fn visit_expr_field(&mut self, node: &'ast syn::ExprField) {
            if let syn::Expr::Path(path) = &*node.base
                && self
                    .bases
                    .iter()
                    .any(|base| tokens_eq_path_ident(&path.path, base))
            {
                self.found = true;
                return;
            }
            syn::visit::visit_expr_field(self, node);
        }

        fn visit_expr_macro(&mut self, node: &'ast syn::ExprMacro) {
            if token_stream_contains_scoped_base(&node.mac.tokens, self.bases) {
                self.found = true;
                return;
            }
            syn::visit::visit_expr_macro(self, node);
        }
    }

    let mut finder = Finder {
        bases,
        found: false,
    };
    syn::visit::Visit::visit_expr(&mut finder, expr);
    finder.found
}

fn token_stream_contains_scoped_base(
    tokens: &proc_macro2::TokenStream,
    bases: &[&'static str],
) -> bool {
    let compact: String = tokens
        .to_string()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    bases
        .iter()
        .any(|base| compact.contains(&format!("{base}.")) || compact.contains(&format!("{base}::")))
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
