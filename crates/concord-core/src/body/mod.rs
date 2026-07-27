//! Stable request-body contracts and the single private prepared-body model.
//!
//! Every managed request stream suppresses empty chunks before native
//! transport and accepts at most [`MAX_EMPTY_REQUEST_CHUNKS`] for one body.
//! Empty chunks do not consume the byte limit or advance an exact length.

mod error;
#[cfg(feature = "multipart")]
mod multipart;
mod prepared;
mod spec;
mod stream;

pub use crate::limits::MAX_EMPTY_REQUEST_CHUNKS;
pub use error::{BodyError, BodyErrorKind};
#[cfg(feature = "multipart")]
pub use multipart::{
    MAX_MULTIPART_CONTENT_TYPE_BYTES, MAX_MULTIPART_FILENAME_BYTES,
    MAX_MULTIPART_INLINE_TEXT_BYTES, MAX_MULTIPART_METADATA_BYTES, MAX_MULTIPART_NAME_BYTES,
    MAX_MULTIPART_PARTS, MultipartBody, MultipartFactory, MultipartPart,
};
pub use prepared::{BodyFactory, BodyKind, RequestBody};
#[cfg(feature = "multipart")]
pub use spec::Multipart;
pub use spec::{
    LengthHint, PageRequestBodySpec, RequestBodyContext, RequestBodyMetadata, RequestBodySpec,
    RequestContentType, Stream,
};
pub use stream::StreamBody;

#[doc(hidden)]
pub use spec::{EmptyBody, PreparedBodySpec, PreparedRequestBody};

#[cfg(feature = "multipart")]
pub(crate) use prepared::wrap_multipart_native_body;
pub(crate) use prepared::{BodyFailureState, NativeBody, PreparedBody, Replayability};
pub(crate) use spec::validate_produced_body;
