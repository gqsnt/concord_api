#![forbid(unsafe_code)]
//! The consuming crate of the two-crate downstream acceptance package
//! (V3-L14, V3-L15, part of V3-O06).
//!
//! `concord-downstream-api` expands `api!`; this crate only uses the public
//! generated surface. It defines the §23.2 generic `execute` helper itself,
//! so the helper's bounds are checked against request types generated in
//! another crate, and it defines a foreign [`RequestControls`]
//! implementation ([`CannedRequest`]) that must not gain native access.
//!
//! # Negative cross-crate checks (V3-L15)
//!
//! A foreign `RequestControls` implementation is not a generated intent. It
//! cannot be bound to a client, so it cannot obtain a native request:
//!
//! ```compile_fail,E0277
//! use concord_downstream_api::InventoryApi;
//! use concord_downstream_app::CannedRequest;
//!
//! fn forge(client: &InventoryApi, canned: CannedRequest<()>) {
//!     let _native = client.request(canned);
//! }
//! ```
//!
//! Nor can it be planned through real public-head preparation:
//!
//! ```compile_fail,E0277
//! use concord_downstream_api::InventoryApi;
//! use concord_downstream_app::CannedRequest;
//!
//! fn forge(client: &InventoryApi, canned: &CannedRequest<()>) {
//!     let _plan = client.plan(canned);
//! }
//! ```
//!
//! The downstream private seal: the generated `GeneratedEndpoint` trait can
//! be named but not implemented outside the generated module, because its
//! supertrait lives in a private module of the declaring crate:
//!
//! ```compile_fail,E0277
//! use concord::ApiError;
//! use concord::endpoint::RequestPlan;
//! use concord_downstream_api::InventoryApi;
//! use concord_downstream_api::inventory_api::GeneratedEndpoint;
//!
//! struct Forged;
//!
//! impl GeneratedEndpoint for Forged {
//!     type Request = ();
//!     fn __concord_bind(self, _client: &InventoryApi) -> Self::Request {}
//!     fn __concord_plan(&self, _client: &InventoryApi) -> Result<RequestPlan, ApiError> {
//!         unreachable!()
//!     }
//! }
//! ```
//!
//! The core-owned native response driver is genuinely sealed as well:
//!
//! ```compile_fail,E0277
//! use concord::endpoint::ResponseAcquisition;
//! use concord::response::ResponseDriver;
//!
//! struct NativeDriver;
//!
//! impl ResponseDriver for NativeDriver {
//!     type Body = ();
//!     const ACQUISITION: ResponseAcquisition = ResponseAcquisition::Buffered;
//! }
//! ```
//!
//! Control: the same binding and planning calls compile for a genuine
//! generated intent, so the failures above are caused by the missing
//! generated-intent evidence and nothing else:
//!
//! ```
//! use concord_downstream_api::InventoryApi;
//!
//! fn bind(client: &InventoryApi) {
//!     let intent = InventoryApi::unbound().shop("s".to_owned()).gadget("g".to_owned());
//!     let _plan = client.plan(&intent);
//!     let _request = client.request(intent);
//! }
//! ```
//!
//! # Constant-bearing rate-key parts are typed, never `dyn` (V3-R13)
//!
//! `RateKeyPart` carries an associated `ENCODING_ID`, so it is not
//! dyn-compatible: the v2.1 emitter's `&dyn RateKeyPart` arrays cannot exist.
//! The downstream-generated `MeteredApi` key path (see
//! `tests/rate_keys.rs`) compiles only because generated code encodes each
//! typed value through an erased internal callback:
//!
//! ```compile_fail,E0038
//! use concord::rate_limit::RateKeyPart;
//! use concord_downstream_api::ShardKey;
//!
//! let parts: [&dyn RateKeyPart; 1] = [&ShardKey(7)];
//! ```
//!
//! Control: the same downstream key type encodes through the typed public
//! path, and the generated request that uses it compiles in this crate:
//!
//! ```
//! use concord::rate_limit::{DEFAULT_RATE_LIMIT_KEY_BYTES, encode_canonical};
//! use concord_downstream_api::{AccountId, MeteredApi, ShardKey};
//!
//! let part = encode_canonical(&ShardKey(7), DEFAULT_RATE_LIMIT_KEY_BYTES).expect("typed key");
//! assert_eq!(part.encoding_id(), "downstream.shard/1");
//! fn usage(client: &MeteredApi) {
//!     let _request = client.accounts(AccountId::new(1)).usage(ShardKey(7), 1_u32);
//! }
//! ```

use std::future::{IntoFuture, Ready, ready};

use concord::{
    ApiError, CallOptions, CancellationToken, ControlError, ControlSummary, RequestControls,
};
use concord_downstream_api::{Gadget, InventoryApi, Widget};

// ANCHOR: downstream_execute
/// The §23.2 shared execution helper, defined in the consuming crate.
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
// ANCHOR_END: downstream_execute

/// Fetches one widget and one gadget, two distinct downstream-generated
/// request types, through the same generic helper.
///
/// # Errors
///
/// The first failing request's error.
pub async fn widget_and_gadget(
    client: &InventoryApi,
    shop: &str,
    widget: &str,
    gadget: &str,
    options: &CallOptions,
) -> Result<(Widget, Gadget), ApiError> {
    let scope = client.shop(shop.to_owned());
    let widget = execute(
        scope.widget(widget.to_owned()).set_view("full".to_owned()),
        options,
    )
    .await?;
    let gadget = execute(scope.gadget(gadget.to_owned()), options).await?;
    Ok((widget, gadget))
}

/// An application type implementing the open [`RequestControls`] trait.
///
/// It resolves to a canned value and holds no client, native request,
/// credential or limiter. Implementing the trait grants it nothing: it
/// cannot be bound to or planned by a client (see the crate-level
/// `compile_fail` checks), and awaiting it never reaches a provider.
pub struct CannedRequest<T> {
    value: T,
    summary: ControlSummary,
    options_applied: u32,
}

impl<T> CannedRequest<T> {
    /// Wraps a canned value with a summary captured from elsewhere (a foreign
    /// type cannot construct a `ControlSummary` of its own).
    #[must_use]
    pub fn new(value: T, summary: ControlSummary) -> Self {
        Self {
            value,
            summary,
            options_applied: 0,
        }
    }

    /// How many times `with_options` was applied.
    #[must_use]
    pub fn options_applied(&self) -> u32 {
        self.options_applied
    }
}

impl<T> RequestControls for CannedRequest<T> {
    fn with_options(mut self, _options: &CallOptions) -> Self {
        self.options_applied = self.options_applied.saturating_add(1);
        self
    }

    fn control_summary(&self) -> ControlSummary {
        self.summary.clone()
    }

    fn cancel_with(self, _token: CancellationToken) -> Result<Self, ControlError> {
        Ok(self)
    }
}

impl<T> IntoFuture for CannedRequest<T> {
    type Output = Result<T, ApiError>;
    type IntoFuture = Ready<Result<T, ApiError>>;

    fn into_future(self) -> Self::IntoFuture {
        ready(Ok(self.value))
    }
}
