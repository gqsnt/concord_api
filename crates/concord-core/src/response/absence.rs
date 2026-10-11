//! Head-only expected absence (`absent on`) and its sealed outcome relation.

use std::marker::PhantomData;

use http::StatusCode;

use crate::endpoint::AbsentStatus;

use super::{Buffered, ResponseBodyError, ResponseDriver, ResponseHead, ResponseRepresentation};

pub(crate) mod private {
    /// Library-owned seal and private constructors of [`super::OptionalOutcome`].
    pub trait OptionalSealed {
        fn absent() -> Self;

        fn present(value: <Self as super::OptionalOutcome>::Present) -> Self
        where
            Self: super::OptionalOutcome;
    }
}

/// The library-sealed relationship between an endpoint result and its
/// head-only expected absence.
///
/// It is implemented only for `Option<T>` with `Present = T`, so a type alias
/// of `Option<T>` works while no other result type can claim an absence rule.
/// The relationship exists for generated type assertions; its absent/present
/// constructors are private to Concord.
///
/// # Extension contract
///
/// * **Ownership.** The trait carries no state; core constructs the absent or
///   present result and moves it to the caller.
/// * **Concurrency.** No behavior of its own; it adds no shared state.
/// * **Cancellation.** No callbacks, so nothing to cancel; an absent result
///   is completed from the response head without polling its body.
/// * **Input sensitivity.** None: the relationship reads no request or
///   response data.
/// * **Errors.** None: absence is a successful outcome for the declared
///   statuses only.
/// * **Resource obligations.** None; the body of an absent response is
///   dropped unread.
/// * **Negative capabilities.** Applications cannot implement it, so they
///   cannot introduce an arbitrary missing-outcome rule or fabricate absence
///   for a status.
#[diagnostic::on_unimplemented(
    message = "[C3-REP] `{Self}` cannot carry expected absence",
    label = "`absent on` requires an `Option<T>` result",
    note = "head-only expected absence is sealed to `Option<T>` results (aliases included): the present payload is `T` and a declared absent status yields `None`",
    note = "write `-> Option<T> via R` on an endpoint with `absent on`, or remove the `absent on` clause"
)]
pub trait OptionalOutcome: private::OptionalSealed + Send + 'static {
    /// The value produced by the present representation on payload success.
    type Present: Send + 'static;
}

impl<T> private::OptionalSealed for Option<T>
where
    T: Send + 'static,
{
    fn absent() -> Self {
        None
    }

    fn present(value: <Self as OptionalOutcome>::Present) -> Self {
        Some(value)
    }
}

impl<T> OptionalOutcome for Option<T>
where
    T: Send + 'static,
{
    type Present = T;
}

/// Constructs the absent value of a sealed optional outcome.
pub(crate) fn absent_outcome<O>() -> O
where
    O: OptionalOutcome,
{
    <O as private::OptionalSealed>::absent()
}

/// Wraps one present value into its sealed optional outcome.
pub(crate) fn present_outcome<O>(value: O::Present) -> O
where
    O: OptionalOutcome,
{
    <O as private::OptionalSealed>::present(value)
}

/// Generated-private present-branch adapter for an endpoint with `absent on`.
///
/// On a payload-success status it forwards [`ResponseRepresentation::ACCEPT`],
/// validation and conversion to the present representation `R` and wraps the
/// converted value as present (`Some`). On a declared absence status the
/// executor never reaches this adapter: it drops the native body unread and
/// constructs the absent value itself. `R` must use the complete
/// [`Buffered`] driver.
#[doc(hidden)]
pub struct HeadAbsence<R>(PhantomData<fn() -> R>);

impl<R, O> ResponseRepresentation<O> for HeadAbsence<R>
where
    O: OptionalOutcome,
    R: ResponseRepresentation<O::Present, Driver = Buffered>,
{
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = R::ACCEPT;

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        R::validate(head)
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<O, ResponseBodyError> {
        R::from_body(body, head).map(present_outcome::<O>)
    }
}

