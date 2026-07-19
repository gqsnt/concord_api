//! Stable request-body contracts and the single private prepared-body model.

mod error;
#[cfg(feature = "multipart")]
mod multipart;
mod prepared;
mod stream;

pub use error::{BodyError, BodyErrorKind};
#[cfg(feature = "multipart")]
pub use multipart::{MultipartBody, MultipartFactory, MultipartPart};
pub use prepared::{BodyFactory, BodyKind, RequestBody};
pub use stream::StreamBody;

#[doc(hidden)]
pub use prepared::{
    BodyMetadata, BodySpec, CodecBody, EmptyBody, PreparedBody, PreparedRequestBody,
    StreamRequestBody,
};

#[cfg(feature = "multipart")]
pub(crate) use prepared::wrap_multipart_native_body;
pub(crate) use prepared::{BodyFailureState, NativeBody, Replayability};
