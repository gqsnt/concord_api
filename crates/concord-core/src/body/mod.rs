//! Stable request-body contracts and the single private prepared-body model.

mod error;
#[cfg(feature = "multipart")]
mod multipart;
mod prepared;
mod spec;
mod stream;

pub use error::{BodyError, BodyErrorKind};
#[cfg(feature = "multipart")]
pub use multipart::{MultipartBody, MultipartFactory, MultipartPart};
pub use prepared::{BodyFactory, BodyKind, RequestBody};
#[cfg(feature = "multipart")]
pub use spec::Multipart;
pub use spec::{
    LengthHint, RequestBodyContext, RequestBodyMetadata, RequestBodySpec, RequestContentType,
    Stream,
};
pub use stream::StreamBody;

#[doc(hidden)]
pub use spec::{EmptyBody, PreparedBodySpec, PreparedRequestBody};

#[cfg(feature = "multipart")]
pub(crate) use prepared::wrap_multipart_native_body;
pub(crate) use prepared::{BodyFailureState, NativeBody, PreparedBody, Replayability};
pub(crate) use spec::validate_produced_body;
