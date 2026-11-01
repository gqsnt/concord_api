//! Concord: a small, typed, declarative layer over [`reqwest`].
//!
//! See the repository's `README.md` and `docs/` for the language and its guarantees.

pub use concord_macros::api;
pub use {bytes, reqwest};

mod error;
mod prepared;
mod repr;
mod value;

#[doc(hidden)]
pub mod __private;

pub use error::{Error, ErrorKind};
pub use prepared::{Decoder, Prepared};
#[cfg(feature = "json")]
pub use repr::Json;
pub use repr::{Binary, BodyError, Decode, Encode, Text};
pub use value::IntoValue;
