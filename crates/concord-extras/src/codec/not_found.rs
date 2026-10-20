use std::marker::PhantomData;

use concord_core::contracts::{
    Buffered, ResponseBodyError, ResponseDriver, ResponseHead, ResponseRepresentation, StatusCode,
};

/// Buffered `404`-as-`None` response representation for `Option<T>`.
///
/// `NotFound<R>` is an ordinary buffered representation: it does **not**
/// change status acceptance, so the endpoint's accepted status set must
/// include `404` explicitly. It forwards `R::ACCEPT` unchanged.
///
/// * On an accepted `404`, [`ResponseRepresentation::validate`] bypasses the
///   wrapped representation's media check, the complete `404` body is still
///   acquired under the buffered response limit, and `from_body` returns
///   `None`.
/// * On any other accepted status, validation and conversion delegate to `R`
///   and the value is wrapped as `Some`.
///
/// # Difference from head-only `absent on`
///
/// A declaration's `absent on 404;` is resolved from the response head alone:
/// the native body is dropped without polling, so an oversized, truncated or
/// stalled body cannot fail the call, and `expose accepted_bytes` is not
/// available. `NotFound<R>` instead **acquires the `404` body completely**:
/// accepted bytes can be exposed for it, and an oversized or truncated `404`
/// body fails with a response-body error exactly like any other buffered
/// response. Choose `NotFound<R>` when the raw bytes of the missing-resource
/// response matter, and `absent on` when they do not.
///
/// Ownership and concurrency: the marker is zero-sized and stateless.
/// Sensitivity: the wrapped representation receives the same accepted head
/// and body it would receive without the wrapper. Errors: non-`404` failures
/// are the wrapped representation's errors unchanged. Negative capabilities:
/// the wrapper cannot accept an additional status, bypass limits or skip body
/// acquisition.
pub struct NotFound<R>(PhantomData<fn() -> R>);

impl<R, T> ResponseRepresentation<Option<T>> for NotFound<R>
where
    R: ResponseRepresentation<T, Driver = Buffered>,
    T: Send + 'static,
{
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = R::ACCEPT;

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        if head.status == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            R::validate(head)
        }
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<Option<T>, ResponseBodyError> {
        if head.status == StatusCode::NOT_FOUND {
            drop(body);
            Ok(None)
        } else {
            R::from_body(body, head).map(Some)
        }
    }
}
