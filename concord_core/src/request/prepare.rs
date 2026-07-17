use std::sync::Arc;
use std::time::Duration;

use super::{
    BodyMetadata, EndpointSpec, HeadBuilder, PreparedHead, Request, RequestMeta, ResponseHandler,
};
use crate::client::ClientCore;
use crate::error::{ApiError, ConfigurationErrorKind, ErrorCategory};
use crate::hooks::DebugLevel;
use crate::operation::{OperationDeadline, OperationId};

pub(crate) struct PreparedOperation<E> {
    pub(crate) core: ClientCore,
    pub(crate) endpoint: E,
    pub(crate) operation_id: OperationId,
    pub(crate) deadline: OperationDeadline,
    pub(crate) head: PreparedHead,
    pub(crate) body_metadata: BodyMetadata,
}

pub(crate) struct PreparationFailure {
    pub(crate) core: ClientCore,
    pub(crate) operation_id: OperationId,
    pub(crate) meta: RequestMeta,
    pub(crate) safe_url: Arc<str>,
    pub(crate) debug: DebugLevel,
    pub(crate) elapsed: Duration,
    pub(crate) error: ApiError,
}

pub(crate) enum OperationPreparation<E> {
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
    let started = tokio::time::Instant::now();
    let meta = endpoint.request_meta();
    let operation_id = core.allocate_operation_id(meta.endpoint(), meta.method())?;
    let safe_base_url = core.safe_base_url();
    let debug = options.resolve_debug(core.default_debug());
    let timeout = options.resolve_timeout(meta.inherited_timeout());
    let deadline = match OperationDeadline::new(started, timeout) {
        Ok(deadline) => deadline,
        Err(_) => {
            let error = ApiError::configuration(
                meta.endpoint(),
                meta.method().clone(),
                ConfigurationErrorKind::InvalidValue,
            );
            return Ok(OperationPreparation::Failed(failure(
                core,
                operation_id,
                meta,
                safe_base_url,
                debug,
                started,
                error,
            )));
        }
    };
    if deadline.check().is_err() {
        let error = timeout_error(meta.endpoint(), meta.method());
        return Ok(OperationPreparation::Failed(failure(
            core,
            operation_id,
            meta,
            safe_base_url,
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
    );
    let (mut head, body_metadata) = match endpoint.prepare_head_and_body_metadata(builder) {
        Ok(prepared) => prepared,
        Err(error) => {
            return Ok(OperationPreparation::Failed(failure(
                core,
                operation_id,
                meta,
                safe_base_url,
                debug,
                started,
                error,
            )));
        }
    };
    if let Err(error) = head.apply_media_headers(E::Response::ACCEPT, body_metadata.content_type())
    {
        return Ok(OperationPreparation::Failed(failure(
            core,
            operation_id,
            meta,
            Arc::from(head.safe_url()),
            debug,
            started,
            error,
        )));
    }
    if let Err(error) = ensure_matching_meta(&meta, &head) {
        return Ok(OperationPreparation::Failed(failure(
            core,
            operation_id,
            meta,
            Arc::from(head.safe_url()),
            debug,
            started,
            error,
        )));
    }
    if let Err(error) = core.preflight_url(meta.endpoint(), meta.method(), head.logical_url()) {
        return Ok(OperationPreparation::Failed(failure(
            core,
            operation_id,
            meta,
            Arc::from(head.safe_url()),
            debug,
            started,
            error,
        )));
    }
    if deadline.check().is_err() {
        let error = timeout_error(meta.endpoint(), meta.method());
        return Ok(OperationPreparation::Failed(failure(
            core,
            operation_id,
            meta,
            Arc::from(head.safe_url()),
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
        body_metadata,
    }))
}

fn failure(
    core: ClientCore,
    operation_id: OperationId,
    meta: RequestMeta,
    safe_url: Arc<str>,
    debug: DebugLevel,
    started: tokio::time::Instant,
    error: ApiError,
) -> PreparationFailure {
    PreparationFailure {
        core,
        operation_id,
        meta,
        safe_url,
        debug,
        elapsed: tokio::time::Instant::now().saturating_duration_since(started),
        error,
    }
}

fn ensure_matching_meta(expected: &RequestMeta, head: &PreparedHead) -> Result<(), ApiError> {
    if expected.endpoint() != head.endpoint() || expected.method() != head.method() {
        return Err(ApiError::classified(
            expected.endpoint(),
            expected.method().clone(),
            ErrorCategory::Internal,
        ));
    }
    Ok(())
}

fn timeout_error(endpoint: &'static str, method: &http::Method) -> ApiError {
    ApiError::classified(endpoint, method.clone(), ErrorCategory::Timeout)
}
