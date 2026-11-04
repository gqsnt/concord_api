//! One `api!` invocation receives `$crate::Payload` from two crates' macros: identical tokens that
//! name two different types. A proc macro cannot see `macro_rules` hygiene, so the conditional
//! `Clone` of an endpoint request keeps one bound per occurrence: the declaration compiles whichever
//! payload is `Clone`, the request works, and only an actual `.clone()` of a request holding a
//! non-`Clone` value fails (`tests/ui`). Found by the 4.1 engineering review (F01): deduplicating
//! the bounds by their printed text dropped the bound of the second, non-`Clone` type.
//!
//! Helpers `a` and `c` have a `Clone` payload, `b` and `d` do not.

use concord::{CodecError, Decode, Encode, ResponseHead, bytes::Bytes, reqwest};

/// Encodes a displayable body as its text, with no `Content-Type`.
pub struct Shown;

impl<T: core::fmt::Display> Encode<T> for Shown {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(value: T) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value.to_string()))
    }
}

/// Decodes a body of decimal digits into a payload of any helper (`Payload(u16)`).
pub struct Number;

macro_rules! number_decodes {
    ($($krate:ident),*) => {$(
        impl Decode<$krate::Payload> for Number {
            type Context = ();
            fn check_content_type(_: &ResponseHead<'_>) -> Result<(), CodecError> {
                Ok(())
            }
            fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
                Ok(())
            }
            fn decode_body(_: (), body: Bytes) -> Result<$krate::Payload, CodecError> {
                core::str::from_utf8(&body)
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .map($krate::Payload)
                    .ok_or_else(|| CodecError::new("not a number"))
            }
        }
    )*};
}
number_decodes!(
    concord_clone_hygiene_a,
    concord_clone_hygiene_b,
    concord_clone_hygiene_c,
    concord_clone_hygiene_d
);

/// Declares client `Probe` over two payload types: two required query values, two optional ones,
/// and a query value with a body.
#[macro_export]
macro_rules! probe {
    ($left:path, $right:path) => {
        concord::api! {
            /// Two payload types, possibly printed the same.
            pub client Probe {
                base "http://clone-hygiene.invalid";
                /// Two required query values.
                GET pair(query first: $left, query second: $right) { path "pair"; }
                /// Two optional query values.
                GET optional(query first?: $left, query second?: $right) { path "optional"; }
                /// A query value and a body.
                POST body(query first: $left, body second: $right) {
                    path "body";
                    request $crate::Shown;
                }
            }
        }
    };
}

/// `a` (`Clone`) then `b` (not `Clone`), both written `$crate::Payload`: the declaration that
/// failed with E0277 in 4.1.
pub mod forward {
    macro_rules! assemble {
        ($left:path; $right:path) => {
            $crate::probe! { $left, $right }
        };
    }
    concord_clone_hygiene_a::begin!(concord_clone_hygiene_b::append);
}

/// `b` (not `Clone`) then `a` (`Clone`).
pub mod reverse {
    macro_rules! assemble {
        ($left:path; $right:path) => {
            $crate::probe! { $left, $right }
        };
    }
    concord_clone_hygiene_b::begin!(concord_clone_hygiene_a::append);
}

/// `a` then `c`, both `Clone`: the requests are `Clone`.
pub mod both_clone {
    macro_rules! assemble {
        ($left:path; $right:path) => {
            $crate::probe! { $left, $right }
        };
    }
    concord_clone_hygiene_a::begin!(concord_clone_hygiene_c::append);
}

/// `b` then `d`, neither `Clone`.
pub mod both_plain {
    macro_rules! assemble {
        ($left:path; $right:path) => {
            $crate::probe! { $left, $right }
        };
    }
    concord_clone_hygiene_b::begin!(concord_clone_hygiene_d::append);
}

/// `a` twice: one type, written the same way twice.
pub mod same_type {
    macro_rules! assemble {
        ($left:path; $right:path) => {
            $crate::probe! { $left, $right }
        };
    }
    concord_clone_hygiene_a::begin!(concord_clone_hygiene_a::append);
}

/// The two types of `forward` written as direct, differently printed paths.
pub mod direct {
    crate::probe! { concord_clone_hygiene_a::Payload, concord_clone_hygiene_b::Payload }
}

/// Output types written as another crate's type macro, through absolute and qualified paths and
/// all three delimiters (F02): ordinary output types, decoded by `response Number;`.
pub mod type_macros {
    concord::api! {
        /// Outputs named by type macros.
        pub client Macros {
            base "http://type-macros.invalid";
            response crate::Number;
            /// Absolute path, parentheses.
            GET absolute() -> ::concord_clone_hygiene_a::payload_type!();
            /// Relative path, brackets.
            GET brackets() -> concord_clone_hygiene_b::payload_type![];
            /// Absolute path, braces.
            GET braces() -> ::concord_clone_hygiene_c::payload_type! {} { path "braces"; }
            /// A type macro body and output.
            POST echo(body value: concord_clone_hygiene_d::payload_type!()) -> concord_clone_hygiene_d::payload_type!() {
                request crate::Shown;
            }
        }
    }
}
