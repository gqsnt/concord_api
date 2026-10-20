//! Explicit endpoint-backed credential storage terminal (AUTH-03, AUTH-05,
//! §11.4).
//!
//! `store_as_<credential>()` wraps the **same** bound source-endpoint request
//! and runs it through the **same** managed executor as an ordinary `.await`;
//! there is no alternative authentication or native pipeline. The only
//! additions are the installation intent reserved on first poll and the
//! atomic, intent-checked local installation after complete decoding.

use std::fmt;
use std::future::IntoFuture;
use std::sync::Arc;
use std::time::Duration;

use crate::auth::{CredentialMaterial, IntentTicket, ReleasedMaterial, SlotSelector, StoreOutcome};
use crate::controls::{
    CallOptions, CancellationToken, ControlError, ControlSummary, RequestControls,
};
use crate::error::{ApiError, ConfigurationErrorKind, FailureStage};
use crate::hooks::BoxFuture;
use crate::request::{
    EndpointSpec, OperationPreparation, Request, RequestOptions, precheck_controls,
    prepare_operation,
};
use crate::response::DeclaredResponse;

use super::{
    ExecutionContext, Finalization, complete_preparation_failure, direct_attempt_ceiling,
    execute_finalized_operation,
};

/// An inert terminal that executes one credential source endpoint and stores
/// its decoded material under strict latest-started explicit intent.
///
/// Resolves to `Result<StoreOutcome, ApiError>` through [`IntoFuture`].
///
/// **Laziness.** Construction and control changes have no effects. The first
/// poll reserves a new installation intent for the credential store (this
/// supersedes every earlier pending intent but does not clear currently
/// installed material) and then executes the source endpoint through the
/// ordinary managed executor, with the same authentication, limits, rate
/// admission, retries, hooks, deadline and caps as a normal `.await`. An
/// unpolled `StoreRequest` never reserves an intent, so a request created
/// before a manual `clear` and first polled after it is a new post-clear
/// intent.
///
/// **Installation.** After complete successful decoding the material is
/// installed atomically, as a newly allocated [`CredentialVersion`](crate::contracts::CredentialVersion),
/// only if this operation's intent is still the latest-started intent;
/// otherwise the material is dropped and the outcome is
/// [`StoreOutcome::Superseded`]. The material is never returned through this
/// terminal. Common caps (timeout cap, selected timeout, attached
/// cancellation signals) apply through local installation: they are decided
/// immediately before installing, after the material's application-defined
/// [`expires_at`](CredentialMaterial::expires_at) has been read, so a delay or
/// cancellation that happens before installation prevents it. Because the
/// caps bound the whole storage operation, a deadline or cancellation decided
/// there is the result even when the intent was also superseded (neither
/// installs anything).
///
/// **Failure and cancellation.** A failed or cancelled operation (including
/// dropping the future) never rolls the intent back and never revives an
/// earlier pending intent; material installed before these intents stays.
/// A successful remote response followed by a failed local installation
/// (deadline, cancellation or a fail-closed credential store) is an
/// [`ApiError`] with [`FailureStage::CredentialInstall`],
/// [`DispatchKnowledge::ResponseObserved`](crate::contracts::DispatchKnowledge::ResponseObserved),
/// the received status ([`ApiError::last_response_status`]), the operation's
/// journal facts and the validated operational endpoint ID of the executed
/// request.
///
/// **Controls.** A cancellation signal already cancelled at first poll, or a
/// zero send cap, fails before the installation intent is reserved: an
/// operation that can never dispatch never supersedes earlier pending
/// intents.
///
/// **Observation.** Local installation is part of the storage operation's
/// result. The operation's single `OperationFinished` observation is emitted
/// only after that complete result is fixed (EXEC-04): after installation,
/// supersession or the installation failure. It carries the operation's
/// identity and exchange facts (final status, attempts) and reports
/// `Success` for both `Ok` outcomes — `Installed` and `Superseded` are
/// successful storage results that the returned [`StoreOutcome`]
/// distinguishes — and the failure's category otherwise. A slow final
/// observer therefore delays delivery but cannot change the outcome.
/// Credential material the operation no longer holds — decoded material that
/// is not installed, installed material that the installation replaced (once
/// nothing else still uses it), and the credential store itself when this
/// operation holds its last reference — is dropped only after that
/// observation, so a panicking application `Drop` cannot misreport the fixed
/// result; its panic then propagates to the caller of the storage future.
///
/// **Sensitivity.** The produced material is credential data; it is moved
/// straight into the credential store (or dropped) and never formatted.
pub struct StoreRequest {
    inner: Box<dyn ErasedStore>,
}

