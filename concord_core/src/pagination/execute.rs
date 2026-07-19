use crate::error::{ApiError, PaginationErrorKind};
use crate::hooks::{BoxFuture, CompletionOutcome, PaginationCompletionEvent};
use crate::request::{EndpointSpec, Request, ResponseHandler, prepare_pagination_page};
use crate::response::ExecuteResponse;

use super::identity::CanonicalIdentity;
use super::limits::CycleDetector;
use super::{PageLimit, PaginationController, PaginationError, PaginationPatch, PaginationRequest};

type DecodedPage<E> = <<E as EndpointSpec>::Response as ResponseHandler>::Output;
type PageItem<E> = <DecodedPage<E> as IntoIterator>::Item;
type PaginationResult<E> = Result<Vec<PageItem<E>>, ApiError>;

#[allow(
    clippy::too_many_arguments,
    reason = "the generated-private bridge keeps each typed pagination role explicit"
)]
#[doc(hidden)]
pub fn paginate_with<E, C, F, B, S>(
    request: Request<E>,
    limit: PageLimit,
    controller_factory: F,
    bind_controller: B,
    short_page_size: S,
) -> BoxFuture<'static, PaginationResult<E>>
where
    E: EndpointSpec + Clone + Sync,
    E::Response: ExecuteResponse,
    DecodedPage<E>: IntoIterator + Send + 'static,
    PageItem<E>: Send + 'static,
    C: PaginationController<DecodedPage<E>>,
    F: FnOnce(&E) -> Result<C, PaginationError> + Send + 'static,
    B: Fn(&mut E, &C) -> Result<(), PaginationError> + Send + Sync + 'static,
    S: Fn(&C) -> Option<usize> + Send + Sync + 'static,
{
    Box::pin(async move {
        let Request {
            core,
            endpoint,
            options,
        } = request;
        let meta = endpoint.request_meta();
        let aggregate_started = tokio::time::Instant::now();
        let aggregate_operation_id = core.allocate_operation_id(meta.endpoint(), meta.method())?;
        let max_pages = core.pagination_max_pages().get();
        let max_items = core.pagination_max_items().get();
        let identity_limit = core.pagination_identity_limit().get();

        let mut pages_completed = 0_u32;
        let mut items_collected = 0_usize;
        let mut recovered_authentication = false;
        let mut final_status = None;

        let requested_pages = match limit {
            PageLimit::Take(pages) => pages,
            PageLimit::HardCap(pages) => pages.get(),
        };

        let result = if requested_pages == 0 {
            Ok(Vec::new())
        } else if requested_pages > max_pages {
            Err(pagination_api_error(
                &meta,
                PaginationError::new(PaginationErrorKind::PageCeilingExceeded),
            ))
        } else {
            match controller_factory(&endpoint) {
                Ok(controller) => {
                    run_pages(
                        &core,
                        &endpoint,
                        options,
                        limit,
                        controller,
                        bind_controller,
                        short_page_size,
                        aggregate_operation_id,
                        max_pages,
                        max_items,
                        identity_limit,
                        &mut pages_completed,
                        &mut items_collected,
                        &mut recovered_authentication,
                        &mut final_status,
                    )
                    .await
                }
                Err(error) => Err(pagination_api_error(&meta, error)),
            }
        };

        let elapsed = tokio::time::Instant::now().saturating_duration_since(aggregate_started);
        let outcome = match &result {
            Ok(_) => CompletionOutcome::Success,
            Err(error) => CompletionOutcome::from_error_category(error.category()),
        };
        let event = PaginationCompletionEvent {
            operation_id: aggregate_operation_id,
            endpoint: meta.endpoint(),
            method: meta.method(),
            pages_completed,
            items_collected,
            recovered_authentication,
            elapsed,
            final_status,
            outcome,
        };
        core.debug_dispatcher(options.resolve_debug(core.default_debug()))
            .pagination_completion(&event);
        core.hooks().on_pagination_complete(event).await;
        result
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "the request-local page state mirrors the normative aggregate state"
)]
async fn run_pages<E, C, B, S>(
    core: &crate::client::ClientCore,
    original_endpoint: &E,
    options: crate::request::RequestOptions,
    limit: PageLimit,
    mut controller: C,
    bind_controller: B,
    short_page_size: S,
    aggregate_operation_id: crate::operation::OperationId,
    max_pages: u32,
    max_items: usize,
    identity_limit: usize,
    pages_completed: &mut u32,
    items_collected: &mut usize,
    recovered_authentication: &mut bool,
    final_status: &mut Option<http::StatusCode>,
) -> PaginationResult<E>
where
    E: EndpointSpec + Clone + Sync,
    E::Response: ExecuteResponse,
    DecodedPage<E>: IntoIterator + Send + 'static,
    PageItem<E>: Send + 'static,
    C: PaginationController<DecodedPage<E>>,
    B: Fn(&mut E, &C) -> Result<(), PaginationError> + Send + Sync + 'static,
    S: Fn(&C) -> Option<usize> + Send + Sync + 'static,
{
    let meta = original_endpoint.request_meta();
    let mut items = Vec::new();
    let mut identities = CycleDetector::default();
    let mut page_index = 0_u32;

    loop {
        let mut endpoint = original_endpoint.clone();
        let mut patch = PaginationPatch::default();
        controller
            .apply(&mut PaginationRequest::new(&mut patch))
            .map_err(|error| pagination_api_error(&meta, error))?;
        bind_controller(&mut endpoint, &controller)
            .map_err(|error| pagination_api_error(&meta, error))?;

        let page = prepare_pagination_page(
            Request {
                core: core.clone(),
                endpoint,
                options,
            },
            patch,
        )?;

        let identity = CanonicalIdentity::from_request_head(
            page.head.endpoint(),
            page.head.method(),
            page.head.logical_url(),
            page.head.public_headers(),
        )
        .map_err(|error| pagination_api_error(&meta, error))?;
        let (digest, retained_bytes) = identities
            .preflight(&identity, identity_limit)
            .map_err(|error| pagination_api_error(&meta, error))?;
        identities
            .retain(identity, digest, retained_bytes)
            .map_err(|error| pagination_api_error(&meta, error))?;

        let executed =
            crate::execute::execute_pagination_page(page, aggregate_operation_id, page_index)
                .await?;
        *recovered_authentication |= executed.recovered_authentication;
        if executed.final_status.is_some() {
            *final_status = executed.final_status;
        }
        let response = executed.result?;
        let page = response.into_value();

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
            matches!(limit, PageLimit::Take(maximum) if accepted_page_count == maximum);
        let candidate = if final_soft_page {
            None
        } else {
            Some(controller.next(&page))
        };

        let mut page_item_count = 0_usize;
        for item in page {
            let next_page_count = page_item_count
                .checked_add(1)
                .ok_or_else(|| pagination_kind_error(&meta, PaginationErrorKind::Overflow))?;
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
            page_item_count = next_page_count;
            *items_collected = next_item_count;
        }
        *pages_completed = accepted_page_count;

        if final_soft_page {
            return Ok(items);
        }
        if page_item_count == 0 {
            return Ok(items);
        }
        if short_page_size(&controller).is_some_and(|size| page_item_count < size) {
            return Ok(items);
        }

        let candidate = candidate
            .expect("non-final soft and hard-cap pages retain one controller result")
            .map_err(|error| pagination_api_error(&meta, error))?;
        let Some(next_controller) = candidate else {
            return Ok(items);
        };

        if matches!(limit, PageLimit::HardCap(cap) if accepted_page_count == cap.get()) {
            return Err(pagination_kind_error(
                &meta,
                PaginationErrorKind::HardCapExceeded,
            ));
        }

        controller = next_controller;
        page_index = page_index
            .checked_add(1)
            .ok_or_else(|| pagination_kind_error(&meta, PaginationErrorKind::Overflow))?;
    }
}

fn pagination_api_error(meta: &crate::request::RequestMeta, error: PaginationError) -> ApiError {
    ApiError::pagination(meta.endpoint(), meta.method().clone(), error)
}

fn pagination_kind_error(
    meta: &crate::request::RequestMeta,
    kind: PaginationErrorKind,
) -> ApiError {
    pagination_api_error(meta, PaginationError::new(kind))
}
