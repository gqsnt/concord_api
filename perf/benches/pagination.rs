use bytes::Bytes;
use concord_core::advanced::{HasNextCursor, OffsetLimitPagination, PaginateBinding};
use concord_core::advanced::{PaginationRuntimeAdapter, ProgressKey};
use concord_core::internal::{
    ClientPlanContext, EndpointMeta, EndpointPlan, PaginationMarker, PreparedBody,
    RequestOverrides, RequestPlan, ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{
    ApiClientError, CursorPagination, Endpoint, PageItems, PagedPagination, PaginatedEndpoint,
    PaginationTermination, ReusableEndpoint, Text,
};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use http::{HeaderValue, Method, StatusCode};
use perf::support::{MockResponse, MockTransport, client, runtime};
use std::env;
use std::future::Future;
use std::hint::black_box;
use std::pin::Pin;

#[derive(Clone, Copy)]
enum Kind {
    Offset,
    Paged,
    Cursor,
    NonProgress,
}

#[derive(Clone)]
struct ItemsEndpoint {
    kind: Kind,
    offset: u64,
    page: u64,
    cursor: Option<String>,
    count: u64,
}

impl ItemsEndpoint {
    fn offset(count: u64) -> Self {
        Self {
            kind: Kind::Offset,
            offset: 0,
            page: 1,
            cursor: None,
            count,
        }
    }

    fn paged(count: u64) -> Self {
        Self {
            kind: Kind::Paged,
            offset: 0,
            page: 1,
            cursor: None,
            count,
        }
    }

    fn cursor(count: u64) -> Self {
        Self {
            kind: Kind::Cursor,
            offset: 0,
            page: 1,
            cursor: None,
            count,
        }
    }

    fn non_progress(count: u64) -> Self {
        Self {
            kind: Kind::NonProgress,
            offset: 0,
            page: 1,
            cursor: Some("BENCH_FAKE_CURSOR".to_string()),
            count,
        }
    }
}

impl Endpoint<perf::support::PerfCx> for ItemsEndpoint {
    type Response = CursorPage;

    fn execute<'a>(
        client: &'a concord_core::prelude::ApiClient<perf::support::PerfCx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>> {
        Box::pin(async move {
            let decoded = client.execute_plan::<Text<String>>(plan).await?;
            Ok(CursorPage::parse(decoded.into_value()))
        })
    }
}

impl ReusableEndpoint<perf::support::PerfCx> for ItemsEndpoint {
    fn plan(
        &self,
        _ctx: &ClientPlanContext<'_, perf::support::PerfCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        let mut policy = ResolvedPolicy::default();
        match self.kind {
            Kind::Offset => {
                policy
                    .query
                    .push(("offset".to_string(), self.offset.to_string()));
                policy
                    .query
                    .push(("limit".to_string(), self.count.to_string()));
            }
            Kind::Paged => {
                policy
                    .query
                    .push(("page".to_string(), self.page.to_string()));
                policy
                    .query
                    .push(("per_page".to_string(), self.count.to_string()));
            }
            Kind::Cursor | Kind::NonProgress => {
                if let Some(cursor) = &self.cursor {
                    policy.query.push(("cursor".to_string(), cursor.clone()));
                }
                policy
                    .query
                    .push(("per_page".to_string(), self.count.to_string()));
            }
        }
        Ok(RequestPlan {
            endpoint: EndpointPlan {
                meta: EndpointMeta {
                    name: "PerfPagination",
                    method: Method::GET,
                    idempotent: true,
                    facade_path: &[],
                },
                route: ResolvedRoute::new(
                    http::uri::Scheme::HTTPS,
                    "example.com",
                    "/perf/pagination",
                ),
                policy,
                response: ResponsePlan {
                    accept: Some(HeaderValue::from_static("text/plain")),
                    no_content: false,
                    format: concord_core::internal::Format::Text,
                },
                pagination: Some(PaginationMarker),
            },
            body: PreparedBody::empty(),
            overrides: RequestOverrides::default(),
        })
    }
}

impl PaginatedEndpoint<perf::support::PerfCx> for ItemsEndpoint {
    type Pagination = OffsetLimitPagination;

    fn pagination_runtime(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::PaginationRuntime<Self, Self::Response>>> {
        match self.kind {
            Kind::Offset => Some(Box::new(
                PaginationRuntimeAdapter::<OffsetLimitPagination>::new(),
            )),
            Kind::Paged => Some(Box::new(PaginationRuntimeAdapter::<PagedPagination>::new())),
            Kind::Cursor | Kind::NonProgress => Some(Box::new(PaginationRuntimeAdapter::<
                CursorPagination<String>,
            >::new())),
        }
    }
}

impl PaginateBinding<OffsetLimitPagination> for ItemsEndpoint {
    fn load_pagination(&self) -> OffsetLimitPagination {
        OffsetLimitPagination {
            offset: self.offset,
            limit: self.count,
        }
    }

