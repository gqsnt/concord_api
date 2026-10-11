use super::{EndpointSpec, HeadBuilder, PreparedHead, Request, RequestMeta, RequestOptions};
use crate::client::ClientCore;
use crate::controls::RequestControlState;
use crate::endpoint::AbsentStatus;
use crate::error::{
    ApiError, ConfigurationErrorKind, DispatchKnowledge, ErrorCategory, FailureStage,
};
use crate::hooks::DebugLevel;
use crate::operation::{OperationDeadline, OperationId};
use crate::response::{ExpectedAbsence, ResponseRepresentation};

pub(crate) struct PreparedOperation<E>
where
    E: EndpointSpec,
{
    pub(crate) core: ClientCore,
    pub(crate) endpoint: E,
    pub(crate) operation_id: OperationId,
    pub(crate) deadline: OperationDeadline,
    pub(crate) head: PreparedHead,
    pub(crate) absence: Option<ExpectedAbsence<E::Output, E::Response>>,
    /// Aggregate traversal accounting; `None` for direct operations.
    pub(crate) meter: Option<std::sync::Arc<crate::pagination::TraversalMeter>>,
    /// Caller cancellation signals raced by the executor. `None` for a
    /// traversal page, whose cancellation the traversal owns.
    pub(crate) controls: Option<RequestControlState>,
}

pub(crate) struct PreparedPaginationPage<E> {
    pub(crate) core: ClientCore,
    pub(crate) endpoint: E,
    pub(crate) head: PreparedHead,
}

pub(crate) struct PreparationFailure {
    pub(crate) core: ClientCore,
    pub(crate) operation_id: OperationId,
    pub(crate) meta: RequestMeta,
    pub(crate) target: crate::hooks::RequestTarget,
    pub(crate) debug: DebugLevel,
    pub(crate) started: tokio::time::Instant,
    pub(crate) error: ApiError,
}

// Every request creates this value. Keeping both bounded outcomes inline avoids
// an allocation on the successful preparation path.
#[allow(clippy::large_enum_variant)]
pub(crate) enum OperationPreparation<E>
where
    E: EndpointSpec,
{
    Ready(PreparedOperation<E>),
    Failed(PreparationFailure),
}

pub(crate) async fn prepare_operation<E>(
    request: Request<E>,
) -> Result<OperationPreparation<E>, ApiError>
where
    E: EndpointSpec,
{
    let Request {
        core,
        endpoint,
        options,
    } = request;
    let meta = endpoint.request_meta();
    // Phase 0: controls and cancellation are validated before identity
    // allocation or any preparation (the zero-send fast path).
    precheck_controls(&options, &meta)?;
    let started = OperationDeadline::now();
    let operation_id = core
        .allocate_operation_id(meta.endpoint_id(), meta.method())
        .map_err(ApiError::before_operation)?;
    let base_target = crate::hooks::RequestTarget::from_url(core.base_url(), 0);
    let debug = options.resolve_debug(core.default_debug());
    let timeout = options
        .resolve_timeout(meta.resolve_inherited_timeout(core.default_runtime_endpoint_timeout()));
    let deadline = match OperationDeadline::new(started, timeout) {
        Ok(deadline) => deadline,
        Err(_) => {
            let error = ApiError::configuration(
                meta.endpoint_id().clone(),
                meta.method().clone(),
                ConfigurationErrorKind::InvalidValue,
            );
            return Ok(OperationPreparation::Failed(failure(
                core,
                operation_id,
                meta,
                base_target,
                debug,
                started,
                error,
            )));
        }
    };
    if deadline.check().is_err() {
        let error = timeout_error(meta.endpoint_id(), meta.method());
        return Ok(OperationPreparation::Failed(failure(
            core,
            operation_id,
            meta,
            base_target,
            debug,
            started,
            error,
        )));
    }

    let builder = HeadBuilder::new(
        core.base_url(),
        meta.clone(),
        &options,
        core.default_debug(),
        core.default_runtime_endpoint_timeout(),
    );
    let mut head = match endpoint.prepare_head(builder) {
        Ok(prepared) => prepared,
        Err(error) => {
            return Ok(OperationPreparation::Failed(failure(
                core,
                operation_id,
                meta,
                base_target,
                debug,
                started,
                error,
            )));
        }
    };
    if let Err(error) = head.validate_auth_context(core.credential_context()) {
        return Ok(OperationPreparation::Failed(failure(
            core,
            operation_id,
            meta,
            head.request_target(),
            debug,
            started,
            error,
        )));
    }
    let absence = endpoint.expected_absence();
    if let Err(error) = validate_prepared_head::<E>(
        &core,
        &meta,
        &mut head,
        absence.as_ref().map(ExpectedAbsence::statuses),
    ) {
        return Ok(OperationPreparation::Failed(failure(
            core,
            operation_id,
            meta,
            head.request_target(),
            debug,
            started,
            error,
        )));
    }
    if deadline.check().is_err() {
        let error = timeout_error(meta.endpoint_id(), meta.method());
        return Ok(OperationPreparation::Failed(failure(
            core,
            operation_id,
            meta,
            head.request_target(),
            debug,
            started,
            error,
        )));
    }
    Ok(OperationPreparation::Ready(PreparedOperation {
        core,
        endpoint,
        operation_id,
        deadline,
        head,
        absence,
        meter: None,
        controls: Some(options.controls().clone()),
    }))
}

