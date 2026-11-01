//! Concord: a small, typed, declarative layer over [`reqwest`].
//!
//! See the repository's `README.md` and `docs/` for the language and its guarantees.

pub use {bytes, reqwest};

mod error;
mod prepared;
mod repr;
mod value;

#[doc(hidden)]
pub mod __private;

/// Declares an HTTP API as a client → scopes → endpoints tree and generates its typed client.
///
/// See the [crate documentation](crate) for the language.
#[macro_export]
macro_rules! api {
    ($($tokens:tt)*) => {
        $crate::__private::api_impl! { $crate; $($tokens)* }
    };
}

pub use error::{Error, ErrorKind};
pub use prepared::{Decoder, Prepared};
#[cfg(feature = "json")]
pub use repr::Json;
pub use repr::{Binary, BodyError, Decode, Encode, Text};
pub use value::IntoValue;
