use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use concord_core::advanced::{
    BoxFuture, CodecError, DecodeBody, DecodeContext, HasNextCursor, HookError,
    PaginationController, PaginationError, PaginationRequest, RateLimitError, RateLimitPermit,
    RateLimitRequest, RateLimitResponse, RateLimiter, RuntimeHooks, SafeProxy,
};
use concord_core::prelude::Bytes;
use concord_macros::api;
use concord_test_support::server::{ResponseScript, ScriptedServer};

api! {
    client OffsetApi {
        base "http://pagination-offset.example.test/v1/";
        GET Items(offset: u64, limit: u32) -> Json<Vec<u32>> {
            path ["items"];
            query {
                "offset" = ep.offset;
                "limit" = ep.limit;
            }
            paginate OffsetPagination {
                offset = ep.offset,
                page_size = ep.limit
            }
        }
    }
}

api! {
    client PageApi {
        base "http://pagination-page.example.test/v1/";
        GET Items(page: u64, per_page: u32) -> Json<Vec<u32>> {
            path ["items"];
            query {
                "page" = ep.page;
                "per_page" = ep.per_page;
            }
            paginate PagePagination {
                page = ep.page,
                page_size = ep.per_page
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

#[derive(Clone)]
struct OneNonClonePage;

fn one_non_clone_page() -> OneNonClonePage {
    OneNonClonePage
}

impl PaginationController<Vec<NonCloneItem>> for OneNonClonePage {
    fn apply(&self, _request: &mut PaginationRequest<'_>) -> Result<(), PaginationError> {
        Ok(())
    }

    fn next(&self, _page: &Vec<NonCloneItem>) -> Result<Option<Self>, PaginationError> {
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
            paginate CursorPagination<String> {
                cursor = ep.cursor
            }
        }
    }
}

static CUSTOM_FACTORY_CALLS: AtomicUsize = AtomicUsize::new(0);
static CUSTOM_NEXT_CALLS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
pub struct VendorController {
    cursor: Option<String>,
    region: String,
}

pub fn vendor_controller(cursor: &Option<String>, region: &str) -> VendorController {
    CUSTOM_FACTORY_CALLS.fetch_add(1, Ordering::SeqCst);
    VendorController {
        cursor: cursor.clone(),
        region: region.to_owned(),
    }
}

impl PaginationController<VendorPage> for VendorController {
    fn apply(&self, request: &mut PaginationRequest<'_>) -> Result<(), PaginationError> {
        match &self.cursor {
            Some(cursor) => request.set_query("cursor", cursor)?,
            None => request.remove_query("cursor")?,
        }
        request.set_header(
            concord_core::advanced::HeaderName::from_static("x-region"),
            &self.region,
        )
    }

    fn next(&self, page: &VendorPage) -> Result<Option<Self>, PaginationError> {
        CUSTOM_NEXT_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(page.next.as_ref().map(|cursor| Self {
            cursor: Some(cursor.clone()),
            region: self.region.clone(),
        }))
    }
}

api! {
    client CustomApi {
        base "http://pagination-custom.example.test/v1/";
        GET Items(cursor: Option<String>, region: String) -> crate::VendorPageCodec {
            path ["items"];
            paginate custom crate::vendor_controller(
                ep.cursor,
                ep.region
            );
        }
    }
}

static ZERO_FACTORY_CALLS: AtomicUsize = AtomicUsize::new(0);

pub fn zero_controller() -> VendorController {
    ZERO_FACTORY_CALLS.fetch_add(1, Ordering::SeqCst);
    VendorController {
        cursor: None,
        region: "zero".to_owned(),
    }
}

api! {
    client ZeroApi {
        base "http://pagination-zero.example.test/v1/";
        GET Items() -> crate::VendorPageCodec {
            paginate custom crate::zero_controller();
        }
    }
}

api! {
    client NonCloneApi {
        base "http://pagination-non-clone.example.test/v1/";
        GET Items() -> crate::NonClonePageCodec {
            paginate custom crate::one_non_clone_page();
        }
    }
}

fn nested_controller(cursor: &Option<String>, region: &str) -> VendorController {
    VendorController {
        cursor: cursor.clone(),
        region: region.to_owned(),
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
                    paginate custom crate::nested_controller(ep.cursor, scope.region);
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
            GET Items(offset: u64, limit: u32) -> Json<Vec<u8>> {
                paginate OffsetPagination {
                    offset = ep.offset,
                    page_size = ep.limit
                }
            }
        }
        scope Right() {
            path ["right"];
            GET Items(page: u64, per_page: u32) -> Json<Vec<u8>> {
                paginate PagePagination {
                    page = ep.page,
                    page_size = ep.per_page
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
        GET Items(offset: u64, limit: u32) -> Json<Vec<u32>> {
            host [ep.offset];
            path ["items"];
            query {
                "offset" = ep.offset;
                "limit" = ep.limit;
            }
            rate_limit page_key;
            paginate OffsetPagination {
                offset = ep.offset,
                page_size = ep.limit
            }
        }
    }
}

mod repeated_nested_expansion {
    use concord_macros::api;

    api! {
        client NestedMacroPaginationApi {
            base "http://pagination-nested-macro.example.test/v1/";
            GET Items(offset: u64, limit: u32) -> Json<Vec<u8>> {
                paginate OffsetPagination {
                    offset = ep.offset,
                    page_size = ep.limit
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
        .items(0, 2)
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
        .items(1, 2)
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
        .items(1, 2)
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
async fn cursor_and_custom_short_pages_follow_controller_authority_and_redact_continuations() {
    use cursor_api::prelude::*;
    use custom_api::prelude::*;

    let cursor_server = ScriptedServer::start([
        ResponseScript::ok(b"1|next-cursor".to_vec())
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
    assert_eq!(cursor_items, [1, 2]);
    let cursor_requests = cursor_server.requests();
    assert_eq!(cursor_requests.len(), 2);
    assert!(!cursor_requests[0].target().contains("cursor="));
    assert!(cursor_requests[1].target().contains("cursor=next-cursor"));

    CUSTOM_FACTORY_CALLS.store(0, Ordering::SeqCst);
    CUSTOM_NEXT_CALLS.store(0, Ordering::SeqCst);
    let hooks = Arc::new(PaginationHooks::default());
    let configured_hooks: Arc<dyn RuntimeHooks> = hooks.clone();
    let custom_server = ScriptedServer::start([
        ResponseScript::ok(b"3|custom-cursor-sentinel-6d91".to_vec())
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
    assert_eq!(custom_items, [3, 4]);
    assert_eq!(CUSTOM_FACTORY_CALLS.load(Ordering::SeqCst), 1);
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
        assert!(!safe.contains("region-header-sentinel-b17"));
    }
    assert_eq!(
        hooks
            .aggregate
            .lock()
            .expect("aggregate capture")
            .as_slice(),
        [(2, 2, false, Some(200))]
    );
}

#[tokio::test]
async fn take_zero_skips_factory_and_page_work_but_emits_one_aggregate_success() {
    use zero_api::prelude::*;

    ZERO_FACTORY_CALLS.store(0, Ordering::SeqCst);
    let hooks = Arc::new(PaginationHooks::default());
    let configured_hooks: Arc<dyn RuntimeHooks> = hooks.clone();
    let client = ZeroApi::builder()
        .runtime(move |runtime| runtime.hooks(configured_hooks))
        .build()
        .expect("zero client");
    let items = client.items().paginate(0_u32).await.expect("zero pages");
    assert!(items.is_empty());
    assert_eq!(ZERO_FACTORY_CALLS.load(Ordering::SeqCst), 0);
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
        .items(u64::MAX, 2)
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
        .items(u64::MAX, 2)
        .paginate(concord_core::prelude::PageLimit::HardCap(
            std::num::NonZeroU32::MIN,
        ))
        .await
        .expect("short page naturally terminates at hard cap");
    assert_eq!(items, [6]);
    assert_eq!(page_server.requests().len(), 1);

    let invalid = PageApi::new()
        .expect("invalid-page client construction")
        .items(0, 2)
        .paginate(1_u32)
        .await
        .expect_err("zero page number is invalid");
    assert_eq!(
        invalid.pagination_kind(),
        Some(concord_core::advanced::PaginationErrorKind::InvalidLimit)
    );
}

#[tokio::test]
async fn empty_present_cursor_is_distinct_and_items_need_not_clone() {
    use cursor_api::prelude::*;
    use non_clone_api::prelude::*;

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
    let endpoint = offset_api::endpoints::Items::new(0, 2);
    let explicit_items = explicit_client
        .request(endpoint)
        .paginate(1_u32)
        .await
        .expect("explicit paginated endpoint");
    assert_eq!(explicit_items, [9]);

    let nested_server = ScriptedServer::start([ResponseScript::ok(b"11|-".to_vec())
        .with_header("content-type", "application/x-concord-page")])
    .expect("nested server");
    let nested_proxy = SafeProxy::parse(nested_server.url("")).expect("nested proxy");
    let nested_hooks = Arc::new(PaginationHooks::default());
    let configured_nested_hooks: Arc<dyn RuntimeHooks> = nested_hooks.clone();
    let nested_client = NestedPaginationApi::builder()
        .http(move |http| http.proxy(nested_proxy))
        .runtime(move |runtime| runtime.hooks(configured_nested_hooks))
        .build()
        .expect("nested client");
    let nested_items = nested_client
        .organizations("region-a".to_owned())
        .repositories("repository-b".to_owned())
        .fetch()
        .paginate(1_u32)
        .await
        .expect("nested aliased pagination");
    assert_eq!(nested_items, [11]);
    assert!(
        nested_server.requests()[0]
            .target()
            .contains("/organizations/region-a/repositories/repository-b/entries")
    );
    let nested_safe = nested_hooks.before.lock().expect("nested safe capture");
    assert_eq!(nested_safe.len(), 1);
    assert!(!nested_safe[0].contains("region-a"));
    assert!(nested_safe[0].contains("repository-b"));

    let collision_client = CollisionPaginationApi::new().expect("collision client");
    let _: concord_core::PaginatedRequest<_> = collision_client.left().items(0, 1).paginate(1_u32);
    let _: concord_core::PaginatedRequest<_> = collision_client.right().items(1, 1).paginate(1_u32);

    let nested_macro_client =
        repeated_nested_expansion::NestedMacroPaginationApi::new().expect("nested macro client");
    let _: concord_core::PaginatedRequest<_> = nested_macro_client.items(0, 1).paginate(1_u32);
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
        .items(INITIAL.parse().expect("offset"), 2)
        .paginate(5_u32)
        .await
        .expect("rate pagination");
    assert_eq!(items, [1, 2, 3]);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].target().contains(INITIAL));
    assert!(requests[1].target().contains(NEXT));
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