    fn store_pagination(&mut self, pagination: &OffsetLimitPagination) {
        self.offset = pagination.offset;
        self.count = pagination.limit;
    }
}

impl PaginateBinding<PagedPagination> for ItemsEndpoint {
    fn load_pagination(&self) -> PagedPagination {
        PagedPagination {
            page: self.page,
            per_page: self.count,
        }
    }

    fn store_pagination(&mut self, pagination: &PagedPagination) {
        self.page = pagination.page;
        self.count = pagination.per_page;
    }
}

impl PaginateBinding<CursorPagination<String>> for ItemsEndpoint {
    fn load_pagination(&self) -> CursorPagination<String> {
        CursorPagination {
            cursor: self.cursor.clone(),
            per_page: self.count,
            send_cursor_on_first: true,
            stop_when_cursor_missing: !matches!(self.kind, Kind::NonProgress),
        }
    }

    fn store_pagination(&mut self, pagination: &CursorPagination<String>) {
        self.cursor = pagination.cursor.clone();
        self.count = pagination.per_page;
    }
}

#[derive(Clone)]
struct CursorPage {
    items: Vec<String>,
    next_cursor: Option<String>,
}

impl CursorPage {
    fn parse(raw: String) -> Self {
        let (items, next_cursor) = raw
            .split_once("|next=")
            .map_or((raw.as_str(), None), |(items, cursor)| {
                (items, (!cursor.is_empty()).then(|| cursor.to_string()))
            });
        let items = items
            .split(',')
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect();
        Self { items, next_cursor }
    }
}

impl PageItems for CursorPage {
    type Item = String;

    fn item_count(&self) -> usize {
        self.items.len()
    }

    fn into_items(self) -> Vec<Self::Item> {
        self.items
    }
}

impl HasNextCursor for CursorPage {
    type Cursor = String;

    fn next_cursor(&self) -> Option<Self::Cursor> {
        self.next_cursor.clone()
    }
}

fn page_body(page: usize, total: usize) -> Bytes {
    let next = if page + 1 < total {
        format!("|next=cursor-{}", page + 1)
    } else {
        String::new()
    };
    Bytes::from(format!("item-{page}{next}"))
}

fn responses(pages: usize) -> Vec<MockResponse> {
    (0..pages)
        .map(|idx| MockResponse::text(StatusCode::OK, page_body(idx, pages)))
        .collect()
}

fn full_fixture_enabled() -> bool {
    matches!(env::var("CONCORD_PERF_FULL"), Ok(value) if value == "1")
}

fn bench_collect(c: &mut Criterion, name: &str, endpoint: ItemsEndpoint, pages: usize) {
    let rt = runtime();
    c.bench_function(name, |b| {
        let endpoint = endpoint.clone();
        b.to_async(&rt).iter_batched(
            move || {
                let transport = MockTransport::scripted(responses(pages));
                (client(transport), endpoint.clone())
            },
            |(client, endpoint)| async move {
                let items = client
                    .request(endpoint)
                    .paginate(PaginationTermination::take_pages(pages))
                    .collect()
                    .await
                    .expect("pagination collect");
                black_box(items.len());
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_non_progress(c: &mut Criterion) {
    let rt = runtime();
    c.bench_function("non_progress/error_path", |b| {
        b.to_async(&rt).iter_batched(
            || {
                let transport = MockTransport::scripted(vec![
                    MockResponse::text(StatusCode::OK, "item-0|next=BENCH_FAKE_CURSOR"),
                ]);
                (client(transport), ItemsEndpoint::non_progress(1))
            },
            |(client, endpoint)| async move {
                let err = client
                    .request(endpoint)
                    .paginate(PaginationTermination::hard_page_cap(10))
                    .collect()
                    .await
                    .expect_err("non-progress should fail");
                black_box((
                    err.pagination_error_kind(),
                    format!("{:?}", ProgressKey::Str("x".into())).len(),
                ));
            },
            BatchSize::SmallInput,
        )
    });
}

fn pagination(c: &mut Criterion) {
    for pages in [1usize, 10, 100] {
        bench_collect(
            c,
            &format!(
                "collect/offset/{pages}_page{}",
                if pages == 1 { "" } else { "s" }
            ),
            ItemsEndpoint::offset(1),
            pages,
        );
    }
    bench_collect(c, "collect/paged/10_pages", ItemsEndpoint::paged(1), 10);
    bench_collect(c, "collect/cursor/100_pages", ItemsEndpoint::cursor(1), 100);
    bench_non_progress(c);

    if full_fixture_enabled() {
        bench_collect(
            c,
            "collect/offset/1000_pages",
            ItemsEndpoint::offset(1),
            1000,
        );
        bench_collect(
            c,
            "collect/cursor/1000_pages",
            ItemsEndpoint::cursor(1),
            1000,
        );
    }
}

criterion_group!(benches, pagination);
criterion_main!(benches);
