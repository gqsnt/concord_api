//! Generated-code bridge for pageable endpoints.
//!
//! The declaration compiler implements [`PageableEndpoint`] for every endpoint
//! with a `paginate via Controller { init Init { .. }; cursor -> slot; }`
//! clause and emits the `.pages()`, `.collect_prefix()` and `.collect_all()`
//! methods on the generated request wrapper as thin calls to
//! [`pages_request`], [`collect_prefix_request`] and [`collect_all_request`].
//! Core owns the single traversal loop; the bridge only supplies typed
//! construction and whole-cursor slot binding.

use std::num::NonZeroU32;

use crate::body::PageRequestRepresentation;
use crate::request::{EndpointSpec, Request};
use crate::response::{Buffered, ResponseRepresentation};

use super::PaginationController;
use super::terminals::{
    CollectAll, CollectPrefix, Pages, new_collect_all, new_collect_prefix, new_pages,
};

/// Public initialization type of a pageable endpoint's controller.
#[doc(hidden)]
pub type ControllerInit<E> = <<E as PageableEndpoint>::Controller as PaginationController<
    <E as EndpointSpec>::Output,
>>::Init;

/// Whole cursor type of a pageable endpoint's controller.
#[doc(hidden)]
pub type ControllerCursor<E> = <<E as PageableEndpoint>::Controller as PaginationController<
    <E as EndpointSpec>::Output,
>>::Cursor;

/// Item type delivered by a pageable endpoint's traversal.
#[doc(hidden)]
pub type PageItem<E> = <<E as PageableEndpoint>::Controller as PaginationController<
    <E as EndpointSpec>::Output,
>>::Item;

/// Generated-private typed pagination integration.
///
/// The page type is the endpoint's decoded [`EndpointSpec::Output`]. Core
/// requires, at every terminal constructor, that the selected response
/// representation uses the complete [`Buffered`] driver and that the request
/// representation opts into [`PageRequestRepresentation`] (`EmptyBody` for a
/// bodyless endpoint; an optional body needs the static capability even when
/// absent). Core additionally checks at first poll that the endpoint effect is
/// `Safe`, so a forged implementation cannot page an unsafe endpoint.
///
/// # What the macro must generate
///
/// * `type Controller` — the controller named by `paginate via`.
/// * [`controller_init`](Self::controller_init) — the declared
///   `init Init { field: value, .. }` expression. Stable values are cloned
///   constants; references clone the current effective slot value. It must
///   not have side effects.
/// * [`apply_cursor`](Self::apply_cursor) — assigns the **whole** cursor value
///   to the declared destination: `self.after = cursor;` for `cursor -> after`
///   (for `after?: String` the cursor is `Option<String>` and replaces the
///   whole optional slot), or `(self.a, self.b) = cursor;` for a tuple
///   destination. No field extraction, `Option` wrapping or conversion.
///   Destinations are unique endpoint-owned scalar slots only (never body,
///   client/scope values, credentials, `each` sources or rate-key inputs).
/// * [`page_clone`](Self::page_clone) — a fresh endpoint value with identical
///   public slots and a page-stable copy of the captured body input (the
///   compiler guarantees `Input: Clone` because the body representation is a
///   `PageRequestRepresentation`).
///
/// # Negative capabilities
///
/// Implementing this trait grants no authority: core re-validates every
/// continuation's public shaping, origin, effect, timeout, accepted status,
/// rate-limit plan and body contract, and rejects repeats and cycles.
#[doc(hidden)]
pub trait PageableEndpoint: EndpointSpec + Sized {
    /// Controller interpreting pages of the endpoint's decoded output.
    type Controller: PaginationController<Self::Output>;

    /// Optional durable state callbacks selected by generated code.
    fn checkpoint_codec(&self) -> Option<super::CheckpointCodec<Self::Controller, Self::Output>> {
        None
    }

    /// Canonical public non-cursor inputs, excluding all sensitive values.
    fn checkpoint_input(&self) -> Result<Vec<u8>, super::CheckpointError> {
        Ok(Vec::new())
    }

    /// Whether a mapped cursor destination is sensitive.
    fn checkpoint_sensitive_cursor(&self) -> bool {
        false
    }

    /// Builds the controller's public initialization value.
    fn controller_init(&self) -> ControllerInit<Self>;

    /// Replaces the declared cursor destination slot(s) with the whole cursor.
    fn apply_cursor(&mut self, cursor: ControllerCursor<Self>);

    /// Reconstructs an independent page-stable copy of this endpoint.
    fn page_clone(&self) -> Self;

    /// Returns whether this endpoint's body slot is outer-present, without
    /// preparing it. Bodyless endpoints return `false`; a required body
    /// returns `true`. Body presence is part of page progress identity: a
    /// continuation that changes it is rejected before the proposing page is
    /// delivered. (The executor additionally verifies the prepared body kind
    /// and exact Content-Type of every page before dispatch.)
    fn page_body_present(&self) -> bool;
}

/// Generated `.pages(maximum_pages)`: constructs an inert lazy traversal.
///
/// `maximum_pages == 0` drops the request without any work and yields a
/// traversal whose summary is `PageBound`/`Unknown` with no operation identity.
#[doc(hidden)]
#[must_use]
pub fn pages_request<E>(request: Request<E>, maximum_pages: u32) -> Pages<PageItem<E>>
where
    E: PageableEndpoint,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
    E::Response: ResponseRepresentation<E::Output, Driver = Buffered>,
{
    new_pages(request, maximum_pages)
}

/// Generated `.collect_prefix(maximum_pages)`: constructs an inert prefix
/// collector carrying the request's controls.
#[doc(hidden)]
#[must_use]
pub fn collect_prefix_request<E>(
    request: Request<E>,
    maximum_pages: u32,
) -> CollectPrefix<PageItem<E>>
where
    E: PageableEndpoint,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
    E::Response: ResponseRepresentation<E::Output, Driver = Buffered>,
{
    new_collect_prefix(request, maximum_pages)
}

/// Generated `.collect_all(maximum_pages)`: constructs an inert complete
/// collector carrying the request's controls.
#[doc(hidden)]
#[must_use]
pub fn collect_all_request<E>(
    request: Request<E>,
    maximum_pages: NonZeroU32,
) -> CollectAll<PageItem<E>>
where
    E: PageableEndpoint,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
    E::Response: ResponseRepresentation<E::Output, Driver = Buffered>,
{
    new_collect_all(request, maximum_pages)
}

/// Restores a lazy traversal through the ordinary managed pipeline.
#[doc(hidden)]
pub fn resume_pages_request<E>(
    request: Request<E>,
    checkpoint: &super::Checkpoint,
    maximum_pages: u32,
) -> Result<Pages<PageItem<E>>, super::CheckpointError>
where
    E: PageableEndpoint,
    E::Controller: super::ResumableController<E::Output>,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
    E::Response: ResponseRepresentation<E::Output, Driver = Buffered>,
{
    super::terminals::new_resumed_pages(request, checkpoint, maximum_pages)
}
