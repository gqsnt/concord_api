//! Private executable proof of pagination identity and aggregate bounds.
//!
//! This module is compiled only for `concord_core` unit tests. It is not the
//! production pagination runtime and deliberately has no HTTP integration.

mod identity;
mod limits;
mod model;

#[cfg(test)]
mod tests;

use identity::IdentityEncodingError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Boundary {
    Hook,
    Credential,
    Limiter,
    Transport,
    Controller,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PaginationError {
    IdentityEncoding(IdentityEncodingError),
    RepeatedIdentity,
    RetainedIdentityByteOverflow,
    RetainedIdentityByteLimit,
    PageCountOverflow,
    PageLimit,
    ItemCountOverflow,
    ItemLimit,
    BoundaryCountOverflow(Boundary),
    Controller,
    TransportScriptExhausted,
}

impl From<IdentityEncodingError> for PaginationError {
    fn from(error: IdentityEncodingError) -> Self {
        Self::IdentityEncoding(error)
    }
}
