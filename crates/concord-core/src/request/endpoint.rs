use crate::body::RequestRepresentation;
use crate::error::ApiError;
use crate::response::{ExpectedAbsence, ResponseRepresentation};

use super::{HeadBuilder, PreparedHead, RequestMeta};

/// Generated-private bridge from one concrete endpoint value into core.
///
/// The request body has exactly one authority: the selected
/// [`RequestRepresentation`] prepares a single `PreparedBody` after public
/// head selection. There is no separate body-metadata callback.
#[doc(hidden)]
pub trait EndpointSpec: Send + 'static {
    /// Exact caller-supplied request-body input, or `()` for a bodyless endpoint.
    type Input: Send + 'static;
    /// Selected request specification, including the private empty bodyless specification.
    type Body: RequestRepresentation<Self::Input>;
    /// Exact value produced by ordinary `.await`.
    type Output: Send + 'static;
    /// Selected accepted-response specification.
    type Response: ResponseRepresentation<Self::Output>;

    /// Returns the operational endpoint ID (an explicit `id` pin or navigation-derived) and
    /// inherited timeout metadata.
    fn request_meta(&self) -> RequestMeta;

    /// Resolves the public request head without materializing the body.
    fn prepare_head(&self, builder: HeadBuilder) -> Result<PreparedHead, ApiError>;

    /// Returns the endpoint's head-only expected absence (`absent on`), if any.
    ///
    /// The default is no absence. Generated code for `absent on S;` returns
    /// `Some(ExpectedAbsence::new(AbsentStatus::codes([S])?))`, which only
    /// type-checks when `Self::Output: OptionalOutcome` and
    /// `Self::Response = HeadAbsence<R>` with a buffered present
    /// representation `R`.
    fn expected_absence(&self) -> Option<ExpectedAbsence<Self::Output, Self::Response>> {
        None
    }

    /// Transfers the selected request-specification input exactly once.
    fn into_body(self) -> Self::Input;
}