/// Generated-private head-only expected-absence declaration of one endpoint.
///
/// `Resp` is the endpoint's selected response representation. The only
/// constructor, [`ExpectedAbsence::new`], exists for `Resp = HeadAbsence<R>`
/// where the endpoint output implements [`OptionalOutcome`] and `R` is a
/// complete [`Buffered`] representation of the present value; a declaration
/// with a streaming/discard present driver or a non-`Option` result therefore
/// does not type-check.
///
/// Core resolves absence after credential-challenge processing and rate
/// feedback and before accepted-head validation or any body poll. The native
/// body is dropped without polling or draining, so an irrelevant huge,
/// malformed, truncated or stalled body can never become a body-limit or
/// decode failure. Header acquisition and every preceding authority and time
/// limit still apply.
#[doc(hidden)]
pub struct ExpectedAbsence<O, Resp> {
    statuses: AbsentStatus,
    absent: fn() -> O,
    representation: PhantomData<fn() -> Resp>,
}

impl<O, R> ExpectedAbsence<O, HeadAbsence<R>>
where
    O: OptionalOutcome,
    R: ResponseRepresentation<O::Present, Driver = Buffered>,
{
    /// Declares the endpoint-owned absence status set.
    #[must_use]
    pub fn new(statuses: AbsentStatus) -> Self {
        Self {
            statuses,
            absent: absent_outcome::<O>,
            representation: PhantomData,
        }
    }
}

impl<O, Resp> ExpectedAbsence<O, Resp> {
    /// Returns the declared absence statuses.
    #[must_use]
    pub const fn statuses(&self) -> &AbsentStatus {
        &self.statuses
    }

    /// Returns whether a final status is a declared absence.
    #[must_use]
    pub fn contains(&self, status: StatusCode) -> bool {
        self.statuses.contains(status)
    }

    pub(crate) fn absent_value(&self) -> O {
        (self.absent)()
    }
}

impl<O, Resp> Clone for ExpectedAbsence<O, Resp> {
    fn clone(&self) -> Self {
        Self {
            statuses: self.statuses.clone(),
            absent: self.absent,
            representation: PhantomData,
        }
    }
}

impl<O, Resp> std::fmt::Debug for ExpectedAbsence<O, Resp> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExpectedAbsence")
            .field("statuses", &self.statuses)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use http::{HeaderMap, Method, Version};
    use url::Url;

    use super::*;
    use crate::endpoint::EndpointId;
    use crate::response::{ExecutionMeta, ResponseBodyErrorKind};

    type AliasOutcome = Option<u32>;

    struct Number;

    impl ResponseRepresentation<u32> for Number {
        type Driver = Buffered;
        const ACCEPT: Option<&'static str> = Some("text/plain");

        fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<u32, ResponseBodyError> {
            std::str::from_utf8(&body)
                .ok()
                .and_then(|text| text.parse().ok())
                .ok_or_else(|| ResponseBodyError::new(ResponseBodyErrorKind::Decode))
        }
    }

    #[test]
    fn sealed_outcome_accepts_option_aliases_and_constructs_privately() {
        let absent: AliasOutcome = absent_outcome();
        let present: AliasOutcome = present_outcome(7);
        assert_eq!(absent, None);
        assert_eq!(present, Some(7));
    }

    #[test]
    fn head_absence_forwards_accept_and_wraps_present_values() {
        assert_eq!(
            <HeadAbsence<Number> as ResponseRepresentation<AliasOutcome>>::ACCEPT,
            Some("text/plain")
        );
        let url = Url::parse("https://absence.example.test/items/1").expect("url");
        let headers = HeaderMap::new();
        let metadata = ExecutionMeta::new(
            EndpointId::from_static("absence.find"),
            Method::GET,
            0,
            1,
            0,
            false,
        );
        let head = ResponseHead {
            endpoint: "absence.find",
            method: &Method::GET,
            url: &url,
            status: StatusCode::OK,
            version: Version::HTTP_11,
            headers: &headers,
            content_length: Some(2),
            metadata: &metadata,
        };
        let value = <HeadAbsence<Number> as ResponseRepresentation<AliasOutcome>>::from_body(
            Bytes::from_static(b"42"),
            &head,
        )
        .expect("present value");
        assert_eq!(value, Some(42));

        let absence = ExpectedAbsence::<AliasOutcome, HeadAbsence<Number>>::new(
            AbsentStatus::codes([StatusCode::NOT_FOUND]).expect("absence status"),
        );
        assert!(absence.contains(StatusCode::NOT_FOUND));
        assert!(!absence.contains(StatusCode::OK));
        assert_eq!(absence.absent_value(), None);
    }
}