/// Phase-0 control validation at first poll, before operation identity,
/// head shaping, body preparation, credential preflight, installation-intent
/// reservation or any other effect.
///
/// * A cancellation signal already observed prevents the operation.
/// * A zero send cap prevents application dispatch without running any
///   unrelated preparation.
///
/// The error carries no operation identity, `NotDispatched` and
/// `BeforeDispatch`; no observation is emitted because no operation started.
pub(crate) fn precheck_controls(
    options: &RequestOptions,
    meta: &RequestMeta,
) -> Result<(), ApiError> {
    let category = if options.controls().is_cancelled() {
        ErrorCategory::Cancelled
    } else if options.controls().caps().send_cap() == Some(0) {
        ErrorCategory::SendCapExhausted
    } else {
        return Ok(());
    };
    Err(
        ApiError::classified(meta.endpoint_id().clone(), meta.method().clone(), category)
            .with_stage(
                FailureStage::BeforeDispatch,
                DispatchKnowledge::NotDispatched,
            )
            .before_operation(),
    )
}

pub(crate) fn prepare_pagination_page<E>(
    request: Request<E>,
) -> Result<PreparedPaginationPage<E>, ApiError>
where
    E: EndpointSpec,
{
    let Request {
        core,
        endpoint,
        options,
    } = request;
    let meta = endpoint.request_meta();
    let builder = HeadBuilder::new(
        core.base_url(),
        meta.clone(),
        &options,
        core.default_debug(),
        core.default_runtime_endpoint_timeout(),
    );
    let mut head = endpoint.prepare_head(builder)?;
    head.validate_auth_context(core.credential_context())?;
    // Head-only absence cannot combine with pagination: a page must be a
    // complete accepted page, never a fabricated absent value.
    if endpoint.expected_absence().is_some() {
        return Err(ApiError::configuration(
            meta.endpoint_id().clone(),
            meta.method().clone(),
            ConfigurationErrorKind::InvalidValue,
        ));
    }
    validate_prepared_head::<E>(&core, &meta, &mut head, None)?;
    Ok(PreparedPaginationPage {
        core,
        endpoint,
        head,
    })
}

fn validate_prepared_head<E>(
    core: &ClientCore,
    meta: &RequestMeta,
    head: &mut PreparedHead,
    absence: Option<&AbsentStatus>,
) -> Result<(), ApiError>
where
    E: EndpointSpec,
{
    head.validate_rate_limit_key_limit(core.rate_limit_key_limit())?;
    // The response representation's static Accept constant is placed here;
    // the body-owned Content-Type is placed only after the one prepared body
    // exists (phase 3), from that prepared body.
    let accept = <E::Response as ResponseRepresentation<E::Output>>::ACCEPT;
    head.apply_media_headers(accept, None, false)?;
    if let Some(absence) = absence {
        validate_absence(meta, head, absence)?;
    }
    head.validate_auth()?;
    ensure_matching_meta(meta, head)?;
    if head.auth().is_empty() {
        core.preflight_url(meta.endpoint_id(), meta.method(), head.logical_url())?;
    } else {
        core.preflight_protected_url(meta.endpoint_id(), meta.method(), head.logical_url())?;
    }
    Ok(())
}

/// Keeps a declared absence set disjoint from accepted statuses and from
/// effective authentication challenges.
fn validate_absence(
    meta: &RequestMeta,
    head: &PreparedHead,
    absence: &AbsentStatus,
) -> Result<(), ApiError> {
    let overlaps_challenge = absence
        .as_slice()
        .iter()
        .any(|status| head.auth().classifies_challenge(*status));
    if absence.overlaps(head.accepted_status()) || overlaps_challenge {
        return Err(ApiError::configuration(
            meta.endpoint_id().clone(),
            meta.method().clone(),
            ConfigurationErrorKind::InvalidValue,
        ));
    }
    Ok(())
}

fn failure(
    core: ClientCore,
    operation_id: OperationId,
    meta: RequestMeta,
    target: crate::hooks::RequestTarget,
    debug: DebugLevel,
    started: tokio::time::Instant,
    error: ApiError,
) -> PreparationFailure {
    PreparationFailure {
        core,
        operation_id,
        meta,
        target,
        debug,
        started,
        error,
    }
}

fn ensure_matching_meta(expected: &RequestMeta, head: &PreparedHead) -> Result<(), ApiError> {
    if expected.endpoint() != head.endpoint() || expected.method() != head.method() {
        return Err(ApiError::classified(
            expected.endpoint_id().clone(),
            expected.method().clone(),
            ErrorCategory::Internal,
        ));
    }
    Ok(())
}

fn timeout_error(endpoint: &crate::endpoint::EndpointId, method: &http::Method) -> ApiError {
    ApiError::classified(endpoint.clone(), method.clone(), ErrorCategory::Timeout)
}
