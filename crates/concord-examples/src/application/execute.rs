//! A shared execution helper (§23.2).
//!
//! It works for every downstream-generated named request and for the library
//! terminal wrappers (`ResponseRequest`, `AcceptedBytesRequest`,
//! `StoreRequest`, `CollectPrefix`, `CollectAll`) because they all implement
//! the open [`RequestControls`] trait and `IntoFuture`. It needs no native
//! request trait and no private downstream seal; implementing
//! `RequestControls` grants no execution authority.

// ANCHOR: execute_helper
use std::future::IntoFuture;

use concord::{ApiError, CallOptions, RequestControls};

/// Applies the caller's additional caps and awaits the request.
///
/// # Errors
///
/// Whatever the request's own terminal returns.
pub async fn execute<R, T>(request: R, options: &CallOptions) -> Result<T, ApiError>
where
    R: RequestControls + IntoFuture<Output = Result<T, ApiError>>,
{
    request.with_options(options).await
}
// ANCHOR_END: execute_helper
