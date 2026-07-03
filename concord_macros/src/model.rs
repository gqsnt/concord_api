//! Shared macro model primitives and normalized semantic tree.
//!
//! Raw parser structs stay in `ast`. `NormApiTree` is the first semantic
//! boundary: parser-only details are normalized before semantic resolution
//! consumes them. Generated code must depend only on resolved sema output plus
//! neutral primitives such as `Scheme` and `SetOp`.

pub(crate) mod docs;
pub(crate) mod facade;
pub(crate) mod norm;

pub(crate) use norm::*;

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
