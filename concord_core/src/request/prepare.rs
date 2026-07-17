use super::{BodyMetadata, EndpointSpec, HeadBuilder, PreparedHead, Request, RequestMeta};
use crate::client::ClientCore;
use crate::error::{ApiError, ConfigurationErrorKind, ErrorCategory};
use crate::operation::{OperationDeadline, OperationId};

#[allow(dead_code, reason = "consumed by the PR 2.3 executor")]
pub(crate) struct PreparedOperation<E> {
    pub(crate) core: ClientCore,
    pub(crate) endpoint: E,
    pub(crate) operation_id: OperationId,
    pub(crate) deadline: OperationDeadline,
    pub(crate) head: PreparedHead,
    pub(crate) body_metadata: BodyMetadata,
}

#[allow(dead_code, reason = "consumed by the PR 2.3 executor")]
pub(crate) async fn prepare_operation<E>(
    request: Request<E>,
) -> Result<PreparedOperation<E>, ApiError>
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
    let timeout = options.resolve_timeout(meta.inherited_timeout());
    let deadline = OperationDeadline::new(started, timeout).map_err(|_| {
        ApiError::configuration(
            meta.endpoint(),
            meta.method().clone(),
            ConfigurationErrorKind::InvalidValue,
        )
    })?;

    let builder = HeadBuilder::new(
        core.base_url(),
        meta.clone(),
        &options,
        core.default_debug(),
    );
    let (head, body_metadata) = endpoint.prepare_head_and_body_metadata(builder)?;
    ensure_matching_meta(&meta, &head)?;
    core.preflight_url(meta.endpoint(), meta.method(), head.logical_url())?;
    Ok(PreparedOperation {
        core,
        endpoint,
        operation_id,
        deadline,
        head,
        body_metadata,
    })
}

#[allow(dead_code, reason = "consumed by the PR 2.3 executor")]
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
