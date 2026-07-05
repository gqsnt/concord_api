use proc_macro2::Span;
use std::cell::Cell;

pub(crate) const MAX_DSL_SCOPE_DEPTH: usize = 64;
pub(crate) const MAX_PUBLIC_EXPR_TOKEN_GROUP_DEPTH: usize = 64;

thread_local! {
    static DSL_SCOPE_DEPTH: Cell<usize> = const { Cell::new(0) };
}

pub(crate) struct DslScopeDepthGuard;

impl DslScopeDepthGuard {
    pub(crate) fn enter(span: Span) -> syn::Result<Self> {
        DSL_SCOPE_DEPTH.with(|depth| {
            let current = depth.get();
            if current >= MAX_DSL_SCOPE_DEPTH {
                return Err(dsl_scope_depth_error(span));
            }
            depth.set(current + 1);
            Ok(Self)
        })
    }
}

impl Drop for DslScopeDepthGuard {
    fn drop(&mut self) {
        DSL_SCOPE_DEPTH.with(|depth| {
            depth.set(depth.get().saturating_sub(1));
        });
    }
}

pub(crate) fn dsl_scope_depth_error(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        format!(
            "DSL scope nesting exceeds maximum supported depth of {}",
            MAX_DSL_SCOPE_DEPTH
        ),
    )
}

pub(crate) fn public_expr_token_group_depth_error(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        format!(
            "public expression token nesting exceeds maximum supported depth of {}",
            MAX_PUBLIC_EXPR_TOKEN_GROUP_DEPTH
        ),
    )
}

pub(crate) fn check_dsl_scope_depth(depth: usize, span: Span) -> syn::Result<()> {
    if depth > MAX_DSL_SCOPE_DEPTH {
        Err(dsl_scope_depth_error(span))
    } else {
        Ok(())
    }
}
