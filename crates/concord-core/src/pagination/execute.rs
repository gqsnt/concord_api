use crate::body::PageRequestRepresentation;
use crate::error::{ApiError, PaginationErrorKind};
use crate::hooks::{BoxFuture, CompletionOutcome, OperationEvent, PaginationFinished};
use crate::operation::OperationDeadline;
use crate::request::{EndpointSpec, Request, prepare_pagination_page};
use crate::response::PageResponseRepresentation;

use super::identity::CanonicalIdentity;
use super::limits::CycleDetector;
use super::{
    PageDecisionLimits, PageLimit, PageResponse, PageStep, PaginatedEndpoint, PaginationController,
    PaginationError,
};

type DecodedPage<E> = <E as EndpointSpec>::Output;
type PageItem<E> =
    <<E as PaginatedEndpoint>::Controller as PaginationController<DecodedPage<E>>>::Item;
type PaginationResult<E> = Result<Vec<PageItem<E>>, ApiError>;

pub(super) fn paginate_with<E>(
    request: Request<E>,
    limit: PageLimit,
) -> BoxFuture<'static, PaginationResult<E>>
where
    E: PaginatedEndpoint,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
    E::Response: PageResponseRepresentation<E::Output>,
    DecodedPage<E>: Send + 'static,
    PageItem<E>: Send + 'static,
{
    Box::pin(async move {
        let Request {
            core,
            endpoint,
            options,
        } = request;
        if matches!(limit, PageLimit::Prefix(0)) {
            return Ok(Vec::new());
        }
        let meta = endpoint.request_meta();
        let aggregate_started = OperationDeadline::now();
        let timeout = options.resolve_timeout(
            meta.resolve_inherited_timeout(core.default_runtime_endpoint_timeout()),
        );
        let aggregate_deadline =
            OperationDeadline::new(aggregate_started, timeout).map_err(|_| {
                ApiError::configuration(
                    meta.endpoint_id().clone(),
                    meta.method().clone(),
                    crate::error::ConfigurationErrorKind::InvalidValue,
                )
            })?;
        let aggregate_operation_id =
            core.allocate_operation_id(meta.endpoint_id(), meta.method())?;
        let max_pages = core.pagination_max_pages().get();
        let max_items = core.pagination_max_items().get();
        let max_response_body_bytes = core.pagination_total_response_body_limit().get();
        let identity_limit = core.pagination_identity_limit().get();

        let mut pages_completed = 0_u32;
        let mut items_collected = 0_usize;
        let mut response_body_bytes_collected = 0_usize;
        let mut attempts_started = 0_u32;
        let mut policy_retries = 0_u32;
        let mut recovered_authentication = false;
        let mut final_status = None;

        let requested_pages = match limit {
            PageLimit::Prefix(pages) => pages,
            PageLimit::CompleteWithin(pages) => pages.get(),
        };

        let result = if requested_pages > max_pages {
            Err(pagination_api_error(
                &meta,
                PaginationError::new(PaginationErrorKind::PageCeilingExceeded),
            ))
        } else {
            match E::initial_controller(&endpoint) {
                Ok(controller) => {
                    run_pages(
                        &core,
                        endpoint,
                        options,
                        limit,
                        controller,
                        aggregate_operation_id,
                        aggregate_deadline,
                        max_pages,
                        max_items,
                        max_response_body_bytes,
                        identity_limit,
                        &mut pages_completed,
                        &mut items_collected,
                        &mut response_body_bytes_collected,
                        &mut attempts_started,
                        &mut policy_retries,
                        &mut recovered_authentication,
                        &mut final_status,
                    )
                    .await
                }
                Err(error) => Err(pagination_api_error(&meta, error)),
            }
        };

        let elapsed = OperationDeadline::now().saturating_duration_since(aggregate_started);
        let outcome = match &result {
            Ok(_) => CompletionOutcome::Success,
            Err(error) => CompletionOutcome::from_error_category(error.category()),
        };
        let event = PaginationFinished {
            operation_id: aggregate_operation_id,
            endpoint: meta.endpoint(),
            method: meta.method(),
            pages_completed,
            items_collected,
            attempts_started,
            policy_retries,
            recovered_authentication,
            elapsed,
            final_status,
            outcome,
        };
        core.debug_dispatcher(options.resolve_debug(core.default_debug()))
            .pagination_completion(&event);
        crate::hooks::observe(
            core.operation_observer().as_ref(),
            OperationEvent::PaginationFinished(event),
        );
        result
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "the request-local page state mirrors the normative aggregate state"
)]
async fn run_pages<E>(
    core: &crate::client::ClientCore,
    original_endpoint: E,
    options: crate::request::RequestOptions,
    limit: PageLimit,
    mut controller: E::Controller,
    aggregate_operation_id: crate::operation::OperationId,
    aggregate_deadline: OperationDeadline,
    max_pages: u32,
    max_items: usize,
    max_response_body_bytes: usize,
    identity_limit: usize,
    pages_completed: &mut u32,
    items_collected: &mut usize,
    response_body_bytes_collected: &mut usize,
    attempts_started: &mut u32,
    policy_retries: &mut u32,
    recovered_authentication: &mut bool,
    final_status: &mut Option<http::StatusCode>,
) -> PaginationResult<E>
where
    E: PaginatedEndpoint,
    E::Input: Clone,
    E::Body: PageRequestRepresentation<E::Input>,
    E::Response: PageResponseRepresentation<E::Output>,
    DecodedPage<E>: Send + 'static,
    PageItem<E>: Send + 'static,
{
    let meta = original_endpoint.request_meta();
    let mut items = Vec::new();
    let mut identities = CycleDetector::default();
    let mut page_index = 0_u32;

    loop {
        aggregate_deadline
            .check()
            .map_err(|_| aggregate_timeout(&meta))?;
        controller
            .validate()
            .map_err(|error| pagination_api_error(&meta, error))?;
        aggregate_deadline
            .check()
            .map_err(|_| aggregate_timeout(&meta))?;
        let mut endpoint = original_endpoint.clone();
        E::bind_controller(&mut endpoint, &controller)
            .map_err(|error| pagination_api_error(&meta, error))?;
        aggregate_deadline
            .check()
            .map_err(|_| aggregate_timeout(&meta))?;

        let page = prepare_pagination_page(Request {
            core: core.clone(),
            endpoint,
            options,
        })?;

        let remaining_identity_bytes = identities
            .remaining_identity_bytes(identity_limit)
            .map_err(|error| pagination_api_error(&meta, error))?;
        let identity = CanonicalIdentity::from_request_head_with_limit(
            page.head.endpoint(),
            page.head.method(),
            page.head.logical_url(),
            page.head.public_headers(),
            remaining_identity_bytes,
        )
        .map_err(|error| pagination_api_error(&meta, error))?;
        let digest = identities
            .preflight(&identity, identity_limit)
            .map_err(|error| pagination_api_error(&meta, error))?;
        identities
            .retain(identity, digest)
            .map_err(|error| pagination_api_error(&meta, error))?;

        let executed = crate::execute::execute_pagination_page(
            page,
            aggregate_operation_id,
            page_index,
            aggregate_deadline,
            crate::execute::MAX_VISIBLE_ATTEMPTS_PER_OPERATION
                .checked_sub(*attempts_started)
                .filter(|remaining| *remaining > 0)
                .ok_or_else(|| {
                    pagination_kind_error(&meta, PaginationErrorKind::AttemptCeilingExceeded)
                })?,
        )
        .await?;
        let page_operation_id = executed.operation_id;
        let page_response_body_bytes = executed.response_body_bytes;
        let next_attempts_started = (*attempts_started)
            .checked_add(executed.attempts_started)
            .ok_or_else(|| pagination_kind_error(&meta, PaginationErrorKind::Overflow))?;
        let next_policy_retries = (*policy_retries)
            .checked_add(executed.policy_retries)
            .ok_or_else(|| pagination_kind_error(&meta, PaginationErrorKind::Overflow))?;
        *attempts_started = next_attempts_started;
        *policy_retries = next_policy_retries;
        *recovered_authentication |= executed.recovered_authentication;
        if executed.final_status.is_some() {
            *final_status = executed.final_status;
        }
        let response = executed.result?;
        let next_response_body_bytes = (*response_body_bytes_collected)
            .checked_add(page_response_body_bytes)
            .ok_or_else(|| pagination_kind_error(&meta, PaginationErrorKind::Overflow))?;
        if next_response_body_bytes > max_response_body_bytes {
            return Err(pagination_kind_error(
                &meta,
                PaginationErrorKind::CollectedResponseByteCeilingExceeded,
            ));
        }
        let page = PageResponse::from_accepted(
            response,
            E::Controller::DESCRIPTION,
            aggregate_operation_id,
            page_operation_id,
            page_index,
        )
        .map_err(|error| pagination_api_error(&meta, error))?;

        let accepted_page_count = (*pages_completed)
            .checked_add(1)
            .ok_or_else(|| pagination_kind_error(&meta, PaginationErrorKind::Overflow))?;
        if accepted_page_count > max_pages {
            return Err(pagination_kind_error(
                &meta,
                PaginationErrorKind::PageCeilingExceeded,
            ));
        }

        let final_soft_page =
            matches!(limit, PageLimit::Prefix(maximum) if accepted_page_count == maximum);
        let remaining_items = max_items
            .checked_sub(*items_collected)
            .ok_or_else(|| pagination_kind_error(&meta, PaginationErrorKind::Overflow))?;
        aggregate_deadline
            .check()
            .map_err(|_| aggregate_timeout(&meta))?;
        let step = controller.advance(page, PageDecisionLimits::new(remaining_items));
        aggregate_deadline
            .check()
            .map_err(|_| aggregate_timeout(&meta))?;
        let (page_items, next_controller) =
            match step.map_err(|error| pagination_api_error(&meta, error))? {
                PageStep::Done { items } => (items, None),
                PageStep::Continue { items, next } => (items, Some(next)),
            };

        for item in page_items {
            let next_item_count = (*items_collected)
                .checked_add(1)
                .ok_or_else(|| pagination_kind_error(&meta, PaginationErrorKind::Overflow))?;
            if next_item_count > max_items {
                return Err(pagination_kind_error(
                    &meta,
                    PaginationErrorKind::CollectedItemCeilingExceeded,
                ));
            }
            items.push(item);
            *items_collected = next_item_count;
        }
        *pages_completed = accepted_page_count;
        *response_body_bytes_collected = next_response_body_bytes;

        if final_soft_page {
            return Ok(items);
        }
        let Some(next_controller) = next_controller else {
            return Ok(items);
        };

        if matches!(limit, PageLimit::CompleteWithin(cap) if accepted_page_count == cap.get()) {
            return Err(pagination_kind_error(
                &meta,
                PaginationErrorKind::CompletionLimitExceeded,
            ));
        }

        controller = next_controller;
        page_index = page_index
            .checked_add(1)
            .ok_or_else(|| pagination_kind_error(&meta, PaginationErrorKind::Overflow))?;
    }
}

fn aggregate_timeout(meta: &crate::request::RequestMeta) -> ApiError {
    ApiError::classified(
        meta.endpoint_id().clone(),
        meta.method().clone(),
        crate::error::ErrorCategory::Timeout,
    )
}

fn pagination_api_error(meta: &crate::request::RequestMeta, error: PaginationError) -> ApiError {
    ApiError::pagination(meta.endpoint_id().clone(), meta.method().clone(), error)
}

fn pagination_kind_error(
    meta: &crate::request::RequestMeta,
    kind: PaginationErrorKind,
) -> ApiError {
    pagination_api_error(meta, PaginationError::new(kind))
}
