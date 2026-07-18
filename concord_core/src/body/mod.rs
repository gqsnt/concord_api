//! Stable request-body contracts and the single private prepared-body model.

mod error;
mod prepared;
mod stream;

pub use error::{BodyError, BodyErrorKind};
pub use prepared::{BodyFactory, BodyKind, RequestBody};
pub use stream::StreamBody;

#[doc(hidden)]
pub use prepared::{BodyMetadata, BodySpec, CodecBody, EmptyBody, PreparedBody, StreamRequestBody};

pub(crate) use prepared::{BodyFailureState, NativeBody};