trait ErasedStore: Send {
    fn options(&self) -> &RequestOptions;
    fn options_mut(&mut self) -> &mut RequestOptions;
    fn declared_timeout(&self) -> Option<Duration>;
    fn start(self: Box<Self>) -> BoxFuture<'static, Result<StoreOutcome, ApiError>>;
}

struct TypedStore<E, M> {
    request: Request<E>,
    selector: Arc<SlotSelector<M>>,
}

impl<E, M> ErasedStore for TypedStore<E, M>
where
    E: EndpointSpec<Output = M>,
    M: CredentialMaterial,
{
    fn options(&self) -> &RequestOptions {
        &self.request.options
    }

    fn options_mut(&mut self) -> &mut RequestOptions {
        &mut self.request.options
    }

    fn declared_timeout(&self) -> Option<Duration> {
        self.request
            .endpoint
            .request_meta()
            .resolve_inherited_timeout(self.request.core.default_runtime_endpoint_timeout())
    }

    fn start(self: Box<Self>) -> BoxFuture<'static, Result<StoreOutcome, ApiError>> {
        let Self { request, selector } = *self;
        Box::pin(execute_store(request, selector))
    }
}

/// Wraps one bound source-endpoint request and its credential store.
pub(crate) fn store_request<E, M>(
    request: Request<E>,
    selector: Arc<SlotSelector<M>>,
) -> StoreRequest
where
    E: EndpointSpec<Output = M>,
    M: CredentialMaterial,
{
    StoreRequest {
        inner: Box::new(TypedStore { request, selector }),
    }
}

impl StoreRequest {
    /// Replaces the selected finite timeout preference. Caps are retained.
    pub fn timeout(mut self, duration: Duration) -> Result<Self, ControlError> {
        self.inner.options_mut().select_timeout(duration)?;
        Ok(self)
    }

    /// Restores the declared timeout. Caps are retained.
    #[must_use]
    pub fn reset_timeout(mut self) -> Self {
        self.inner.options_mut().reset_selected_timeout();
        self
    }

    /// Attaches one more cancellation signal (deduplicated by identity).
    pub fn cancel_with(mut self, token: CancellationToken) -> Result<Self, ControlError> {
        self.inner.options_mut().add_cancellation(token)?;
        Ok(self)
    }

    /// Intersects additional caps; caps only narrow execution.
    #[must_use]
    pub fn with_options(mut self, options: &CallOptions) -> Self {
        self.inner.options_mut().apply_call_options(options);
        self
    }

    /// Returns the value-free effective control summary.
    #[must_use]
    pub fn control_summary(&self) -> ControlSummary {
        self.inner
            .options()
            .control_summary(self.inner.declared_timeout())
    }
}

impl RequestControls for StoreRequest {
    fn with_options(self, options: &CallOptions) -> Self {
        Self::with_options(self, options)
    }

    fn control_summary(&self) -> ControlSummary {
        Self::control_summary(self)
    }

    fn cancel_with(self, token: CancellationToken) -> Result<Self, ControlError> {
        Self::cancel_with(self, token)
    }
}

impl IntoFuture for StoreRequest {
    type Output = Result<StoreOutcome, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        self.inner.start()
    }
}

impl fmt::Debug for StoreRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoreRequest")
            .field("controls", &self.control_summary())
            .finish_non_exhaustive()
    }
}

