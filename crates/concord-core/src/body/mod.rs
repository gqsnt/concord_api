//! Request-body contracts and the single core-owned prepared-body model.
//!
//! A [`RequestRepresentation`] turns one exact body input into exactly one
//! [`PreparedBody`]. The prepared body is the sole source of the request's
//! actual `Content-Type`, its trustworthy length information and its
//! mechanical replay capability; [`BodyDescription`] is static explanation
//! metadata that plans read without preparing anything.
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
pub use prepared::{BodyFactory, BodyKind, MAX_REQUEST_CONTENT_TYPE_BYTES, PreparedBody};
#[cfg(feature = "multipart")]
pub use spec::Multipart;
pub use spec::{
    BodyDescription, LengthHint, MAX_BODY_MEDIA_HINT_BYTES, PageRequestRepresentation,
    RequestBodyContext, RequestRepresentation,
};
pub use stream::RequestStream;

/// Deprecated name for the one core-owned managed body builder.
///
/// This alias only preserves source that names the managed body type. It does
/// not preserve the retired v2.1 obligation to report request metadata
/// separately from preparation: [`RequestRepresentation`] has a single
/// `prepare` method whose returned [`PreparedBody`] owns the exact media type.
#[deprecated(note = "use `PreparedBody`; the v2.1 dual metadata obligation was removed")]
pub type RequestBody = PreparedBody;

#[doc(hidden)]
pub use spec::{EmptyBody, OptionalRequestRepresentation, PreparedBodySpec, PreparedRequestBody};

#[cfg(all(test, feature = "multipart"))]
pub(crate) use multipart::generated_boundaries_on_this_thread;
pub(crate) use prepared::{BodyFailureState, ManagedBody, NativeBody, Replayability};
