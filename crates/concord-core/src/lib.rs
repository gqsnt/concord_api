//! The runtime of Concord: request composition, native headers, snapshots, the open
//! [`Encode`]/[`Decode`] format contracts, and the one [`Error`] type.
//!
//! Applications normally depend on the `concord` crate, which re-exports everything public here
//! together with the `api!` macro and the standard formats. Depend on this crate directly to
//! write a format (implement [`Encode`] and/or [`Decode`] for a type of yours) without the macro
//! or the standard formats: a statement names your format by its path exactly as it names
//! `concord::Json`, and the generated code calls the same traits.
//!
//! This crate knows no format. It depends on reqwest, `bytes`, `url` and `percent-encoding`;
//! nothing here parses JSON or forms.
//!
//! `__private` is support code for `api!` expansions, not a public API: its names and signatures
//! may change in any release.

pub use {bytes, reqwest};

mod codec;
mod error;
mod header;
mod names;
mod prepared;
mod response;
mod value;

#[doc(hidden)]
pub mod __private;

pub use codec::{
    CODEC_ERROR_CONTEXT, CodecError, ContentTypeField, Decode, Encode, EncodedBody, Incremental,
    Records, ResponseHead,
};
pub use error::{Error, ErrorKind};
pub use header::ToHeaderValue;
pub use names::{HeaderNameError, HeaderNameErrors};
pub use prepared::{Decoder, Endpoint, Prepared};
pub use response::{Incoming, Operation, Received};
pub use value::IntoValue;