async fn execute_store<E, M>(
    request: Request<E>,
    selector: Arc<SlotSelector<M>>,
) -> Result<StoreOutcome, ApiError>
where
    E: EndpointSpec<Output = M>,
    M: CredentialMaterial,
{
    let meta = request.endpoint.request_meta();
    // The credential store must belong to the client executing its source.
    if !selector
        .context()
        .same_as(request.core.credential_context())
    {
        return Err(ApiError::configuration(
            meta.endpoint_id().clone(),
            meta.method().clone(),
            ConfigurationErrorKind::InvalidAuthentication,
        )
        .before_operation());
    }
    // Phase 0 control validation precedes the intent reservation.
    precheck_controls(&request.options, &meta)?;

    // Phase 0: the first poll reserves a new installation intent before the
    // source endpoint executes.
    let ticket = selector.reserve_store_intent().map_err(|error| {
        ApiError::authentication(meta.endpoint_id().clone(), meta.method().clone(), error)
            .before_operation()
    })?;

    let prepared = match prepare_operation(request).await? {
        OperationPreparation::Ready(prepared) => prepared,
        OperationPreparation::Failed(failure) => {
            return complete_preparation_failure(failure);
        }
    };
    let attempt_ceiling = direct_attempt_ceiling(prepared.head.call_caps());

    // Credential material the finalization no longer holds. It is released
    // only after the storage result is fixed and finally observed, so its
    // application-defined `Drop` (which may panic) runs outside the result
    // decision and cannot misreport the result. For the same reason the
    // finalization only borrows the credential store: this operation may hold
    // its last reference, and releasing the store releases its material.
    let mut spent: Option<SpentMaterial<M>> = None;
    let spent_slot = &mut spent;
    let store: &SlotSelector<M> = &selector;
    // §11.4 / EXEC-04: local installation is the finalization step of the
    // source operation. It runs inside the operation's journal and
    // cancellation scope, after complete decoding and before the storage
    // result is fixed; the single final observation follows it.
    let executed = execute_finalized_operation::<E, DeclaredResponse<E::Response>, _, _>(
        prepared,
        ExecutionContext::DIRECT,
        attempt_ceiling,
        move |response, finalization| {
            let (result, material) =
                install_material(store, ticket, response.into_value(), finalization);
            *spent_slot = Some(material);
            result
        },
    )
    .await;
    // The result is fixed and observed; a panicking `Drop` now propagates to
    // the caller without having changed either.
    if let Some(spent) = spent {
        spent.release();
    }
    drop(selector);
    executed.result
}

/// Credential material a storage finalization no longer holds. It is kept
/// until the storage result is fixed and finally observed, and only then
/// released.
enum SpentMaterial<M> {
    /// Decoded material the common caps kept from being installed.
    NotInstalled(M),
    /// Material released by the installation decision: the decoded value
    /// when it is not installed, or the installed value it replaced.
    Released(ReleasedMaterial<M>),
}

impl<M> SpentMaterial<M> {
    /// Runs the application-defined `Drop` of the material.
    fn release(self) {
        match self {
            Self::NotInstalled(material) => drop(material),
            Self::Released(material) => drop(material),
        }
    }
}

/// Installs the decoded material of a storage operation (AUTH-03, §11.4).
///
/// Common caps apply through local installation: cancellation and the
/// deadline are decided at [`FailureStage::CredentialInstall`] before and
/// after the material's application-defined expiry read (a synchronous
/// extension call, §13.3), immediately before the atomic, intent-checked
/// installation, whose lock-guarded decision runs no application code. The
/// caps are decided first because they bound the whole storage operation: a
/// deadline or cancellation observed here wins even over an intent that was
/// also superseded, and neither outcome installs anything or rolls the intent
/// back (§11.3). A failure keeps the journal facts of the completed exchange
/// (ResponseObserved, status, attempts, counts) and the validated operational
/// endpoint ID (AUTH-05). Material that is not installed, and installed
/// material it replaces, are never returned: they are handed back as
/// [`SpentMaterial`] for the caller to release after the storage result is
/// fixed and finally observed.
fn install_material<M>(
    selector: &SlotSelector<M>,
    ticket: IntentTicket<M>,
    material: M,
    finalization: &mut Finalization<'_>,
) -> (Result<StoreOutcome, ApiError>, SpentMaterial<M>)
where
    M: CredentialMaterial,
{
    if let Err(error) = finalization.decide_caps(FailureStage::CredentialInstall) {
        return (Err(error), SpentMaterial::NotInstalled(material));
    }
    let expires_at = material.expires_at();
    if let Err(error) = finalization.decide_caps(FailureStage::CredentialInstall) {
        return (Err(error), SpentMaterial::NotInstalled(material));
    }
    let (outcome, released) = selector.install_with_expiry(ticket, material, expires_at);
    let outcome = outcome.map_err(|error| {
        ApiError::authentication(
            finalization.endpoint_id().clone(),
            finalization.method().clone(),
            error,
        )
    });
    (outcome, SpentMaterial::Released(released))
}
