//! Raw parser AST for the Concord DSL.
//!
//! This module is intentionally syntax-shaped. Codegen must
//! never consume these types directly; `sema` is the boundary that normalizes
//! them into resolved semantic data.

use crate::model::Scheme;
use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{Expr, Ident, LitInt, LitStr, Path, Type};

// Keep AST definitions grouped by DSL concept while preserving a single ast namespace.
include!("common.rs");
include!("auth.rs");
include!("routing.rs");
include!("rate_limit.rs");
include!("behavior.rs");
include!("pagination.rs");
include!("policy.rs");
include!("raw.rs");
