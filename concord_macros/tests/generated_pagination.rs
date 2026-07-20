use std::cell::Cell;
use std::num::{NonZeroU32, NonZeroU64, NonZeroUsize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use concord_core::advanced::{
    BoxFuture, CodecError, DecodeBody, DecodeContext, HasNextCursor, HookError,
    PaginationController, PaginationError, RateLimitError, RateLimitPermit, RateLimitRequest,
    RateLimitResponse, RateLimiter, RuntimeHooks, SafeProxy,
};
use concord_core::prelude::Bytes;
use concord_macros::api;
use concord_test_support::server::{ResponseScript, ScriptedServer};

fn nz32(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).expect("positive u32")
}

fn nz64(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).expect("positive u64")
}

api! {
    client OffsetApi {
        base "http://pagination-offset.example.test/v1/";
        GET Items(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u32>> {
            path ["items"];
            query {
                "offset" = ep.offset;
                "limit" = ep.limit;
            }
            paginate concord_core::advanced::OffsetPagination {
                offset: bind ep.offset,
                page_size: bind ep.limit,
            }
        }
    }
}

api! {
    client PageApi {
        base "http://pagination-page.example.test/v1/";
        GET Items(page: std::num::NonZeroU64, per_page: std::num::NonZeroU32) -> Json<Vec<u32>> {
            path ["items"];
            query {
                "page" = ep.page;
                "per_page" = ep.per_page;
            }
            paginate concord_core::advanced::PagePagination {
                page: bind ep.page,
                page_size: bind ep.per_page,
            }
        }
    }
}

static APPLICATION_BOUND_CLONES: AtomicUsize = AtomicUsize::new(0);
static APPLICATION_STABLE_CLONES: AtomicUsize = AtomicUsize::new(0);

pub struct CountedOffset(u64);

impl Clone for CountedOffset {
    fn clone(&self) -> Self {
        APPLICATION_BOUND_CLONES.fetch_add(1, Ordering::SeqCst);
        Self(self.0)
    }
}

impl std::fmt::Display for CountedOffset {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

pub struct StablePolicy(&'static str);

impl Clone for StablePolicy {
    fn clone(&self) -> Self {
        APPLICATION_STABLE_CLONES.fetch_add(1, Ordering::SeqCst);
        Self(self.0)
    }
}

mod application_count_controller {
    use super::*;

    pub struct ApplicationCountController {
        pub offset: CountedOffset,
        pub page_size: NonZeroU32,
        pub policy: StablePolicy,
        pub attempts: u8,
        pub enabled: bool,
        pub label: &'static str,
    }

    impl PaginationController<Vec<u32>> for ApplicationCountController {
        fn next(self, _page: &Vec<u32>) -> Result<Option<Self>, PaginationError> {
            let offset = self
                .offset
                .0
                .checked_add(u64::from(self.page_size.get()))
                .ok_or_else(|| {
                    PaginationError::new(concord_core::advanced::PaginationErrorKind::Overflow)
                })?;
            Ok(Some(Self {
                offset: CountedOffset(offset),
                page_size: self.page_size,
                policy: self.policy,
                attempts: self.attempts,
                enabled: self.enabled,
                label: self.label,
            }))
        }

        fn short_page_size(&self) -> Option<NonZeroUsize> {
            NonZeroUsize::new(self.page_size.get() as usize)
        }
    }
}

use application_count_controller::ApplicationCountController;

api! {
    client ApplicationCountApi {
        base "http://pagination-application-count.example.test/v1/";
        var policy: crate::StablePolicy;
        GET Items(offset: crate::CountedOffset, limit: std::num::NonZeroU32) -> Json<Vec<u32>> {
            path ["items"];
            query {
                "offset" = ep.offset;
                "limit" = ep.limit;
            }
            paginate ApplicationCountController {
                offset: bind ep.offset,
                page_size: bind ep.limit,
                policy: vars.policy,
                attempts: 0,
                enabled: true,
                label: "count",
            }
        }
    }
}

pub struct VendorPageCodec;

pub struct VendorPage {
    items: Vec<u32>,
    next: Option<String>,
}

impl IntoIterator for VendorPage {
    type Item = u32;
    type IntoIter = std::vec::IntoIter<u32>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl HasNextCursor<String> for VendorPage {
    fn next_cursor(&self) -> Option<&String> {
        self.next.as_ref()
    }
}

impl DecodeBody for VendorPageCodec {
    type Output = VendorPage;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-page");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type == Self::ACCEPT {
            Ok(())
        } else {
            Err(CodecError::new("page media mismatch"))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let text = std::str::from_utf8(&body).map_err(|_| CodecError::new("page utf8"))?;
        let (items, next) = text
            .split_once('|')
            .ok_or_else(|| CodecError::new("page structure"))?;
        let items = if items.is_empty() {
            Vec::new()
        } else {
            items
                .split(',')
                .map(|item| {
                    item.parse::<u32>()
                        .map_err(|_| CodecError::new("page item"))
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let next = (next != "-").then(|| next.to_owned());
        Ok(VendorPage { items, next })
    }
}

pub struct NonCloneItem(u32);
pub struct NonClonePageCodec;

impl DecodeBody for NonClonePageCodec {
    type Output = Vec<NonCloneItem>;

    const ACCEPT: Option<&'static str> = None;

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let text = std::str::from_utf8(&body).map_err(|_| CodecError::new("item utf8"))?;
        text.split(',')
            .filter(|item| !item.is_empty())
            .map(|item| {
                item.parse::<u32>()
                    .map(NonCloneItem)
                    .map_err(|_| CodecError::new("item number"))
            })
            .collect()
    }
}

pub struct OneNonClonePage {
    pub marker: Cell<()>,
}

#[allow(
    clippy::declare_interior_mutable_const,
    reason = "the fixture intentionally constructs a Send but non-Sync controller"
)]
pub const NOT_SYNC_MARKER: Cell<()> = Cell::new(());

impl PaginationController<Vec<NonCloneItem>> for OneNonClonePage {
    fn next(self, _page: &Vec<NonCloneItem>) -> Result<Option<Self>, PaginationError> {
        Ok(None)
    }
}

api! {
    client CursorApi {
        base "http://pagination-cursor.example.test/v1/";
        GET Items(cursor: Option<String>) -> crate::VendorPageCodec {
            path ["items"];
            query {
                "cursor" = ep.cursor;
            }
            paginate concord_core::advanced::CursorPagination<String> {
                cursor: bind ep.cursor,
            }
        }
    }
}

static CUSTOM_VALIDATE_CALLS: AtomicUsize = AtomicUsize::new(0);
static CUSTOM_NEXT_CALLS: AtomicUsize = AtomicUsize::new(0);

pub struct VendorController {
    pub cursor: Option<String>,
    pub region: String,
}

impl PaginationController<VendorPage> for VendorController {
    fn validate(&self) -> Result<(), PaginationError> {
        CUSTOM_VALIDATE_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn next(self, page: &VendorPage) -> Result<Option<Self>, PaginationError> {
        CUSTOM_NEXT_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(page.next.as_ref().map(|cursor| Self {
            cursor: Some(cursor.clone()),
            region: self.region,
        }))
    }
}

pub struct ScopeBindingController {
    pub cursor: Option<String>,
    pub region: String,
    pub repository: String,
}

impl PaginationController<VendorPage> for ScopeBindingController {
    fn next(self, page: &VendorPage) -> Result<Option<Self>, PaginationError> {
        Ok(page.next.as_ref().map(|next| Self {
            cursor: Some(next.clone()),
            region: next.clone(),
            repository: self.repository,
        }))
    }
}

api! {
    client CustomApi {
        base "http://pagination-custom.example.test/v1/";
        GET Items(cursor: Option<String>, region: String) -> crate::VendorPageCodec {
            path ["items"];
            query { "cursor" = ep.cursor; }
            headers { "x-region" = ep.region; }
            paginate crate::VendorController {
                cursor: bind ep.cursor,
                region: ep.region,
            }
        }
    }
}

pub struct ZeroController {}

impl PaginationController<VendorPage> for ZeroController {
    fn next(self, _page: &VendorPage) -> Result<Option<Self>, PaginationError> {
        Ok(None)
    }
}

api! {
    client ZeroApi {
        base "http://pagination-zero.example.test/v1/";
        GET Items() -> crate::VendorPageCodec {
            paginate crate::ZeroController {}
        }
    }
}

api! {
    client NonCloneApi {
        base "http://pagination-non-clone.example.test/v1/";
        GET Items() -> crate::NonClonePageCodec {
            paginate crate::OneNonClonePage {
                marker: crate::NOT_SYNC_MARKER,
            }
        }
    }
}

api! {
    client NestedPaginationApi {
        base "http://pagination-nested.example.test/v1/";
        scope Organizations(region: String) {
            path ["organizations", scope.region];
            scope Repositories(repository: String) {
                path ["repositories", scope.repository];
                GET Entries(cursor: Option<String>) as fetch -> crate::VendorPageCodec {
                    path ["entries"];
                    query { "cursor" = ep.cursor; }
                    headers { "x-region" = scope.region; }
                    paginate crate::ScopeBindingController {
                        cursor: bind ep.cursor,
                        region: bind scope.region,
                        repository: scope.repository,
                    }
                }
            }
        }
    }
}

api! {
    client CollisionPaginationApi {
        base "http://pagination-collision.example.test/v1/";
        scope Left() {
            path ["left"];
            GET Items(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u8>> {
                paginate concord_core::advanced::OffsetPagination {
                    offset: bind ep.offset,
                    page_size: bind ep.limit,
                }
            }
        }
        scope Right() {
            path ["right"];
            GET Items(page: std::num::NonZeroU64, per_page: std::num::NonZeroU32) -> Json<Vec<u8>> {
                paginate concord_core::advanced::PagePagination {
                    page: bind ep.page,
                    page_size: bind ep.per_page,
                }
            }
        }
    }
}

api! {
    client PaginationRateApi {
        base "http://pagination-rate.example.test/v1/";
        rate_limits {
            page_key {
                by [host, ep.offset];
                100 / 1s;
            }
        }
        GET Items(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u32>> {
            host [ep.offset];
            path ["items", ep.offset];
            query {
                "offset" = ep.offset;
                "limit" = ep.limit;
            }
            headers { "X-Page-Offset" = ep.offset; }
            rate_limit page_key;
            paginate concord_core::advanced::OffsetPagination {
                offset: bind ep.offset,
                page_size: bind ep.limit,
            }
        }
    }
}

mod repeated_nested_expansion {
    use concord_macros::api;

    api! {
        client NestedMacroPaginationApi {
            base "http://pagination-nested-macro.example.test/v1/";
            GET Items(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u8>> {
                paginate concord_core::advanced::OffsetPagination {
                    offset: bind ep.offset,
                    page_size: bind ep.limit,
                }
            }
        }
    }
}

#[derive(Default)]
struct PaginationHooks {
    before: Mutex<Vec<String>>,
    response: Mutex<Vec<String>>,
    completion: Mutex<Vec<String>>,
    aggregate: Mutex<Vec<GeneratedAggregateRecord>>,
}

type GeneratedAggregateRecord = (u32, usize, bool, Option<u16>);
type GeneratedRateKeyPart = (String, String);
type GeneratedLimiterContext = (u32, String, Option<String>, Vec<GeneratedRateKeyPart>);

#[derive(Default)]
struct PaginationLimiterProbe {
    contexts: Mutex<Vec<GeneratedLimiterContext>>,
    observations: Mutex<Vec<GeneratedLimiterContext>>,
}

impl RateLimiter for PaginationLimiterProbe {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let parts = request
            .plan
            .rules()
            .iter()
            .flat_map(|rule| rule.key().parts())
            .map(|part| (part.name().to_owned(), part.value().to_owned()))
            .collect();
        self.contexts.lock().expect("limiter contexts").push((
            request.page_index,
            request.safe_url.to_owned(),
            request.host.map(str::to_owned),
            parts,
        ));
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe<'a>(&'a self, response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
        let request = response.request;
        let parts = request
            .plan
            .rules()
            .iter()
            .flat_map(|rule| rule.key().parts())
            .map(|part| (part.name().to_owned(), part.value().to_owned()))
            .collect();
        self.observations
            .lock()
            .expect("limiter observations")
            .push((
                request.page_index,
                request.safe_url.to_owned(),
                request.host.map(str::to_owned),
                parts,
            ));
        Box::pin(async {})
    }
}

impl RuntimeHooks for PaginationHooks {
    fn before_send<'a>(
        &'a self,
        context: concord_core::advanced::BeforeSend<'a>,
    ) -> BoxFuture<'a, Result<(), HookError>> {
        Box::pin(async move {
            self.before
                .lock()
                .expect("before lock")
                .push(format!("{} {:?}", context.safe_url, context.headers));
            Ok(())
        })
    }

    fn on_pagination_complete<'a>(
        &'a self,
        context: concord_core::advanced::PaginationCompletionEvent<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.aggregate.lock().expect("aggregate lock").push((
                context.pages_completed,
                context.items_collected,
                context.recovered_authentication,
                context.final_status.map(|status| status.as_u16()),
            ));
        })
    }

    fn on_response_head<'a>(
        &'a self,
        context: concord_core::advanced::ResponseHeadEvent<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.response
                .lock()
                .expect("response lock")
                .push(format!("{} {:?}", context.safe_url, context.headers));
        })
    }

    fn on_complete<'a>(
        &'a self,
        context: concord_core::advanced::CompletionEvent<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.completion
                .lock()
                .expect("completion lock")
                .push(context.safe_url.to_owned());
        })
    }
}

