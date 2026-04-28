//! Shared macro model primitives.
//!
//! These are syntax-neutral value types used across parse, sema, and codegen.
//! Raw parser structs stay in `ast`; generated code must depend only on sema
//! output plus these neutral primitives.

#[derive(Debug, Clone, Copy)]
pub(crate) enum Scheme {
    Http,
    Https,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetOp {
    Set,
    Push,
}
