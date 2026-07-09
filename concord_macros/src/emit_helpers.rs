// concord_macros/src/emit_helpers.rs
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use quote::quote;
use syn::spanned::Spanned;
use syn::{Ident, LitStr};

use crate::limits::{MAX_PUBLIC_EXPR_TOKEN_GROUP_DEPTH, public_expr_token_group_depth_error};

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

pub fn compile_error_tokens(msg: &str, span: Span) -> TokenStream2 {
    let msg = LitStr::new(msg, span);
    quote! { compile_error!(#msg) }
}

pub fn compile_error_expr(msg: &str, span: Span) -> TokenStream2 {
    let msg = LitStr::new(msg, span);
    quote! {{
        compile_error!(#msg);
        loop {}
    }}
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

pub fn public_expr_reserved_root_kind(ident: &Ident) -> Option<PublicExprForbiddenKind> {
    public_expr_forbidden_ident_kind(&unraw_ident_text(ident))
}

pub fn is_public_expr_reserved_root(ident: &Ident) -> bool {
    public_expr_reserved_root_kind(ident).is_some()
}

pub fn is_cx_field(expr: &syn::Expr) -> Option<syn::Ident> {
    match expr {
        syn::Expr::Field(f) => {
            if let syn::Expr::Path(p) = &*f.base
                && tokens_eq_path_ident(&p.path, "vars")
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicExprForbiddenKind {
    Auth,
    Secret,
    GeneratedLocal,
    SecretExposure,
}

#[derive(Clone, Debug)]
pub struct PublicExprForbidden {
    pub ident: String,
    pub span: Span,
    pub kind: PublicExprForbiddenKind,
}

pub fn public_expr_forbidden(expr: &syn::Expr) -> syn::Result<Option<PublicExprForbidden>> {
    struct Finder {
        found: Result<Option<PublicExprForbidden>, syn::Error>,
    }

    impl Finder {
        fn record_ident(&mut self, ident: &Ident) {
            if matches!(&self.found, Ok(None))
                && let Some(kind) = public_expr_reserved_root_kind(ident)
            {
                self.found = Ok(Some(PublicExprForbidden {
                    ident: unraw_ident_text(ident),
                    span: ident.span(),
                    kind,
                }));
            }
        }

        fn record_secret_exposure(&mut self, ident: &Ident) {
            let text = unraw_ident_text(ident);
            if matches!(&self.found, Ok(None)) && is_secret_exposure_method(&text) {
                self.found = Ok(Some(PublicExprForbidden {
                    ident: text,
                    span: ident.span(),
                    kind: PublicExprForbiddenKind::SecretExposure,
                }));
            }
        }

        fn scan_tokens(&mut self, tokens: &TokenStream2) {
            if matches!(&self.found, Ok(None)) {
                self.found = public_token_stream_forbidden(tokens, 0);
            }
        }
    }

    impl<'ast> syn::visit::Visit<'ast> for Finder {
        fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
            if let Some(first) = node.path.segments.first() {
                self.record_ident(&first.ident);
                if matches!(&self.found, Ok(Some(_))) {
                    return;
                }
            }
            syn::visit::visit_expr_path(self, node);
        }

        fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
            self.record_secret_exposure(&node.method);
            if matches!(&self.found, Ok(Some(_))) {
                return;
            }
            syn::visit::visit_expr_method_call(self, node);
        }

        fn visit_macro(&mut self, node: &'ast syn::Macro) {
            if let Some(first) = node.path.segments.first() {
                self.record_ident(&first.ident);
                if matches!(&self.found, Ok(Some(_))) {
                    return;
                }
            }
            self.scan_tokens(&node.tokens);
            if matches!(&self.found, Ok(Some(_))) {
                return;
            }
            syn::visit::visit_macro(self, node);
        }
    }

    let mut finder = Finder { found: Ok(None) };
    syn::visit::Visit::visit_expr(&mut finder, expr);
    finder.found
}

pub fn public_token_stream_forbidden(
    tokens: &TokenStream2,
    depth: usize,
) -> syn::Result<Option<PublicExprForbidden>> {
    if depth > MAX_PUBLIC_EXPR_TOKEN_GROUP_DEPTH {
        return Err(public_expr_token_group_depth_error(tokens.span()));
    }
    let mut prev_was_dot = false;
    for token in tokens.clone() {
        match token {
            TokenTree::Ident(ident) => {
                let text = unraw_ident_text(&ident);
                if prev_was_dot && is_secret_exposure_method(&text) {
                    return Ok(Some(PublicExprForbidden {
                        ident: text,
                        span: ident.span(),
                        kind: PublicExprForbiddenKind::SecretExposure,
                    }));
                }
                if let Some(kind) = public_expr_forbidden_ident_kind(&text) {
                    return Ok(Some(PublicExprForbidden {
                        ident: text,
                        span: ident.span(),
                        kind,
                    }));
                }
                prev_was_dot = false;
            }
            TokenTree::Group(group) => {
                if let Some(found) = public_token_stream_forbidden(&group.stream(), depth + 1)? {
                    return Ok(Some(found));
                }
                prev_was_dot = false;
            }
            TokenTree::Punct(punct) => {
                prev_was_dot = punct.as_char() == '.';
            }
            TokenTree::Literal(_) => {
                prev_was_dot = false;
            }
        }
    }
    Ok(None)
}

fn public_expr_forbidden_ident_kind(ident: &str) -> Option<PublicExprForbiddenKind> {
    match ident {
        "auth" => Some(PublicExprForbiddenKind::Auth),
        "secret" | "secrets" => Some(PublicExprForbiddenKind::Secret),
        "ctx" | "cx" | "ep" | "vars" | "client" | "runtime" | "policy" | "req" | "request"
        | "headers" | "url" | "transport" | "self" => Some(PublicExprForbiddenKind::GeneratedLocal),
        _ => None,
    }
}

fn unraw_ident_text(ident: &Ident) -> String {
    let text = ident.to_string();
    text.strip_prefix("r#").unwrap_or(&text).to_string()
}

fn is_secret_exposure_method(ident: &str) -> bool {
    matches!(ident, "expose" | "expose_secret")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::{Delimiter, Group, TokenStream as TokenStream2, TokenTree};
    use quote::quote;

    fn wrap_in_paren_groups(tokens: TokenStream2, depth: usize) -> TokenStream2 {
        let mut out = tokens;
        for _ in 0..depth {
            let mut group = Group::new(Delimiter::Parenthesis, out);
            group.set_span(Span::call_site());
            let mut next = TokenStream2::new();
            next.extend([TokenTree::Group(group)]);
            out = next;
        }
        out
    }

    #[test]
    fn compile_error_expr_is_a_valid_expression_block() {
        let tokens = compile_error_expr("missing default", Span::call_site());
        let rendered = tokens.to_string();
        assert!(rendered.contains("compile_error"));
        assert!(rendered.contains("loop"));
    }

    #[test]
    fn compile_error_expr_can_be_wrapped_in_a_field_initializer() {
        let err = compile_error_expr("missing default", Span::call_site());
        let rendered = quote!(field_name: #err).to_string();
        assert!(rendered.contains("field_name"));
        assert!(rendered.contains("compile_error"));
        assert!(rendered.contains("loop"));
    }

    #[test]
    fn public_token_stream_forbidden_detects_secret_exposure_inside_nested_groups() {
        let tokens = wrap_in_paren_groups(quote!(value.expose_secret()), 2);
        let found = public_token_stream_forbidden(&tokens, 0)
            .expect("scanner should succeed")
            .expect("secret exposure should be detected");
        assert_eq!(found.kind, PublicExprForbiddenKind::SecretExposure);
    }

    #[test]
    fn public_token_stream_forbidden_rejects_excessive_group_nesting() {
        let tokens = wrap_in_paren_groups(
            quote!(LEAK_SENTINEL_PUBLIC_EXPR_TOKEN),
            MAX_PUBLIC_EXPR_TOKEN_GROUP_DEPTH + 1,
        );
        let err = public_token_stream_forbidden(&tokens, 0).expect_err("scanner should fail");
        let rendered = err.to_string();
        assert!(
            rendered.contains("public expression token nesting exceeds maximum supported depth")
        );
        assert!(rendered.contains(&MAX_PUBLIC_EXPR_TOKEN_GROUP_DEPTH.to_string()));
        assert!(!rendered.contains("LEAK_SENTINEL_PUBLIC_EXPR_TOKEN"));
    }
}