#[test]
fn production_macro_pipeline_has_no_controller_category_or_callback_branches() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for relative in [
        "src/ast.rs",
        "src/model.rs",
        "src/parse.rs",
        "src/resolve.rs",
        "src/codegen.rs",
    ] {
        let path = root.join(relative);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        for forbidden in [
            "OffsetPagination",
            "PagePagination",
            "CursorPagination",
            "BuiltInPagination",
            "Pagination::Offset",
            "Pagination::Page",
            "Pagination::Cursor",
            "Pagination::Custom",
            "controller_factory",
            "short_page_callback",
        ] {
            assert!(
                !production.contains(forbidden),
                "{relative} retains controller-category semantics {forbidden}"
            );
        }
    }

    let core_root = root
        .parent()
        .expect("workspace root")
        .join("concord_core/src");
    let execute = std::fs::read_to_string(core_root.join("pagination/execute.rs"))
        .expect("core pagination loop source");
    for forbidden in [
        "controller_factory",
        "short_page_callback",
        "PaginationRequest",
        "PaginationPatch",
        "page_item_count == 0",
    ] {
        assert!(
            !execute.contains(forbidden),
            "core loop retains superseded pagination mechanism {forbidden}"
        );
    }
}

#[tokio::test]
async fn generated_offset_pagination_is_directly_awaitable_and_stops_on_short_page() {
    use offset_api::prelude::*;

    let server = ScriptedServer::start([
        ResponseScript::ok(br#"[1,2]"#.to_vec()).with_header("content-type", "application/json"),
        ResponseScript::ok(br#"[3]"#.to_vec()).with_header("content-type", "application/json"),
    ])
    .expect("pagination server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let client = OffsetApi::builder()
        .http(move |http| http.proxy(proxy))
        .build()
        .expect("generated pagination client");

    let items = client
        .items(0, nz32(2))
        .paginate(10_u32)
        .await
        .expect("paginated items");
    assert_eq!(items, [1, 2, 3]);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].target().contains("offset=0&limit=2"));
    assert!(requests[1].target().contains("offset=2&limit=2"));
}

#[tokio::test]
async fn application_controller_selects_the_same_count_termination_without_macro_privilege() {
    use application_count_api::prelude::*;

    let server = ScriptedServer::start([
        ResponseScript::ok(br#"[1,2]"#.to_vec()).with_header("content-type", "application/json"),
        ResponseScript::ok(br#"[3]"#.to_vec()).with_header("content-type", "application/json"),
    ])
    .expect("application count server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let client = ApplicationCountApi::builder()
        .policy(StablePolicy("stable-policy"))
        .http(move |http| http.proxy(proxy))
        .build()
        .expect("application count client");

    let pagination = client.items(CountedOffset(0), nz32(2)).paginate(10_u32);
    APPLICATION_BOUND_CLONES.store(0, Ordering::SeqCst);
    APPLICATION_STABLE_CLONES.store(0, Ordering::SeqCst);
    let items = pagination
        .await
        .expect("application-selected count termination");
    assert_eq!(items, [1, 2, 3]);
    assert_eq!(APPLICATION_BOUND_CLONES.load(Ordering::SeqCst), 3);
    assert_eq!(APPLICATION_STABLE_CLONES.load(Ordering::SeqCst), 1);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].target().contains("offset=0&limit=2"));
    assert!(requests[1].target().contains("offset=2&limit=2"));
}

#[tokio::test]
async fn generated_page_number_soft_and_hard_limits_are_deterministic() {
    use page_api::prelude::*;

    let soft_server = ScriptedServer::start([
        ResponseScript::ok(br#"[1,2]"#.to_vec()).with_header("content-type", "application/json"),
        ResponseScript::ok(br#"[3,4]"#.to_vec()).with_header("content-type", "application/json"),
    ])
    .expect("soft server");
    let soft_proxy = SafeProxy::parse(soft_server.url("")).expect("proxy");
    let soft_client = PageApi::builder()
        .http(move |http| http.proxy(soft_proxy))
        .build()
        .expect("page client");
    let items = soft_client
        .items(nz64(1), nz32(2))
        .paginate(2_u32)
        .await
        .expect("soft limit succeeds");
    assert_eq!(items, [1, 2, 3, 4]);
    assert_eq!(soft_server.requests().len(), 2);

    let hard_server =
        ScriptedServer::start([ResponseScript::ok(br#"[7,8]"#.to_vec())
            .with_header("content-type", "application/json")])
        .expect("hard server");
    let hard_proxy = SafeProxy::parse(hard_server.url("")).expect("proxy");
    let hard_client = PageApi::builder()
        .http(move |http| http.proxy(hard_proxy))
        .build()
        .expect("page client");
    let error = hard_client
        .items(nz64(1), nz32(2))
        .paginate(concord_core::prelude::PageLimit::HardCap(
            std::num::NonZeroU32::MIN,
        ))
        .await
        .expect_err("next page beyond hard cap");
    assert_eq!(
        error.pagination_kind(),
        Some(concord_core::advanced::PaginationErrorKind::HardCapExceeded)
    );
    assert_eq!(hard_server.requests().len(), 1);
}

#[tokio::test]
async fn cursor_and_custom_empty_pages_follow_controller_authority_and_redact_continuations() {
    use cursor_api::prelude::*;
    use custom_api::prelude::*;

    let cursor_server = ScriptedServer::start([
        ResponseScript::ok(b"|next-cursor".to_vec())
            .with_header("content-type", "application/x-concord-page"),
        ResponseScript::ok(b"2|-".to_vec())
            .with_header("content-type", "application/x-concord-page"),
    ])
    .expect("cursor server");
    let cursor_proxy = SafeProxy::parse(cursor_server.url("")).expect("proxy");
    let cursor_client = CursorApi::builder()
        .http(move |http| http.proxy(cursor_proxy))
        .build()
        .expect("cursor client");
    let cursor_items = cursor_client
        .items()
        .paginate(5_u32)
        .await
        .expect("cursor pages");
    assert_eq!(cursor_items, [2]);
    let cursor_requests = cursor_server.requests();
    assert_eq!(cursor_requests.len(), 2);
    assert!(!cursor_requests[0].target().contains("cursor="));
    assert!(cursor_requests[1].target().contains("cursor=next-cursor"));

    CUSTOM_VALIDATE_CALLS.store(0, Ordering::SeqCst);
    CUSTOM_NEXT_CALLS.store(0, Ordering::SeqCst);
    let hooks = Arc::new(PaginationHooks::default());
    let configured_hooks: Arc<dyn RuntimeHooks> = hooks.clone();
    let custom_server = ScriptedServer::start([
        ResponseScript::ok(b"|custom-cursor-sentinel-6d91".to_vec())
            .with_header("content-type", "application/x-concord-page"),
        ResponseScript::ok(b"4|-".to_vec())
            .with_header("content-type", "application/x-concord-page"),
    ])
    .expect("custom server");
    let custom_proxy = SafeProxy::parse(custom_server.url("")).expect("proxy");
    let custom_client = CustomApi::builder()
        .http(move |http| http.proxy(custom_proxy))
        .runtime(move |runtime| runtime.hooks(configured_hooks))
        .build()
        .expect("custom client");
    let custom_items = custom_client
        .items("region-header-sentinel-b17".to_owned())
        .paginate(5_u32)
        .await
        .expect("custom pages");
    assert_eq!(custom_items, [4]);
    assert_eq!(CUSTOM_VALIDATE_CALLS.load(Ordering::SeqCst), 2);
    assert_eq!(CUSTOM_NEXT_CALLS.load(Ordering::SeqCst), 2);
    let requests = custom_server.requests();
    assert_eq!(requests.len(), 2, "a nonempty short custom page continues");
    assert!(
        requests[1]
            .target()
            .contains("cursor=custom-cursor-sentinel-6d91")
    );
    assert_eq!(
        requests[1].header("x-region"),
        Some("region-header-sentinel-b17")
    );
    for safe in hooks.before.lock().expect("before capture").iter() {
        assert!(!safe.contains("custom-cursor-sentinel-6d91"));
        assert!(safe.contains("region-header-sentinel-b17"));
    }
    assert_eq!(
        hooks
            .aggregate
            .lock()
            .expect("aggregate capture")
            .as_slice(),
        [(2, 1, false, Some(200))]
    );
}

#[tokio::test]
async fn take_zero_skips_controller_and_page_work_but_emits_one_aggregate_success() {
    use zero_api::prelude::*;

    let hooks = Arc::new(PaginationHooks::default());
    let configured_hooks: Arc<dyn RuntimeHooks> = hooks.clone();
    let client = ZeroApi::builder()
        .runtime(move |runtime| runtime.hooks(configured_hooks))
        .build()
        .expect("zero client");
    let items = client.items().paginate(0_u32).await.expect("zero pages");
    assert!(items.is_empty());
    assert!(hooks.before.lock().expect("before capture").is_empty());
    assert_eq!(
        hooks
            .aggregate
            .lock()
            .expect("aggregate capture")
            .as_slice(),
        [(0, 0, false, None)]
    );
}

#[tokio::test]
async fn built_in_overflow_is_ignored_on_short_stop_and_hard_cap_can_end_naturally() {
    use offset_api::prelude::*;
    use page_api::prelude::*;

    let offset_server = ScriptedServer::start([
        ResponseScript::ok(br#"[5]"#.to_vec()).with_header("content-type", "application/json")
    ])
    .expect("offset overflow server");
    let offset_proxy = SafeProxy::parse(offset_server.url("")).expect("offset proxy");
    let offset_client = OffsetApi::builder()
        .http(move |http| http.proxy(offset_proxy))
        .build()
        .expect("offset overflow client");
    let items = offset_client
        .items(u64::MAX, nz32(2))
        .paginate(2_u32)
        .await
        .expect("short page overrides speculative offset overflow");
    assert_eq!(items, [5]);
    assert_eq!(offset_server.requests().len(), 1);

    let page_server = ScriptedServer::start([
        ResponseScript::ok(br#"[6]"#.to_vec()).with_header("content-type", "application/json")
    ])
    .expect("page hard-cap server");
    let page_proxy = SafeProxy::parse(page_server.url("")).expect("page proxy");
    let page_client = PageApi::builder()
        .http(move |http| http.proxy(page_proxy))
        .build()
        .expect("page hard-cap client");
    let items = page_client
        .items(nz64(u64::MAX), nz32(2))
        .paginate(concord_core::prelude::PageLimit::HardCap(
            std::num::NonZeroU32::MIN,
        ))
        .await
        .expect("short page naturally terminates at hard cap");
    assert_eq!(items, [6]);
    assert_eq!(page_server.requests().len(), 1);
}

#[tokio::test]
async fn empty_present_cursor_is_distinct_and_items_need_not_clone() {
    use cursor_api::prelude::*;
    use non_clone_api::prelude::*;

    fn assert_send<T: Send>() {}
    assert_send::<OneNonClonePage>();

    let cursor_server = ScriptedServer::start([
        ResponseScript::ok(b"1|".to_vec())
            .with_header("content-type", "application/x-concord-page"),
        ResponseScript::ok(b"2|-".to_vec())
            .with_header("content-type", "application/x-concord-page"),
    ])
    .expect("empty cursor server");
    let cursor_proxy = SafeProxy::parse(cursor_server.url("")).expect("cursor proxy");
    let cursor_client = CursorApi::builder()
        .http(move |http| http.proxy(cursor_proxy))
        .build()
        .expect("empty cursor client");
    let items = cursor_client
        .items()
        .paginate(3_u32)
        .await
        .expect("empty present cursor continues");
    assert_eq!(items, [1, 2]);
    let requests = cursor_server.requests();
    assert_eq!(requests.len(), 2);
    assert!(!requests[0].target().contains("cursor="));
    assert!(requests[1].target().contains("cursor="));

    let item_server = ScriptedServer::start([ResponseScript::ok(b"7,8".to_vec())])
        .expect("non-clone item server");
    let item_proxy = SafeProxy::parse(item_server.url("")).expect("item proxy");
    let item_client = NonCloneApi::builder()
        .http(move |http| http.proxy(item_proxy))
        .build()
        .expect("non-clone item client");
    let items = item_client
        .items()
        .paginate(1_u32)
        .await
        .expect("non-clone items");
    assert_eq!(
        items.into_iter().map(|item| item.0).collect::<Vec<_>>(),
        [7, 8]
    );
}

#[tokio::test]
async fn explicit_endpoint_nested_scope_alias_and_repeated_helpers_have_parity() {
    use collision_pagination_api::prelude::*;
    use nested_pagination_api::prelude::*;
    use offset_api::prelude::*;
    use repeated_nested_expansion::nested_macro_pagination_api::prelude::*;

    let explicit_server = ScriptedServer::start([
        ResponseScript::ok(br#"[9]"#.to_vec()).with_header("content-type", "application/json")
    ])
    .expect("explicit server");
    let explicit_proxy = SafeProxy::parse(explicit_server.url("")).expect("explicit proxy");
    let explicit_client = OffsetApi::builder()
        .http(move |http| http.proxy(explicit_proxy))
        .build()
        .expect("explicit client");
    let endpoint = offset_api::endpoints::Items::new(0, nz32(2));
    let explicit_items = explicit_client
        .request(endpoint)
        .paginate(1_u32)
        .await
        .expect("explicit paginated endpoint");
    assert_eq!(explicit_items, [9]);

    let nested_server = ScriptedServer::start([
        ResponseScript::ok(b"|region-b".to_vec())
            .with_header("content-type", "application/x-concord-page"),
        ResponseScript::ok(b"11|-".to_vec())
            .with_header("content-type", "application/x-concord-page"),
        ResponseScript::ok(b"12|-".to_vec())
            .with_header("content-type", "application/x-concord-page"),
    ])
    .expect("nested server");
    let nested_proxy = SafeProxy::parse(nested_server.url("")).expect("nested proxy");
    let nested_hooks = Arc::new(PaginationHooks::default());
    let configured_nested_hooks: Arc<dyn RuntimeHooks> = nested_hooks.clone();
    let nested_client = NestedPaginationApi::builder()
        .http(move |http| http.proxy(nested_proxy))
        .runtime(move |runtime| runtime.hooks(configured_nested_hooks))
        .build()
        .expect("nested client");
    let repositories = nested_client
        .organizations("region-a".to_owned())
        .repositories("repository-b".to_owned());
    let nested_items = repositories
        .fetch()
        .paginate(3_u32)
        .await
        .expect("nested aliased pagination");
    assert_eq!(nested_items, [11]);
    let original_snapshot_items = repositories
        .fetch()
        .paginate(1_u32)
        .await
        .expect("original scope snapshot remains unchanged");
    assert_eq!(original_snapshot_items, [12]);
    let nested_requests = nested_server.requests();
    assert!(
        nested_requests[0]
            .target()
            .contains("/organizations/region-a/repositories/repository-b/entries")
    );
    assert!(
        nested_requests[1]
            .target()
            .contains("/organizations/region-b/repositories/repository-b/entries")
    );
    assert!(
        nested_requests[2]
            .target()
            .contains("/organizations/region-a/repositories/repository-b/entries")
    );
    let nested_safe = nested_hooks.before.lock().expect("nested safe capture");
    assert_eq!(nested_safe.len(), 3);
    assert!(nested_safe.iter().all(|safe| !safe.contains("region-a")));
    assert!(nested_safe.iter().all(|safe| !safe.contains("region-b")));
    assert!(nested_safe[0].contains("repository-b"));

    let collision_client = CollisionPaginationApi::new().expect("collision client");
    let _: concord_core::PaginatedRequest<_> =
        collision_client.left().items(0, nz32(1)).paginate(1_u32);
    let _: concord_core::PaginatedRequest<_> = collision_client
        .right()
        .items(nz64(1), nz32(1))
        .paginate(1_u32);

    let nested_macro_client =
        repeated_nested_expansion::NestedMacroPaginationApi::new().expect("nested macro client");
    let _: concord_core::PaginatedRequest<_> =
        nested_macro_client.items(0, nz32(1)).paginate(1_u32);
}

#[tokio::test]
async fn generated_pagination_preserves_resolved_rate_keys_and_actual_hosts() {
    use pagination_rate_api::prelude::*;

    const INITIAL: &str = "7319058410827319461";
    const NEXT: &str = "7319058410827319463";
    let server = ScriptedServer::start([
        ResponseScript::ok(br#"[1,2]"#.to_vec()).with_header("content-type", "application/json"),
        ResponseScript::ok(br#"[3]"#.to_vec()).with_header("content-type", "application/json"),
    ])
    .expect("rate pagination server");
    let proxy = SafeProxy::parse(server.url("")).expect("rate pagination proxy");
    let limiter = Arc::new(PaginationLimiterProbe::default());
    let configured_limiter: Arc<dyn RateLimiter> = limiter.clone();
    let hooks = Arc::new(PaginationHooks::default());
    let configured_hooks: Arc<dyn RuntimeHooks> = hooks.clone();
    let client = PaginationRateApi::builder()
        .http(move |http| http.proxy(proxy))
        .runtime(move |runtime| {
            runtime
                .hooks(configured_hooks)
                .rate_limiter(configured_limiter)
        })
        .build()
        .expect("rate pagination client");
    let items = client
        .items(INITIAL.parse().expect("offset"), nz32(2))
        .paginate(5_u32)
        .await
        .expect("rate pagination");
    assert_eq!(items, [1, 2, 3]);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].target().contains(INITIAL));
    assert!(requests[1].target().contains(NEXT));
    assert_eq!(requests[0].header("x-page-offset"), Some(INITIAL));
    assert_eq!(requests[1].header("x-page-offset"), Some(NEXT));
    let contexts = limiter.contexts.lock().expect("limiter contexts");
    assert_eq!(contexts.len(), 2);
    let observations = limiter.observations.lock().expect("limiter observations");
    assert_eq!(observations.as_slice(), contexts.as_slice());
    assert_eq!(
        contexts.iter().map(|context| context.0).collect::<Vec<_>>(),
        [0, 1]
    );
    let first_host = format!("{INITIAL}.pagination-rate.example.test");
    let next_host = format!("{NEXT}.pagination-rate.example.test");
    assert_eq!(contexts[0].2.as_deref(), Some(first_host.as_str()));
    assert_eq!(contexts[1].2.as_deref(), Some(next_host.as_str()));
    assert_ne!(contexts[0].2, contexts[1].2);
    assert_eq!(
        contexts[0].3,
        [
            ("host".to_owned(), first_host),
            ("ep.offset".to_owned(), INITIAL.to_owned()),
        ]
    );
    assert_eq!(
        contexts[1].3,
        [
            ("host".to_owned(), next_host),
            ("ep.offset".to_owned(), NEXT.to_owned()),
        ]
    );
    for (_, safe_url, _, parts) in contexts.iter() {
        let rendered_safe = format!("{safe_url}\n{:?}", hooks.before.lock().expect("hooks"));
        assert!(safe_url.contains("x.pagination-rate.example.test"));
        assert!(!rendered_safe.contains(INITIAL));
        assert!(!rendered_safe.contains(NEXT));
        assert!(parts.iter().all(|(_, value)| value != "<pagination>"));
    }
    assert_eq!(
        hooks
            .aggregate
            .lock()
            .expect("aggregate capture")
            .as_slice(),
        [(2, 3, false, Some(200))]
    );
    for safe in hooks.before.lock().expect("before capture").iter() {
        assert!(!safe.contains(INITIAL));
        assert!(!safe.contains(NEXT));
    }
    for safe in hooks.response.lock().expect("response capture").iter() {
        assert!(!safe.contains(INITIAL));
        assert!(!safe.contains(NEXT));
    }
    for safe in hooks.completion.lock().expect("completion capture").iter() {
        assert!(!safe.contains(INITIAL));
        assert!(!safe.contains(NEXT));
    }
}
