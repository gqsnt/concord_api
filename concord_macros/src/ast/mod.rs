use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{Expr, Ident, LitBool, LitInt, LitStr, Path, Type};

// Keep AST definitions grouped by DSL concept while preserving a single ast namespace.
include!("common.rs");
include!("auth.rs");
include!("routing.rs");
include!("cache.rs");
include!("retry.rs");
include!("rate_limit.rs");
include!("pagination.rs");
include!("mapping.rs");
include!("policy.rs");
