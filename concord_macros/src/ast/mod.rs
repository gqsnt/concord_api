//! Raw parser AST for the Concord DSL.
//!
//! This module is intentionally syntax-shaped. It may contain historical
//! fields or removed syntax forms when the parser needs them to emit precise
//! v5 migration diagnostics. Codegen must never consume these types directly;
//! `sema` is the boundary that normalizes them into resolved v5 data.

use crate::model::{Scheme, SetOp};
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
include!("raw.rs");
