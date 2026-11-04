//! A `Payload` written `$crate::Payload` by this crate's macros: the same tokens as the other
//! helpers' payloads, a different type. Here its `Payload` is `Clone`.

/// The payload; displayed as its number.
#[derive(Clone, Debug, PartialEq)]
pub struct Payload(pub u16);

impl ::core::fmt::Display for Payload {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        ::core::fmt::Display::fmt(&self.0, f)
    }
}

/// Calls `$next!` with this crate's payload type, written `$crate::Payload`.
#[macro_export]
macro_rules! begin {
    ($next:path) => {
        $next! { $crate::Payload }
    };
}

/// Appends this crate's payload type, written `$crate::Payload`, to `$first` and calls
/// `assemble!` (defined by the caller) with both.
#[macro_export]
macro_rules! append {
    ($first:path) => {
        assemble! { $first; $crate::Payload }
    };
}

/// This crate's payload type as a type macro (`::concord_clone_hygiene_c::payload_type!()`).
#[macro_export]
macro_rules! payload_type {
    () => {
        $crate::Payload
    };
}
