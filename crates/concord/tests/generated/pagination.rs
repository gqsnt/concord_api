//! Generated pagination (§14) through downstream-generated v3 clients against
//! a loopback server: public `init`/`cursor` controllers with private state,
//! the three explicit traversal terminals (`pages`, `collect_prefix`,
//! `collect_all`), truthful traversal summaries, page-stable bodies, the
//! aggregate state machine and budgets, cancellation, and every built-in
//! controller.
//!
//! Migration notes (spec §25.2): every v2 `.paginate(PageLimit::Prefix(n))`
//! became `.collect_prefix(n)` (or `.pages(n)`) with summary assertions, every
//! `PageLimit::CompleteWithin(n)` became `.collect_all(n)`, and v2 public
//! controller-field bindings became `init Type { ... }; cursor -> slot;`
//! (PAGE-01). The v2 `PageResponseRepresentation` marker, the controller
//! `validate` hook and the request-representation `metadata` callback no
//! longer exist; the tests below assert their v3 replacements.

use std::cell::Cell;
use std::num::{NonZeroU32, NonZeroU64, NonZeroUsize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::body::{
    BodyError, BodyFactory, PageRequestRepresentation, PreparedBody, RequestBodyContext,
    RequestRepresentation, RequestStream,
};
use concord::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use concord::error::{ErrorCategory, FailureStage};
use concord::http::SafeProxy;
use concord::observation::{OperationEvent, OperationObserver};
use concord::pagination::{
    CollectAll, CollectPrefix, ContinuationKnowledge, CursorInit, CursorPagination, HasNextCursor,
    LinkCursorPagination, NextHeaderPagination, OffsetInit, OffsetPagination, PageDecisionLimits,
    PageInit, PagePagination, PageResponse, PageStep, Pages, PaginationController,
    PaginationDescription, PaginationError, PaginationErrorKind, StopReason, TraversalSummary,
};
use concord::rate_limit::{
    BoxFuture, Enforcement, LimiterCapability, RateLimitError, RateLimitPermit, RateLimitRequest,
    RateLimitResponse, RateLimiter,
};
use concord::response::{
    Buffered, ResponseBodyError, ResponseBodyErrorKind, ResponseHead, ResponseRepresentation,
};
use concord::retry::{RetryDecision, RetryFailure, RetryPolicy};
use concord::{
    Bytes, CallOptions, CancellationToken, DispatchKnowledge, RequestControls, Text, api,
};
use concord_test_support::server::{ResponseScript, ScriptedServer};

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

fn nz32(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).expect("positive u32")
}

fn nz64(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).expect("positive u64")
}

fn json(body: &str) -> ResponseScript {
    ResponseScript::ok(body.as_bytes().to_vec()).with_header("content-type", "application/json")
}

fn vendor(body: &str) -> ResponseScript {
    ResponseScript::ok(body.as_bytes().to_vec())
        .with_header("content-type", "application/x-concord-page")
}

fn proxy(server: &ScriptedServer) -> SafeProxy {
    SafeProxy::parse(server.url("")).expect("loopback proxy")
}

fn bounded_items<T>(items: Vec<T>, limits: PageDecisionLimits) -> Result<Vec<T>, PaginationError> {
    if items.len() > limits.remaining_items() {
        return Err(PaginationError::new(
            PaginationErrorKind::CollectedItemCeilingExceeded,
        ));
    }
    Ok(items)
}

/// Keeps a paused current-thread runtime busy so that Tokio's test clock
/// never auto-advances while a request waits on a real loopback socket; time
/// then moves only through explicit `tokio::time::advance` calls.
struct ClockGuard(tokio::task::JoinHandle<()>);

impl ClockGuard {
    fn start() -> Self {
        Self(tokio::spawn(async {
            loop {
                tokio::task::yield_now().await;
            }
        }))
    }
}

impl Drop for ClockGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Buffered JSON page representation owned by this test module. Its
/// spelling carries no macro semantics.
pub struct PageJson;

impl<T> ResponseRepresentation<T> for PageJson
where
    T: serde::de::DeserializeOwned + Send + 'static,
{
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = Some("application/json");

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        if head
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json"))
        {
            Ok(())
        } else {
            Err(ResponseBodyError::new(
                ResponseBodyErrorKind::InvalidMediaType,
            ))
        }
    }

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<T, ResponseBodyError> {
        serde_json::from_slice(&body)
            .map_err(|_| ResponseBodyError::new(ResponseBodyErrorKind::Decode))
    }
}

#[cfg(feature = "json")]
/// A `Vec<u32>` page accepted for any 2xx or 404 status, with exact media.
pub struct AcceptedSearchPageSpec;

#[cfg(feature = "json")]
impl ResponseRepresentation<Vec<u32>> for AcceptedSearchPageSpec {
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = Some("application/json");

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        if head
            .headers
            .get("content-type")
            .is_some_and(|value| value == "application/json")
        {
            Ok(())
        } else {
            Err(ResponseBodyError::new(
                ResponseBodyErrorKind::InvalidMediaType,
            ))
        }
    }

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<Vec<u32>, ResponseBodyError> {
        serde_json::from_slice(&body)
            .map_err(|_| ResponseBodyError::new(ResponseBodyErrorKind::Decode))
    }
}

/// A downstream buffered page representation with no iterable marker: any
/// `Buffered` representation is pageable by a controller that knows its page.
pub struct CustomPageSpec;

impl ResponseRepresentation<Vec<u32>> for CustomPageSpec {
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-custom-page");

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        if head
            .headers
            .get("content-type")
            .is_some_and(|value| value == "application/x-concord-custom-page")
        {
            Ok(())
        } else {
            Err(ResponseBodyError::new(
                ResponseBodyErrorKind::InvalidMediaType,
            ))
        }
    }

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<Vec<u32>, ResponseBodyError> {
        let text = std::str::from_utf8(&body)
            .map_err(|_error| ResponseBodyError::new(ResponseBodyErrorKind::Decode))?;
        if text.is_empty() {
            return Ok(Vec::new());
        }
        text.split(',')
            .map(|value| {
                value
                    .parse::<u32>()
                    .map_err(|_error| ResponseBodyError::new(ResponseBodyErrorKind::Decode))
            })
            .collect()
    }
}

/// A vendor page `items|next` where `-` means no next cursor.
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

pub struct VendorPageCodec;

impl DecodeBody for VendorPageCodec {
    type Output = VendorPage;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-page");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type == <Self as DecodeBody>::ACCEPT {
            Ok(())
        } else {
            Err(CodecError::new(CodecErrorKind::Decode))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let text =
            std::str::from_utf8(&body).map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        let (items, next) = text
            .split_once('|')
            .ok_or_else(|| CodecError::new(CodecErrorKind::Decode))?;
        let items = if items.is_empty() {
            Vec::new()
        } else {
            items
                .split(',')
                .map(|item| {
                    item.parse::<u32>()
                        .map_err(|_| CodecError::new(CodecErrorKind::Decode))
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let next = (next != "-").then(|| next.to_owned());
        Ok(VendorPage { items, next })
    }
}

/// A page item that is deliberately neither `Clone` nor `Copy`.
pub struct NonCloneItem(u32);

pub struct NonClonePageCodec;

impl DecodeBody for NonClonePageCodec {
    type Output = Vec<NonCloneItem>;

    const ACCEPT: Option<&'static str> = None;

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let text =
            std::str::from_utf8(&body).map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        text.split(',')
            .filter(|item| !item.is_empty())
            .map(|item| {
                item.parse::<u32>()
                    .map(NonCloneItem)
                    .map_err(|_| CodecError::new(CodecErrorKind::Decode))
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

#[cfg(feature = "json")]
/// A JSON search filter that counts each serialization.
#[derive(Clone)]
pub struct CountedSearchInput {
    value: String,
    encodes: Arc<AtomicUsize>,
}

#[cfg(feature = "json")]
impl serde::Serialize for CountedSearchInput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.encodes.fetch_add(1, Ordering::SeqCst);
        serializer.serialize_str(&self.value)
    }
}

/// A page-stable body whose preparation returns a reconstructible factory.
#[derive(Clone)]
pub struct RepeatableFactoryInput {
    bytes: Bytes,
    prepare_calls: Arc<AtomicUsize>,
    create_calls: Arc<AtomicUsize>,
}

pub struct RepeatableFactorySpec;

impl RequestRepresentation<RepeatableFactoryInput> for RepeatableFactorySpec {
    fn prepare(
        input: RepeatableFactoryInput,
        _context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        input.prepare_calls.fetch_add(1, Ordering::SeqCst);
        Ok(PreparedBody::factory(RepeatableFactory {
            bytes: input.bytes,
            create_calls: input.create_calls,
        }))
    }
}

impl PageRequestRepresentation<RepeatableFactoryInput> for RepeatableFactorySpec {}

struct RepeatableFactory {
    bytes: Bytes,
    create_calls: Arc<AtomicUsize>,
}

impl BodyFactory for RepeatableFactory {
    fn create(&self) -> Result<RequestStream, BodyError> {
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        Ok(RequestStream::from_bytes(self.bytes.clone()))
    }
}

// ---------------------------------------------------------------------------
// Downstream controllers: public Init, private state (PAGE-01, V3-P01)
// ---------------------------------------------------------------------------

/// Public initialization of [`StableCycleController`].
pub struct StableCycleInit {
    pub cursor: String,
}

/// Always proposes the same cursor again: an immediate repeat.
pub struct StableCycleController {
    cursor: String,
}

impl PaginationController<Vec<u32>> for StableCycleController {
    type Item = u32;
    type Init = StableCycleInit;
    type Cursor = String;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: StableCycleInit) -> Result<Self, PaginationError> {
        Ok(Self {
            cursor: init.cursor,
        })
    }

    fn cursor(&self) -> String {
        self.cursor.clone()
    }

    fn advance(
        self,
        page: PageResponse<Vec<u32>>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        Ok(PageStep::Continue {
            items: bounded_items(page.into_page(), limits)?,
            next: self,
        })
    }
}

/// Public initialization of [`RotatingCycleController`].
pub struct RotatingInit {
    pub cursor: String,
}

/// Walks `a -> b -> c -> a`: a longer cycle only visible on the third page.
pub struct RotatingCycleController {
    cursor: String,
}

impl PaginationController<Vec<u32>> for RotatingCycleController {
    type Item = u32;
    type Init = RotatingInit;
    type Cursor = String;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: RotatingInit) -> Result<Self, PaginationError> {
        Ok(Self {
            cursor: init.cursor,
        })
    }

    fn cursor(&self) -> String {
        self.cursor.clone()
    }

    fn advance(
        self,
        page: PageResponse<Vec<u32>>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let next = match self.cursor.as_str() {
            "a" => "b",
            "b" => "c",
            _ => "a",
        };
        Ok(PageStep::Continue {
            items: bounded_items(page.into_page(), limits)?,
            next: Self {
                cursor: next.to_owned(),
            },
        })
    }
}

static APPLICATION_BOUND_CLONES: AtomicUsize = AtomicUsize::new(0);
static APPLICATION_STABLE_CLONES: AtomicUsize = AtomicUsize::new(0);

/// An offset whose clones are counted.
pub struct CountedOffset(u64);

impl Clone for CountedOffset {
    fn clone(&self) -> Self {
        APPLICATION_BOUND_CLONES.fetch_add(1, Ordering::SeqCst);
        Self(self.0)
    }
}

impl concord::RequestAtom for CountedOffset {
    fn write_atom(
        &self,
        output: &mut concord::BoundedAtomWriter<'_>,
    ) -> Result<(), concord::AtomError> {
        output.write(&self.0)
    }
}

/// A client value whose clones are counted; it reaches the controller only
/// through the `init` clause.
pub struct StablePolicy(&'static str);

impl Clone for StablePolicy {
    fn clone(&self) -> Self {
        APPLICATION_STABLE_CLONES.fetch_add(1, Ordering::SeqCst);
        Self(self.0)
    }
}

/// Public initialization of [`ApplicationCountController`]: offsets, a client
/// value and stable literals.
pub struct ApplicationCountInit {
    pub offset: CountedOffset,
    pub page_size: NonZeroU32,
    pub policy: StablePolicy,
    pub attempts: u8,
    pub enabled: bool,
    pub label: &'static str,
}

/// Application-selected count termination with private state.
pub struct ApplicationCountController {
    offset: CountedOffset,
    page_size: NonZeroU32,
    policy: StablePolicy,
    attempts: u8,
    enabled: bool,
    label: &'static str,
}

impl PaginationController<Vec<u32>> for ApplicationCountController {
    type Item = u32;
    type Init = ApplicationCountInit;
    type Cursor = CountedOffset;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: ApplicationCountInit) -> Result<Self, PaginationError> {
        if init.attempts != 0 || !init.enabled || init.label != "count" || init.policy.0.is_empty()
        {
            return Err(PaginationError::new(PaginationErrorKind::Controller));
        }
        Ok(Self {
            offset: init.offset,
            page_size: init.page_size,
            policy: init.policy,
            attempts: init.attempts,
            enabled: init.enabled,
            label: init.label,
        })
    }

    fn cursor(&self) -> CountedOffset {
        self.offset.clone()
    }

    fn advance(
        self,
        response: PageResponse<Vec<u32>>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let items = bounded_items(response.into_page(), limits)?;
        if items.len() < self.page_size.get() as usize {
            return Ok(PageStep::Done { items });
        }
        let offset = self
            .offset
            .0
            .checked_add(u64::from(self.page_size.get()))
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        Ok(PageStep::Continue {
            items,
            next: Self {
                offset: CountedOffset(offset),
                page_size: self.page_size,
                policy: self.policy,
                attempts: self.attempts,
                enabled: self.enabled,
                label: self.label,
            },
        })
    }
}

static CUSTOM_START_CALLS: AtomicUsize = AtomicUsize::new(0);
static CUSTOM_CURSOR_CALLS: AtomicUsize = AtomicUsize::new(0);
static CUSTOM_ADVANCE_CALLS: AtomicUsize = AtomicUsize::new(0);

/// Public initialization of [`VendorController`].
pub struct VendorInit {
    pub cursor: Option<String>,
    pub region: String,
}

/// A vendor controller that follows the page's own cursor even for short
/// pages, carrying an initialization-only region in private state.
pub struct VendorController {
    cursor: Option<String>,
    region: String,
}

impl PaginationController<VendorPage> for VendorController {
    type Item = u32;
    type Init = VendorInit;
    type Cursor = Option<String>;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: VendorInit) -> Result<Self, PaginationError> {
        CUSTOM_START_CALLS.fetch_add(1, Ordering::SeqCst);
        if init.region.is_empty() {
            return Err(PaginationError::new(PaginationErrorKind::Controller));
        }
        Ok(Self {
            cursor: init.cursor,
            region: init.region,
        })
    }

    fn cursor(&self) -> Option<String> {
        CUSTOM_CURSOR_CALLS.fetch_add(1, Ordering::SeqCst);
        self.cursor.clone()
    }

    fn advance(
        self,
        page: PageResponse<VendorPage>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        CUSTOM_ADVANCE_CALLS.fetch_add(1, Ordering::SeqCst);
        let next = page.page().next.as_ref().map(|cursor| Self {
            cursor: Some(cursor.clone()),
            region: self.region,
        });
        let items = bounded_items(page.into_page().items, limits)?;
        Ok(match next {
            Some(next) => PageStep::Continue { items, next },
            None => PageStep::Done { items },
        })
    }
}

/// Public initialization of [`ScopeBindingController`] from endpoint and
/// ancestor-scope values.
pub struct ScopeBindingInit {
    pub cursor: Option<String>,
    pub region: String,
    pub repository: String,
}

pub struct ScopeBindingController {
    cursor: Option<String>,
    region: String,
    repository: String,
}

impl PaginationController<VendorPage> for ScopeBindingController {
    type Item = u32;
    type Init = ScopeBindingInit;
    type Cursor = Option<String>;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: ScopeBindingInit) -> Result<Self, PaginationError> {
        if init.region != "region-a" || init.repository != "repository-b" {
            return Err(PaginationError::new(PaginationErrorKind::Controller));
        }
        Ok(Self {
            cursor: init.cursor,
            region: init.region,
            repository: init.repository,
        })
    }

    fn cursor(&self) -> Option<String> {
        self.cursor.clone()
    }

    fn advance(
        self,
        page: PageResponse<VendorPage>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let next = page.page().next.as_ref().map(|next| Self {
            cursor: Some(next.clone()),
            region: self.region,
            repository: self.repository,
        });
        let items = bounded_items(page.into_page().items, limits)?;
        Ok(match next {
            Some(next) => PageStep::Continue { items, next },
            None => PageStep::Done { items },
        })
    }
}

/// Public initialization of [`OneNonClonePage`]: the marker is `Send` but not
/// `Sync`, and nothing about it is `Clone`.
pub struct OneNonCloneInit {
    pub marker: Cell<()>,
}

/// A single-page controller that is `Send` but not `Sync`.
pub struct OneNonClonePage {
    marker: Cell<()>,
}

#[allow(
    clippy::declare_interior_mutable_const,
    reason = "the fixture intentionally constructs a Send but non-Sync controller"
)]
pub const NOT_SYNC_MARKER: Cell<()> = Cell::new(());

impl PaginationController<Vec<NonCloneItem>> for OneNonClonePage {
    type Item = NonCloneItem;
    type Init = OneNonCloneInit;
    type Cursor = Option<String>;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: OneNonCloneInit) -> Result<Self, PaginationError> {
        Ok(Self {
            marker: init.marker,
        })
    }

    fn cursor(&self) -> Option<String> {
        None
    }

    fn advance(
        self,
        page: PageResponse<Vec<NonCloneItem>>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let _marker = self.marker;
        Ok(PageStep::Done {
            items: bounded_items(page.into_page(), limits)?,
        })
    }
}

// ---------------------------------------------------------------------------
// Generated clients
// ---------------------------------------------------------------------------

#[cfg(feature = "json")]
api! {
    client JsonBodyPaginationApi {
        timeout 30s;
        base "http://pagination-body.example.test/v1/";

        POST json_pages(
            query offset: u64,
            query limit: NonZeroU32,
            body filter: CountedSearchInput via concord::Json,
        ) -> Vec<u32> via AcceptedSearchPageSpec {
            status [200..=299, 404];
            path "json-pages";
            effect safe;
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: limit };
                cursor -> offset;
            }
        }
    }
}

api! {
    client BodyPaginationApi {
        timeout 30s;
        base "http://pagination-body.example.test/v1/";

        POST factory_pages(
            query offset: u64,
            query limit: NonZeroU32,
            body payload: RepeatableFactoryInput via RepeatableFactorySpec,
        ) -> Vec<u32> via PageJson {
            path "factory-pages";
            effect safe;
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: limit };
                cursor -> offset;
            }
        }

        POST stable_cycle(query cursor: String, body payload: String via Text) -> Vec<u32> via PageJson {
            effect safe;
            path "stable-cycle";
            paginate via StableCycleController {
                init StableCycleInit { cursor: cursor };
                cursor -> cursor;
            }
        }
    }
}

api! {
    client OffsetApi {
        timeout 30s;
        base "http://pagination-offset.example.test/v1/";

        GET items(query offset: u64, query limit: NonZeroU32) -> Vec<u32> via PageJson {
            path "items";
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: limit };
                cursor -> offset;
            }
        }

        /// The Init deliberately names a different first offset than the slot.
        GET mismatched(query offset: u64 = 0, query limit: NonZeroU32) -> Vec<u32> via PageJson {
            path "mismatched";
            paginate via OffsetPagination {
                init OffsetInit { offset: 5, page_size: limit };
                cursor -> offset;
            }
        }

        GET rotating(query cursor: String) -> Vec<u32> via PageJson {
            path "rotating";
            paginate via RotatingCycleController {
                init RotatingInit { cursor: cursor };
                cursor -> cursor;
            }
        }
    }
}

api! {
    client PageApi {
        timeout 30s;
        base "http://pagination-page.example.test/v1/";

        GET items(query page: NonZeroU64, query per_page: NonZeroU32) -> Vec<u32> via PageJson {
            path "items";
            paginate via PagePagination {
                init PageInit { page: page, page_size: per_page };
                cursor -> page;
            }
        }
    }
}

api! {
    client ApplicationCountApi {
        timeout 30s;
        base "http://pagination-application-count.example.test/v1/";

        values {
            policy: StablePolicy;
        }

        GET items(query offset: CountedOffset, query limit: NonZeroU32) -> Vec<u32> via PageJson {
            path "items";
            paginate via ApplicationCountController {
                init ApplicationCountInit {
                    offset: offset,
                    page_size: limit,
                    policy: client.policy,
                    attempts: 0,
                    enabled: true,
                    label: "count",
                };
                cursor -> offset;
            }
        }
    }
}

api! {
    client CustomResponsePageApi {
        timeout 30s;
        base "http://pagination-custom-response.example.test/v1/";

        GET items(query offset: u64, query limit: NonZeroU32) -> Vec<u32> via CustomPageSpec {
            path "items";
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: limit };
                cursor -> offset;
            }
        }
    }
}

api! {
    client CursorApi {
        timeout 30s;
        base "http://pagination-cursor.example.test/v1/";

        GET items(query cursor?: String) -> VendorPage via VendorPageCodec {
            path "items";
            paginate via CursorPagination<String> {
                init CursorInit { cursor: cursor };
                cursor -> cursor;
            }
        }
    }
}

api! {
    client CustomApi {
        timeout 30s;
        base "http://pagination-custom.example.test/v1/";

        GET items(query cursor?: String, header("x-region") region: String) -> VendorPage via VendorPageCodec {
            path "items";
            paginate via VendorController {
                init VendorInit { cursor: cursor, region: region };
                cursor -> cursor;
            }
        }
    }
}

api! {
    client NonCloneApi {
        timeout 30s;
        base "http://pagination-non-clone.example.test/v1/";

        GET items(query marker?: String) -> Vec<NonCloneItem> via NonClonePageCodec {
            paginate via OneNonClonePage {
                init OneNonCloneInit { marker: NOT_SYNC_MARKER };
                cursor -> marker;
            }
        }
    }
}

api! {
    client NestedPaginationApi {
        timeout 30s;
        base "http://pagination-nested.example.test/v1/";

        scope organizations(#[sensitive] region: String) {
            path "organizations/{region}";

            scope repositories(repository: String) {
                path "repositories/{repository}";

                GET fetch(query cursor?: String) -> VendorPage via VendorPageCodec {
                    path "entries";
                    headers { "x-region" = organizations.region; }
                    paginate via ScopeBindingController {
                        init ScopeBindingInit {
                            cursor: cursor,
                            region: organizations.region,
                            repository: repositories.repository,
                        };
                        cursor -> cursor;
                    }
                }
            }
        }
    }
}

api! {
    client CollisionPaginationApi {
        timeout 30s;
        base "http://pagination-collision.example.test/v1/";

        scope left() {
            path "left";

            GET items(query offset: u64, query limit: NonZeroU32) -> Vec<u8> via PageJson {
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, page_size: limit };
                    cursor -> offset;
                }
            }
        }

        scope right() {
            path "right";

            GET items(query page: NonZeroU64, query per_page: NonZeroU32) -> Vec<u8> via PageJson {
                paginate via PagePagination {
                    init PageInit { page: page, page_size: per_page };
                    cursor -> page;
                }
            }
        }
    }
}

api! {
    client PaginationRateApi {
        timeout 30s;
        base "http://pagination-rate.example.test/v1/";

        GET items(offset: u64, query limit: NonZeroU32) -> Vec<u32> via PageJson {
            id "pagination.rate.items";
            path "items/{offset}";
            query { "offset" = offset; }
            headers { "X-Page-Offset" = offset; }
            rate_limit { id "concord.pagination.1"; key [endpoint]; cost 1; allow 100 per 1s; }
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: limit };
                cursor -> offset;
            }
        }
    }
}

api! {
    client BuiltInCursorApi {
        timeout 30s;
        base "http://pagination-built-in.example.test/v1/";

        GET headed(query cursor?: String) -> Vec<u32> via PageJson {
            path "headed";
            paginate via NextHeaderPagination {
                init CursorInit { cursor: cursor };
                cursor -> cursor;
            }
        }

        GET linked(query cursor?: String, query filter: String = "blue") -> Vec<u32> via PageJson {
            path "linked";
            paginate via LinkCursorPagination {
                init CursorInit { cursor: cursor };
                cursor -> cursor;
            }
        }
    }
}

mod repeated_nested_expansion {
    use std::num::NonZeroU32;

    use concord::api;
    use concord::pagination::{OffsetInit, OffsetPagination};

    use super::PageJson;

    api! {
        client NestedMacroPaginationApi {
            timeout 30s;
            base "http://pagination-nested-macro.example.test/v1/";

            GET items(query offset: u64, query limit: NonZeroU32) -> Vec<u8> via PageJson {
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, page_size: limit };
                    cursor -> offset;
                }
            }
        }
    }
}

#[cfg(feature = "json")]
mod envelope {
    //! The §17.2 downstream cursor-envelope controller over first-party
    //! `Json`: the envelope implements no `IntoIterator` (V3-P01).

    use concord::api;
    use concord::pagination::{
        PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
        PaginationError, PaginationErrorKind,
    };

    #[derive(serde::Deserialize)]
    pub struct Envelope<T> {
        pub data: Vec<T>,
        pub next: Option<String>,
    }

    /// Public initialization of [`EnvelopePages`].
    pub struct CursorStart {
        pub after: Option<String>,
    }

    /// Private controller state.
    pub struct EnvelopePages {
        after: Option<String>,
    }

    impl<T: Send + 'static> PaginationController<Envelope<T>> for EnvelopePages {
        type Item = T;
        type Init = CursorStart;
        type Cursor = Option<String>;

        const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

        fn start(init: Self::Init) -> Result<Self, PaginationError> {
            Ok(Self { after: init.after })
        }

        fn cursor(&self) -> Self::Cursor {
            self.after.clone()
        }

        fn advance(
            self,
            response: PageResponse<Envelope<T>>,
            limits: PageDecisionLimits,
        ) -> Result<PageStep<Self, T>, PaginationError> {
            let Envelope { data, next } = response.into_page();
            if data.len() > limits.remaining_items() {
                return Err(PaginationError::new(
                    PaginationErrorKind::CollectedItemCeilingExceeded,
                ));
            }
            match next {
                None => Ok(PageStep::Done { items: data }),
                Some(cursor) => {
                    if cursor.is_empty() || self.after.as_ref() == Some(&cursor) {
                        return Err(PaginationError::new(
                            PaginationErrorKind::InvalidContinuation,
                        ));
                    }
                    Ok(PageStep::Continue {
                        next: Self {
                            after: Some(cursor),
                        },
                        items: data,
                    })
                }
            }
        }
    }

    api! {
        client EnvelopeApi {
            timeout 30s;
            base "http://pagination-envelope.example.test/v1/";
            response concord::Json;

            GET feed(query after?: String) -> Envelope<u32> {
                path "feed";
                paginate via EnvelopePages {
                    init CursorStart { after: after };
                    cursor -> after;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Observation and limiter probes
// ---------------------------------------------------------------------------

type AggregateRecord = (u32, usize, bool, Option<u16>, StopReason);
type LimiterContext = (u32, Option<String>, Vec<(String, Vec<u8>)>);

#[derive(Default)]
struct PaginationHooks {
    before: Mutex<Vec<String>>,
    response: Mutex<Vec<String>>,
    completion: Mutex<Vec<String>>,
    aggregate: Mutex<Vec<AggregateRecord>>,
}

impl OperationObserver for PaginationHooks {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::AttemptStarted(context) => {
                self.before
                    .lock()
                    .expect("before lock")
                    .push(format!("{context:?}"));
            }
            OperationEvent::ResponseHeadObserved(context) => {
                self.response
                    .lock()
                    .expect("response lock")
                    .push(format!("{context:?}"));
            }
            OperationEvent::OperationFinished(context) => {
                self.completion
                    .lock()
                    .expect("completion lock")
                    .push(format!("{context:?}"));
            }
            OperationEvent::PaginationFinished(context) => {
                self.aggregate.lock().expect("aggregate lock").push((
                    context.pages_completed,
                    context.items_collected,
                    context.recovered_authentication,
                    context.final_status.map(|status| status.as_u16()),
                    context.stop_reason,
                ));
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct PaginationLimiterProbe {
    contexts: Mutex<Vec<LimiterContext>>,
    observations: Mutex<Vec<LimiterContext>>,
}

fn limiter_context(request: &RateLimitRequest<'_>) -> LimiterContext {
    let parts = request
        .plan
        .policies()
        .iter()
        .flat_map(|rule| rule.key().parts())
        .map(|part| (part.name().to_owned(), part.value().to_owned()))
        .collect();
    (
        request.page_index,
        request.logical_origin.map(str::to_owned),
        parts,
    )
}

impl RateLimiter for PaginationLimiterProbe {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "concord.test.pagination-probe", 1)
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.contexts
            .lock()
            .expect("limiter contexts")
            .push(limiter_context(&request));
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe(&self, response: RateLimitResponse<'_>) {
        self.observations
            .lock()
            .expect("limiter observations")
            .push(limiter_context(&response.request));
    }
}

#[cfg(feature = "json")]
#[derive(Default)]
struct BodyPaginationProbe {
    before: AtomicUsize,
    responses: AtomicUsize,
    completions: AtomicUsize,
    aggregates: AtomicUsize,
    acquisitions: AtomicUsize,
    observations: AtomicUsize,
}

#[cfg(feature = "json")]
impl RateLimiter for BodyPaginationProbe {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "concord.test.body-pagination", 1)
    }

    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe(&self, _response: RateLimitResponse<'_>) {
        self.observations.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(feature = "json")]
impl OperationObserver for BodyPaginationProbe {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::AttemptStarted(_) => {
                self.before.fetch_add(1, Ordering::SeqCst);
            }
            OperationEvent::ResponseHeadObserved(_) => {
                self.responses.fetch_add(1, Ordering::SeqCst);
            }
            OperationEvent::OperationFinished(_) => {
                self.completions.fetch_add(1, Ordering::SeqCst);
            }
            OperationEvent::PaginationFinished(_) => {
                self.aggregates.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

fn retry_rejected_statuses() -> RetryPolicy {
    RetryPolicy::builder()
        .without_budget()
        .max_retries_per_operation(1)
        .classify_fn(|attempt| match attempt.failure() {
            RetryFailure::RejectedStatus { .. } => RetryDecision::Retry,
            _ => RetryDecision::Stop,
        })
        .build()
        .expect("bounded page retry policy")
}

fn offset_client(server: &ScriptedServer) -> OffsetApi {
    let proxy = proxy(server);
    OffsetApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("offset pagination client")
}

fn assert_summary(
    summary: &TraversalSummary,
    stop: StopReason,
    continuation: ContinuationKnowledge,
    received: u32,
    delivered: u32,
    items: usize,
) {
    assert_eq!(summary.stop_reason(), stop, "stop reason");
    assert_eq!(
        summary.continuation(),
        continuation,
        "continuation knowledge"
    );
    assert_eq!(summary.pages_received(), received, "pages received");
    assert_eq!(summary.pages_delivered(), delivered, "pages delivered");
    assert_eq!(summary.items_delivered(), items, "items delivered");
}

// ---------------------------------------------------------------------------
// Page-stable bodies and policy retry
// ---------------------------------------------------------------------------

#[cfg(feature = "json")]
#[tokio::test]
async fn repeatable_json_body_is_prepared_once_per_page_and_reused_for_policy_retry() {
    let server = ScriptedServer::start([
        ResponseScript::status(503, b"retry".to_vec()),
        json("[1,2]"),
        ResponseScript::status(404, br#"[3]"#.to_vec())
            .with_header("content-type", "application/json"),
    ])
    .expect("repeatable JSON pagination server");
    let proxy = proxy(&server);
    let probe = Arc::new(BodyPaginationProbe::default());
    let limiter: Arc<dyn RateLimiter> = probe.clone();
    let observation: Arc<dyn OperationObserver> = probe.clone();
    let payload = "x".repeat(2_048);
    let encoded = format!("\"{payload}\"");
    let body_limit = NonZeroUsize::new(encoded.len()).expect("positive JSON body limit");
    // The page-stable body is not part of progress identity, so an identity
    // ceiling far below the body size still admits the traversal.
    let identity_limit =
        NonZeroUsize::new(1_024).expect("positive identity limit below stable body bytes");
    let client = JsonBodyPaginationApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .retry(retry_rejected_statuses())
        .configure_runtime(move |runtime| {
            runtime
                .operation_observer(observation)
                .rate_limiter(limiter)
                .request_body_limit(body_limit.get())
                .pagination_identity_limit(identity_limit.get())
        })
        .build()
        .expect("repeatable JSON pagination client");
    let encodes = Arc::new(AtomicUsize::new(0));

    let prefix = client
        .json_pages(
            0,
            nz32(2),
            CountedSearchInput {
                value: payload,
                encodes: Arc::clone(&encodes),
            },
        )
        .collect_prefix(3)
        .await
        .expect("body pagination including an accepted non-2xx page");

    assert_eq!(prefix.items(), [1, 2, 3]);
    let summary = prefix.summary();
    // The short accepted 404 page proves exhaustion within the bound.
    assert_summary(
        summary,
        StopReason::Exhausted,
        ContinuationKnowledge::Exhausted,
        2,
        2,
        3,
    );
    assert_eq!(summary.requested_page_bound(), 3);
    assert_eq!(summary.application_attempts(), 3);
    assert_eq!(summary.automatic_replays(), 1);
    assert_eq!(summary.quota_admissions(), 3);
    assert_eq!(
        summary.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert!(summary.operation_id().is_some());
    // One preparation per page; the policy retry reuses the prepared bytes.
    assert_eq!(encodes.load(Ordering::SeqCst), 2);
    assert_eq!(probe.before.load(Ordering::SeqCst), 3);
    assert_eq!(probe.responses.load(Ordering::SeqCst), 3);
    assert_eq!(probe.acquisitions.load(Ordering::SeqCst), 3);
    assert_eq!(probe.observations.load(Ordering::SeqCst), 3);
    // Each page is a complete child operation with its own terminal
    // observation in addition to the one aggregate pagination event.
    assert_eq!(probe.completions.load(Ordering::SeqCst), 2);
    assert_eq!(probe.aggregates.load(Ordering::SeqCst), 1);

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].target().contains("offset=0&limit=2"));
    assert!(requests[1].target().contains("offset=0&limit=2"));
    assert!(requests[2].target().contains("offset=2&limit=2"));
    assert!(
        requests
            .iter()
            .all(|request| request.body() == encoded.as_bytes())
    );
    assert!(
        requests
            .iter()
            .all(|request| request.header("content-type") == Some("application/json"))
    );
}

#[tokio::test]
async fn custom_page_body_factory_opts_in_and_reconstructs_once_per_visible_attempt() {
    let server =
        ScriptedServer::start([ResponseScript::status(503, b"retry".to_vec()), json("[9]")])
            .expect("repeatable factory pagination server");
    let proxy = proxy(&server);
    let client = BodyPaginationApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .retry(retry_rejected_statuses())
        .build()
        .expect("repeatable factory pagination client");
    let prepare_calls = Arc::new(AtomicUsize::new(0));
    let create_calls = Arc::new(AtomicUsize::new(0));
    let stable_body = Bytes::from_static(b"repeatable-factory-page-body");
    let input = RepeatableFactoryInput {
        bytes: stable_body.clone(),
        prepare_calls: Arc::clone(&prepare_calls),
        create_calls: Arc::clone(&create_calls),
    };

    // v2's separate `metadata` callback is gone: `PreparedBody` is the single
    // body authority, and planning never prepares a body (EXEC-03).
    let plan = client
        .factory_pages(0, nz32(2), input.clone())
        .plan()
        .expect("body-free plan");
    assert!(plan.observed_url().contains("offset=0&limit=2"));
    assert_eq!(prepare_calls.load(Ordering::SeqCst), 0);
    assert_eq!(create_calls.load(Ordering::SeqCst), 0);

    let items = client
        .factory_pages(0, nz32(2), input)
        .collect_all(nz32(2))
        .await
        .expect("custom repeatable factory page");

    assert_eq!(items, [9]);
    assert_eq!(prepare_calls.load(Ordering::SeqCst), 1);
    assert_eq!(create_calls.load(Ordering::SeqCst), 2);
    assert_eq!(server.request_count(), 2);
    assert!(
        server
            .requests()
            .iter()
            .all(|request| request.body() == stable_body.as_ref())
    );
}

// ---------------------------------------------------------------------------
// Progress identity: initial cursor, immediate and longer cycles (V3-P03)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stable_body_does_not_hide_repeated_public_page_identity() {
    let server = ScriptedServer::start([json("[1]")]).expect("stable-body cycle server");
    let proxy = proxy(&server);
    let client = BodyPaginationApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("stable-body cycle client");

    let error = client
        .stable_cycle("same", "stable request body".to_owned())
        .collect_prefix(2)
        .await
        .expect_err("repeated public page identity must be rejected");

    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::RepeatedRequest)
    );
    assert_eq!(error.stage(), FailureStage::PaginationDecision);
    assert_eq!(server.request_count(), 1);
    let summary = error.traversal_summary().expect("traversal summary");
    // The page was received, but its batch is withheld because the proposed
    // continuation repeats it.
    assert_summary(
        summary,
        StopReason::Failed,
        ContinuationKnowledge::Unknown,
        1,
        0,
        0,
    );
}

#[tokio::test]
async fn longer_cycle_is_rejected_before_the_closing_page_is_delivered() {
    let server = ScriptedServer::start([json("[1]"), json("[2]"), json("[3]")])
        .expect("rotating cycle server");
    let client = offset_client(&server);

    let mut pages = client.rotating("a").pages(10);
    let first = pages.next_page().await.expect("page a").expect("page a");
    assert_eq!((first.index(), first.items()), (0, [1].as_slice()));
    let second = pages.next_page().await.expect("page b").expect("page b");
    assert_eq!((second.index(), second.items()), (1, [2].as_slice()));
    let error = pages
        .next_page()
        .await
        .expect_err("c -> a closes a three-request cycle");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::RepeatedRequest)
    );
    // Terminal errors are returned exactly once.
    assert!(pages.next_page().await.expect("after failure").is_none());
    assert_summary(
        &pages.summary(),
        StopReason::Failed,
        ContinuationKnowledge::Unknown,
        3,
        2,
        2,
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].target().ends_with("rotating?cursor=a"));
    assert!(requests[1].target().ends_with("rotating?cursor=b"));
    assert!(requests[2].target().ends_with("rotating?cursor=c"));
}

#[tokio::test]
async fn initial_cursor_must_reproduce_the_selected_request_before_any_send() {
    let server = ScriptedServer::start([json("[4]")]).expect("initial cursor server");
    let client = offset_client(&server);

    let error = client
        .mismatched(nz32(2))
        .collect_prefix(3)
        .await
        .expect_err("Init offset 5 does not reproduce the selected offset 0");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::InvalidInitialCursor)
    );
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(server.request_count(), 0);
    let summary = error.traversal_summary().expect("traversal summary");
    assert!(summary.operation_id().is_some());
    assert_summary(
        summary,
        StopReason::Failed,
        ContinuationKnowledge::Unknown,
        0,
        0,
        0,
    );
    assert_eq!(summary.application_attempts(), 0);

    // Selecting the slot the Init names makes the identities agree.
    let prefix = client
        .mismatched(nz32(2))
        .set_offset(5_u64)
        .collect_prefix(3)
        .await
        .expect("matching initial cursor");
    assert_eq!(prefix.items(), [4]);
    assert!(server.requests()[0].target().contains("offset=5&limit=2"));
}

// ---------------------------------------------------------------------------
// Terminals and truthful summaries (V3-P04, V3-P05)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn generated_offset_pagination_collects_only_after_exhaustion() {
    let server = ScriptedServer::start([json("[1,2]"), json("[3]"), json("[1,2]")])
        .expect("pagination server");
    let client = offset_client(&server);

    let items = client
        .items(0, nz32(2))
        .collect_all(nz32(10))
        .await
        .expect("complete traversal");
    assert_eq!(items, [1, 2, 3]);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].target().contains("offset=0&limit=2"));
    assert!(requests[1].target().contains("offset=2&limit=2"));

    // CALL-03: an ordinary await returns one decoded page and never starts
    // pagination, even when that page is full.
    let first_page = client.items(0, nz32(2)).await.expect("one page");
    assert_eq!(first_page, [1, 2]);
    assert_eq!(server.request_count(), 3);
}

#[tokio::test]
async fn downstream_page_response_capability_requires_no_macro_semantics() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"1,2".to_vec())
            .with_header("content-type", "application/x-concord-custom-page"),
        ResponseScript::ok(b"3".to_vec())
            .with_header("content-type", "application/x-concord-custom-page"),
    ])
    .expect("custom page response server");
    let proxy = proxy(&server);
    let client = CustomResponsePageApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("custom page response client");

    let items = client
        .items(0, nz32(2))
        .collect_all(nz32(10))
        .await
        .expect("downstream page response specification");
    assert_eq!(items, [1, 2, 3]);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests.iter().all(|request| {
            request.header("accept") == Some("application/x-concord-custom-page")
        })
    );
}

#[tokio::test]
async fn application_controller_selects_the_same_count_termination_without_macro_privilege() {
    let server =
        ScriptedServer::start([json("[1,2]"), json("[3]")]).expect("application count server");
    let proxy = proxy(&server);
    let client = ApplicationCountApi::builder()
        .set_policy(StablePolicy("stable-policy"))
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("application count client");

    let collection = client
        .items(CountedOffset(0), nz32(2))
        .collect_all(nz32(10));
    APPLICATION_BOUND_CLONES.store(0, Ordering::SeqCst);
    APPLICATION_STABLE_CLONES.store(0, Ordering::SeqCst);
    let items = collection
        .await
        .expect("application-selected count termination");
    assert_eq!(items, [1, 2, 3]);
    // Only the roles that need `Clone` clone: the Init reference to the
    // offset slot, the initial cursor and the one continuation cursor. Page
    // copies share the retained slot payloads instead of cloning them.
    assert_eq!(APPLICATION_BOUND_CLONES.load(Ordering::SeqCst), 3);
    // The client value is cloned once, into the Init.
    assert_eq!(APPLICATION_STABLE_CLONES.load(Ordering::SeqCst), 1);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].target().contains("offset=0&limit=2"));
    assert!(requests[1].target().contains("offset=2&limit=2"));
}

#[tokio::test]
async fn page_number_prefix_bound_and_complete_limit_are_distinct() {
    let prefix_server =
        ScriptedServer::start([json("[1,2]"), json("[3,4]")]).expect("prefix server");
    let prefix_proxy = proxy(&prefix_server);
    let prefix_client = PageApi::builder()
        .configure_http(move |http| http.proxy(prefix_proxy))
        .build()
        .expect("page client");
    let prefix = prefix_client
        .items(nz64(1), nz32(2))
        .collect_prefix(2)
        .await
        .expect("explicit prefix");
    assert_eq!(prefix.items(), [1, 2, 3, 4]);
    // The bound was reached while a valid continuation existed: this prefix
    // is explicitly not evidence of exhaustion.
    assert_summary(
        prefix.summary(),
        StopReason::PageBound,
        ContinuationKnowledge::NextRequestKnown,
        2,
        2,
        4,
    );
    assert_eq!(prefix_server.requests().len(), 2);
    assert!(
        prefix_server.requests()[1]
            .target()
            .contains("page=2&per_page=2")
    );

    let complete_server = ScriptedServer::start([json("[7,8]")]).expect("complete server");
    let complete_proxy = proxy(&complete_server);
    let complete_client = PageApi::builder()
        .configure_http(move |http| http.proxy(complete_proxy))
        .build()
        .expect("page client");
    let error = complete_client
        .items(nz64(1), nz32(2))
        .collect_all(NonZeroU32::MIN)
        .await
        .expect_err("continuation beyond the complete bound");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::CompletionLimitExceeded)
    );
    let summary = error.traversal_summary().expect("completion-limit summary");
    // No success Vec with an undisclosed prefix: the page was received but
    // never delivered.
    assert_summary(
        summary,
        StopReason::PageBound,
        ContinuationKnowledge::NextRequestKnown,
        1,
        0,
        0,
    );
    assert_eq!(complete_server.requests().len(), 1);
}

#[tokio::test]
async fn built_in_overflow_is_ignored_on_short_stop_and_exact_bound_exhaustion_succeeds() {
    let offset_server = ScriptedServer::start([json("[5]")]).expect("offset overflow server");
    let offset_client = offset_client(&offset_server);
    let prefix = offset_client
        .items(u64::MAX, nz32(2))
        .collect_prefix(2)
        .await
        .expect("short page overrides speculative offset overflow");
    assert_eq!(prefix.items(), [5]);
    assert_summary(
        prefix.summary(),
        StopReason::Exhausted,
        ContinuationKnowledge::Exhausted,
        1,
        1,
        1,
    );
    assert_eq!(offset_server.requests().len(), 1);

    let page_server = ScriptedServer::start([json("[6]")]).expect("exact-bound page server");
    let page_proxy = proxy(&page_server);
    let page_client = PageApi::builder()
        .configure_http(move |http| http.proxy(page_proxy))
        .build()
        .expect("exact-bound page client");
    // `Done` on the final permitted page is exhaustion: no extra fetch.
    let items = page_client
        .items(nz64(u64::MAX), nz32(2))
        .collect_all(NonZeroU32::MIN)
        .await
        .expect("short page naturally terminates at the complete bound");
    assert_eq!(items, [6]);
    assert_eq!(page_server.requests().len(), 1);
}

#[tokio::test]
async fn lazy_pages_distinguish_exhaustion_bound_and_application_stop() {
    let server = ScriptedServer::start([json("[1,2]"), json("[3]"), json("[1,2]"), json("[1,2]")])
        .expect("lazy pages server");
    let client = offset_client(&server);

    // Exhaustion: the short page is Done; later calls stay None and finish
    // preserves Exhausted.
    let mut exhausted = client.items(0, nz32(2)).pages(5);
    assert_eq!(exhausted.summary().stop_reason(), StopReason::NotStarted);
    assert!(exhausted.summary().operation_id().is_none());
    let first = exhausted
        .next_page()
        .await
        .expect("page 0")
        .expect("page 0");
    assert_eq!(first.index(), 0);
    assert_eq!(first.metadata().page_index(), 0);
    assert_eq!(exhausted.summary().stop_reason(), StopReason::Active);
    assert_eq!(
        exhausted.summary().continuation(),
        ContinuationKnowledge::NextRequestKnown
    );
    let second = exhausted
        .next_page()
        .await
        .expect("page 1")
        .expect("page 1");
    assert_eq!((second.index(), second.into_items()), (1, vec![3]));
    // The first page's metadata is a fixed snapshot.
    assert_eq!(first.metadata().page_index(), 0);
    assert!(exhausted.next_page().await.expect("exhausted").is_none());
    let summary = exhausted.finish();
    assert_summary(
        &summary,
        StopReason::Exhausted,
        ContinuationKnowledge::Exhausted,
        2,
        2,
        3,
    );

    // Page bound: a full page on the last permitted page.
    let mut bounded = client.items(0, nz32(2)).pages(1);
    assert_eq!(
        bounded
            .next_page()
            .await
            .expect("bounded")
            .expect("page")
            .items(),
        [1, 2]
    );
    assert!(bounded.next_page().await.expect("bound reached").is_none());
    assert_summary(
        &bounded.finish(),
        StopReason::PageBound,
        ContinuationKnowledge::NextRequestKnown,
        1,
        1,
        2,
    );

    // Application stop: the caller finishes while a continuation is known.
    let mut stopped = client.items(0, nz32(2)).pages(5);
    assert_eq!(
        stopped
            .next_page()
            .await
            .expect("stopped")
            .expect("page")
            .items(),
        [1, 2]
    );
    assert_summary(
        &stopped.finish(),
        StopReason::Application,
        ContinuationKnowledge::NextRequestKnown,
        1,
        1,
        2,
    );
    assert_eq!(server.request_count(), 4);
}

// ---------------------------------------------------------------------------
// Controller authority, cursors and redaction
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cursor_and_custom_empty_pages_follow_controller_authority_and_redact_continuations() {
    let cursor_server =
        ScriptedServer::start([vendor("|next-cursor"), vendor("2|-")]).expect("cursor server");
    let cursor_proxy = proxy(&cursor_server);
    let cursor_client = CursorApi::builder()
        .configure_http(move |http| http.proxy(cursor_proxy))
        .build()
        .expect("cursor client");
    let cursor_items = cursor_client
        .items()
        .collect_all(nz32(5))
        .await
        .expect("cursor pages");
    assert_eq!(cursor_items, [2]);
    let cursor_requests = cursor_server.requests();
    assert_eq!(cursor_requests.len(), 2);
    assert!(!cursor_requests[0].target().contains("cursor="));
    assert!(cursor_requests[1].target().contains("cursor=next-cursor"));

    CUSTOM_START_CALLS.store(0, Ordering::SeqCst);
    CUSTOM_CURSOR_CALLS.store(0, Ordering::SeqCst);
    CUSTOM_ADVANCE_CALLS.store(0, Ordering::SeqCst);
    let hooks = Arc::new(PaginationHooks::default());
    let configured_hooks: Arc<dyn OperationObserver> = hooks.clone();
    let custom_server =
        ScriptedServer::start([vendor("|custom-cursor-sentinel-6d91"), vendor("4|-")])
            .expect("custom server");
    let custom_proxy = proxy(&custom_server);
    let custom_client = CustomApi::builder()
        .configure_http(move |http| http.proxy(custom_proxy))
        .configure_runtime(move |runtime| runtime.operation_observer(configured_hooks))
        .build()
        .expect("custom client");
    let custom_prefix = custom_client
        .items("region-header-sentinel-b17")
        .collect_prefix(5)
        .await
        .expect("custom pages");
    assert_eq!(custom_prefix.items(), [4]);
    assert_eq!(custom_prefix.summary().stop_reason(), StopReason::Exhausted);
    // v2's per-page `validate` hook is gone. The controller starts once from
    // its public Init; core reads its whole cursor once for the initial
    // identity check and once per proposed continuation.
    assert_eq!(CUSTOM_START_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(CUSTOM_CURSOR_CALLS.load(Ordering::SeqCst), 2);
    assert_eq!(CUSTOM_ADVANCE_CALLS.load(Ordering::SeqCst), 2);
    let requests = custom_server.requests();
    assert_eq!(requests.len(), 2, "an empty custom page still continues");
    assert!(
        requests[1]
            .target()
            .contains("cursor=custom-cursor-sentinel-6d91")
    );
    assert_eq!(
        requests[1].header("x-region"),
        Some("region-header-sentinel-b17")
    );
    let observed = format!(
        "{:?}{:?}{:?}",
        hooks.before.lock().expect("before capture"),
        hooks.response.lock().expect("response capture"),
        hooks.completion.lock().expect("completion capture"),
    );
    assert!(!observed.contains("custom-cursor-sentinel-6d91"));
    assert!(!observed.contains("region-header-sentinel-b17"));
    assert_eq!(hooks.before.lock().expect("before capture").len(), 2);
    assert_eq!(
        hooks
            .aggregate
            .lock()
            .expect("aggregate capture")
            .as_slice(),
        [(2, 1, false, Some(200), StopReason::Exhausted)]
    );
}

api! {
    /// PAGE-01: a `#[sensitive]` whole-slot cursor destination.
    client SensitiveCursorApi {
        timeout 30s;
        base "http://pagination-sensitive-cursor.example.test/v1/";

        GET items(#[sensitive] query cursor?: String) -> VendorPage via VendorPageCodec {
            path "items";
            paginate via CursorPagination<String> {
                init CursorInit { cursor: cursor };
                cursor -> cursor;
            }
        }
    }
}

const SENSITIVE_CURSOR_SENTINEL: &str = "sensitive-cursor-sentinel-3f7a";

#[derive(Default)]
struct EventDebugRecorder {
    events: Mutex<Vec<String>>,
}

impl OperationObserver for EventDebugRecorder {
    fn observe(&self, event: OperationEvent<'_>) {
        self.events
            .lock()
            .expect("event records")
            .push(format!("{event:?}"));
    }
}

/// PAGE-01: the public sensitivity of a cursor destination propagates to the
/// continuation request. The controller's cursor sentinel reaches the wire of
/// page 2 in its declared query slot, but not the page-2 error (Display,
/// Debug or source chain), the traversal summaries, page metadata, observer
/// events, or the client, request and traversal debug output.
#[tokio::test]
async fn sensitive_cursor_destination_reaches_the_wire_but_no_page_two_diagnostic() {
    let server = ScriptedServer::start([
        vendor(&format!("1|{SENSITIVE_CURSOR_SENTINEL}")),
        ResponseScript::status(503, b"page-two-unavailable".to_vec()),
    ])
    .expect("sensitive cursor server");
    let proxy = proxy(&server);
    let recorder = Arc::new(EventDebugRecorder::default());
    let observer: Arc<dyn OperationObserver> = recorder.clone();
    let client = SensitiveCursorApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.operation_observer(observer))
        .build()
        .expect("sensitive cursor client");

    // The destination slot is statically classified sensitive: a plan of a
    // request carrying the same value in that slot redacts it. Continuations
    // reuse the same head program and classification.
    let explicit = client
        .items()
        .set_cursor(SENSITIVE_CURSOR_SENTINEL.to_owned())
        .plan()
        .expect("plan with an explicit sensitive cursor");
    assert!(explicit.observed_url().contains("cursor="));
    assert!(!explicit.observed_url().contains(SENSITIVE_CURSOR_SENTINEL));
    let request_debug = format!("{:?} {explicit:?}", client.items());
    let mut pages = client.items().pages(3);
    let first = pages
        .next_page()
        .await
        .expect("first page")
        .expect("first page is delivered");
    assert_eq!(first.items(), [1]);
    let first_page_debug = format!("{first:?} {:?}", first.metadata());
    let summary_after_first = pages.summary();
    assert_eq!(
        summary_after_first.continuation(),
        ContinuationKnowledge::NextRequestKnown
    );
    let traversal_debug = format!("{pages:?} {summary_after_first:?}");

    let error = pages
        .next_page()
        .await
        .expect_err("the continuation page is rejected");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.page_index(), Some(1));
    assert!(pages.next_page().await.expect("terminal").is_none());
    let finished = pages.finish();
    assert_eq!(finished.stop_reason(), StopReason::Failed);

    // The sentinel reached the wire of the continuation request only.
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(!requests[0].target().contains(SENSITIVE_CURSOR_SENTINEL));
    assert!(
        requests[1]
            .target()
            .contains(&format!("cursor={SENSITIVE_CURSOR_SENTINEL}"))
    );

    let mut error_chain = format!("{error} {error:?}");
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        error_chain.push_str(&format!(" {cause} {cause:?}"));
        source = cause.source();
    }
    let events = recorder.events.lock().expect("event records").join("\n");
    assert!(!events.is_empty());
    let corpus = [
        format!("{client:?}"),
        request_debug,
        first_page_debug,
        traversal_debug,
        format!("{finished:?}"),
        error_chain,
        format!("{:?}", error.traversal_summary()),
        format!("{:?}", error.http_response()),
        events,
    ]
    .join("\n");
    assert!(
        !corpus.contains(SENSITIVE_CURSOR_SENTINEL),
        "a sensitive cursor leaked into an automatic surface:\n{corpus}"
    );
}

#[tokio::test]
async fn empty_present_cursor_is_distinct_and_items_need_not_clone() {
    fn assert_send<T: Send>() {}
    assert_send::<OneNonClonePage>();

    let cursor_server =
        ScriptedServer::start([vendor("1|"), vendor("2|-")]).expect("empty cursor server");
    let cursor_proxy = proxy(&cursor_server);
    let cursor_client = CursorApi::builder()
        .configure_http(move |http| http.proxy(cursor_proxy))
        .build()
        .expect("empty cursor client");
    let prefix = cursor_client
        .items()
        .collect_prefix(3)
        .await
        .expect("empty present cursor continues");
    assert_eq!(prefix.items(), [1, 2]);
    assert_eq!(prefix.summary().stop_reason(), StopReason::Exhausted);
    let requests = cursor_server.requests();
    assert_eq!(requests.len(), 2);
    assert!(!requests[0].target().contains("cursor="));
    assert!(requests[1].target().ends_with("items?cursor="));

    let item_server = ScriptedServer::start([ResponseScript::ok(b"7,8".to_vec())])
        .expect("non-clone item server");
    let item_proxy = proxy(&item_server);
    let item_client = NonCloneApi::builder()
        .configure_http(move |http| http.proxy(item_proxy))
        .build()
        .expect("non-clone item client");
    let items = item_client
        .items()
        .collect_prefix(1)
        .await
        .expect("non-clone items")
        .into_items();
    assert_eq!(
        items.into_iter().map(|item| item.0).collect::<Vec<_>>(),
        [7, 8]
    );
}

#[tokio::test]
async fn explicit_endpoint_nested_scope_and_repeated_helpers_have_parity() {
    let explicit_server = ScriptedServer::start([json("[9]")]).expect("explicit server");
    let explicit_client = offset_client(&explicit_server);
    let endpoint = OffsetApi::unbound().items(0, nz32(2));
    let explicit_items = explicit_client
        .request(endpoint)
        .collect_prefix(1)
        .await
        .expect("explicit paginated endpoint");
    assert_eq!(explicit_items.items(), [9]);

    let nested_server =
        ScriptedServer::start([vendor("|region-b"), vendor("11|-"), vendor("12|-")])
            .expect("nested server");
    let nested_proxy = proxy(&nested_server);
    let nested_hooks = Arc::new(PaginationHooks::default());
    let configured_nested_hooks: Arc<dyn OperationObserver> = nested_hooks.clone();
    let nested_client = NestedPaginationApi::builder()
        .configure_http(move |http| http.proxy(nested_proxy))
        .configure_runtime(move |runtime| runtime.operation_observer(configured_nested_hooks))
        .build()
        .expect("nested client");
    let repositories = nested_client
        .organizations("region-a")
        .repositories("repository-b");
    let nested_items = repositories
        .fetch()
        .collect_prefix(3)
        .await
        .expect("nested scope pagination");
    assert_eq!(nested_items.items(), [11]);
    let original_snapshot_items = repositories
        .fetch()
        .collect_prefix(1)
        .await
        .expect("original scope snapshot remains unchanged");
    assert_eq!(original_snapshot_items.items(), [12]);
    let nested_requests = nested_server.requests();
    assert_eq!(nested_requests.len(), 3);
    for request in &nested_requests {
        assert!(
            request
                .target()
                .contains("/organizations/region-a/repositories/repository-b/entries")
        );
        assert_eq!(request.header("x-region"), Some("region-a"));
    }
    assert!(nested_requests[1].target().contains("cursor=region-b"));
    let nested_safe = nested_hooks.before.lock().expect("nested safe capture");
    assert_eq!(nested_safe.len(), 3);
    assert!(nested_safe.iter().all(|safe| !safe.contains("region-a")));
    assert!(nested_safe.iter().all(|safe| !safe.contains("region-b")));
    assert!(
        nested_safe
            .iter()
            .all(|safe| !safe.contains("repository-b"))
    );
    // The sensitive scope value is withheld from the automatic plan.
    let plan = repositories.fetch().plan().expect("nested plan");
    assert!(
        !plan.observed_url().contains("region-a"),
        "{}",
        plan.observed_url()
    );
    assert!(plan.observed_url().contains("repository-b"));

    let collision_client = CollisionPaginationApi::new().expect("collision client");
    let _: CollectPrefix<u8> = collision_client.left().items(0, nz32(1)).collect_prefix(1);
    let _: CollectAll<u8> = collision_client
        .right()
        .items(nz64(1), nz32(1))
        .collect_all(nz32(1));
    let _: Pages<u8> = collision_client.right().items(nz64(1), nz32(1)).pages(1);

    let nested_macro_client =
        repeated_nested_expansion::NestedMacroPaginationApi::new().expect("nested macro client");
    let _: CollectPrefix<u8> = nested_macro_client.items(0, nz32(1)).collect_prefix(1);
}

#[tokio::test]
async fn generated_pagination_omits_dynamic_origins_absent_that_rate_dimension() {
    const INITIAL: &str = "7319058410827319461";
    const NEXT: &str = "7319058410827319463";
    let server =
        ScriptedServer::start([json("[1,2]"), json("[3]")]).expect("rate pagination server");
    let proxy = proxy(&server);
    let limiter = Arc::new(PaginationLimiterProbe::default());
    let configured_limiter: Arc<dyn RateLimiter> = limiter.clone();
    let hooks = Arc::new(PaginationHooks::default());
    let configured_hooks: Arc<dyn OperationObserver> = hooks.clone();
    let client = PaginationRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| {
            runtime
                .operation_observer(configured_hooks)
                .rate_limiter(configured_limiter)
        })
        .build()
        .expect("rate pagination client");
    let items = client
        .items(INITIAL.parse::<u64>().expect("offset"), nz32(2))
        .collect_all(nz32(5))
        .await
        .expect("rate pagination");
    assert_eq!(items, [1, 2, 3]);

    // The page-live offset moves the path, query and header. It can never
    // move the logical origin: cursor destinations are endpoint-owned slots
    // and an endpoint cannot own a subdomain clause (ORIGIN-02, PAGE-01).
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].target().contains(&format!("/items/{INITIAL}?")));
    assert!(requests[1].target().contains(&format!("/items/{NEXT}?")));
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
    // No origin dimension, so no logical origin is disclosed to the limiter.
    assert_eq!(contexts[0].1, None);
    assert_eq!(contexts[1].1, None);
    assert_eq!(contexts[0].2.len(), 1);
    assert_eq!(contexts[0].2, contexts[1].2);
    let mut endpoint_bytes = vec![4];
    endpoint_bytes.extend_from_slice(b"utf8");
    endpoint_bytes.extend_from_slice(&(b"pagination.rate.items".len() as u64).to_be_bytes());
    endpoint_bytes.extend_from_slice(b"pagination.rate.items");
    assert_eq!(
        contexts[0].2[0],
        ("endpoint".to_owned(), endpoint_bytes),
        "the endpoint dimension is the pinned operational ID, not a page value"
    );
    assert_eq!(
        hooks
            .aggregate
            .lock()
            .expect("aggregate capture")
            .as_slice(),
        [(2, 3, false, Some(200), StopReason::Exhausted)]
    );
    let observed = format!(
        "{:?}{:?}{:?}",
        hooks.before.lock().expect("before capture"),
        hooks.response.lock().expect("response capture"),
        hooks.completion.lock().expect("completion capture"),
    );
    assert!(!observed.contains(INITIAL));
    assert!(!observed.contains(NEXT));
}

// ---------------------------------------------------------------------------
// Built-in controllers through the Init API (V3-P08)
// ---------------------------------------------------------------------------

fn built_in_client(server: &ScriptedServer) -> BuiltInCursorApi {
    let proxy = proxy(server);
    BuiltInCursorApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("built-in cursor client")
}

#[tokio::test]
async fn next_header_controller_follows_one_header_and_rejects_duplicates() {
    let server =
        ScriptedServer::start([json("[1]").with_header("x-next-cursor", "n-1"), json("[2]")])
            .expect("header cursor server");
    let client = built_in_client(&server);
    let items = client
        .headed()
        .collect_all(nz32(3))
        .await
        .expect("header cursor pages");
    assert_eq!(items, [1, 2]);
    let requests = server.requests();
    assert!(!requests[0].target().contains("cursor="));
    assert!(requests[1].target().ends_with("headed?cursor=n-1"));

    let duplicate = ScriptedServer::start([json("[1]")
        .with_header("x-next-cursor", "n-1")
        .with_header("x-next-cursor", "n-2")])
    .expect("duplicate header server");
    let client = built_in_client(&duplicate);
    let mut pages = client.headed().pages(3);
    let error = pages
        .next_page()
        .await
        .expect_err("two continuation header values are ambiguous");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::InvalidContinuation)
    );
    assert!(pages.next_page().await.expect("after failure").is_none());
    assert_summary(
        &pages.summary(),
        StopReason::Failed,
        ContinuationKnowledge::Unknown,
        1,
        0,
        0,
    );
    assert_eq!(duplicate.request_count(), 1);
}

#[tokio::test]
async fn link_controller_owns_only_the_same_origin_cursor_slot() {
    let server = ScriptedServer::start([
        json("[1]").with_header(
            "link",
            r#"</v1/linked?filter=blue&cursor=a,b>; rel="next", </v1/linked?filter=blue>; rel="first""#,
        ),
        json("[2]"),
    ])
    .expect("link cursor server");
    let client = built_in_client(&server);
    let items = client
        .linked()
        .collect_all(nz32(3))
        .await
        .expect("link cursor pages");
    assert_eq!(items, [1, 2]);
    let requests = server.requests();
    assert!(requests[0].target().ends_with("linked?filter=blue"));
    // The comma inside `<...>` belongs to the cursor, not to the link list.
    assert!(
        requests[1]
            .target()
            .ends_with("linked?cursor=a%2Cb&filter=blue"),
        "{}",
        requests[1].target()
    );

    for (link, kind) in [
        (
            r#"</v1/linked?filter=blue&cursor=x>; rel="next", </v1/linked?filter=blue&cursor=y>; rel=next"#,
            PaginationErrorKind::InvalidContinuation,
        ),
        (
            "<http://elsewhere.example.test/v1/linked?filter=blue&cursor=x>; rel=next",
            PaginationErrorKind::CrossOriginContinuation,
        ),
        (
            "</v1/other?filter=blue&cursor=x>; rel=next",
            PaginationErrorKind::InvalidContinuation,
        ),
        (
            "</v1/linked?filter=red&cursor=x>; rel=next",
            PaginationErrorKind::InvalidContinuation,
        ),
        (
            "</v1/linked?filter=blue&cursor=x&cursor=y>; rel=next",
            PaginationErrorKind::InvalidContinuation,
        ),
    ] {
        let server = ScriptedServer::start([json("[1]").with_header("link", link)])
            .expect("rejected link server");
        let client = built_in_client(&server);
        let error = client
            .linked()
            .collect_prefix(3)
            .await
            .expect_err("the next link must only move the cursor");
        assert_eq!(error.pagination_kind(), Some(kind), "{link}");
        let summary = error.traversal_summary().expect("summary");
        assert_eq!(
            (summary.pages_received(), summary.pages_delivered()),
            (1, 0)
        );
        assert_eq!(server.request_count(), 1, "{link}");
    }
}

#[cfg(feature = "json")]
#[tokio::test]
async fn downstream_envelope_controller_pages_a_json_envelope_without_iterable_bounds() {
    let server = ScriptedServer::start([
        json(r#"{"data":[1,2],"next":"cursor-2"}"#),
        json(r#"{"data":[3],"next":null}"#),
    ])
    .expect("envelope server");
    let proxy = proxy(&server);
    let client = envelope::EnvelopeApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("envelope client");
    let items = client
        .feed()
        .collect_all(nz32(4))
        .await
        .expect("envelope pages");
    assert_eq!(items, [1, 2, 3]);
    let requests = server.requests();
    assert!(requests[0].target().ends_with("/v1/feed"));
    assert!(requests[1].target().ends_with("/v1/feed?after=cursor-2"));
}

// ---------------------------------------------------------------------------
// Aggregate deadline and budgets (V3-P06)
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn aggregate_deadline_spans_time_spent_between_polls() {
    let _clock = ClockGuard::start();
    let server = ScriptedServer::start([json("[1,2]"), json("[3,4]")]).expect("deadline server");
    let client = offset_client(&server);

    let mut pages = client
        .items(0, nz32(2))
        .timeout(Duration::from_secs(5))
        .expect("finite timeout")
        .pages(4);
    let first = pages.next_page().await.expect("first page").expect("page");
    assert_eq!(first.items(), [1, 2]);
    // The application pauses between polls; the single aggregate deadline
    // keeps running and is never reset per page.
    tokio::time::advance(Duration::from_secs(5)).await;
    let error = pages
        .next_page()
        .await
        .expect_err("aggregate deadline elapsed during the caller pause");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(server.request_count(), 1);
    assert!(pages.next_page().await.expect("after failure").is_none());
    assert_eq!(pages.summary().stop_reason(), StopReason::Failed);
    assert_eq!(pages.summary().pages_delivered(), 1);
}

#[tokio::test]
async fn send_and_replay_caps_persist_across_pages_and_retries() {
    let proxy_for = |server: &ScriptedServer| {
        let proxy = proxy(server);
        OffsetApi::builder()
            .configure_http(move |http| http.proxy(proxy))
            .retry(retry_rejected_statuses())
            .build()
            .expect("retrying offset client")
    };

    // Two sends: the first page's retry consumes the second send, so the
    // next page cannot be dispatched.
    let sends = ScriptedServer::start([
        ResponseScript::status(503, b"retry".to_vec()),
        json("[1,2]"),
        json("[3]"),
    ])
    .expect("send-cap server");
    let client = proxy_for(&sends);
    let mut pages = client
        .items(0, nz32(2))
        .with_options(&CallOptions::new().cap_sends(2))
        .pages(5);
    assert_eq!(
        pages
            .next_page()
            .await
            .expect("page 0")
            .expect("page")
            .items(),
        [1, 2]
    );
    let error = pages
        .next_page()
        .await
        .expect_err("the send cap is aggregate, not per page");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::SendCapExhausted)
    );
    assert_eq!(sends.request_count(), 2);
    let summary = pages.summary();
    assert_eq!(summary.application_attempts(), 2);
    assert_eq!(summary.automatic_replays(), 1);
    assert_eq!(summary.stop_reason(), StopReason::Failed);

    // One replay: the first page used it, so the second page's rejected
    // status is terminal instead of being retried.
    let replays = ScriptedServer::start([
        ResponseScript::status(503, b"retry".to_vec()),
        json("[1,2]"),
        ResponseScript::status(503, b"again".to_vec()),
        json("[3]"),
    ])
    .expect("replay-cap server");
    let client = proxy_for(&replays);
    let error = client
        .items(0, nz32(2))
        .with_options(&CallOptions::new().cap_replays(1))
        .collect_prefix(5)
        .await
        .expect_err("the replay cap is cumulative across the traversal");
    assert_eq!(error.http_status().map(|status| status.as_u16()), Some(503));
    assert_eq!(replays.request_count(), 3);
    let summary = error.traversal_summary().expect("replay summary");
    assert_eq!(summary.application_attempts(), 3);
    assert_eq!(summary.automatic_replays(), 1);
    assert_eq!(summary.pages_received(), 1);
}

#[tokio::test]
async fn acquired_bytes_include_rejected_bodies_and_the_byte_budget_is_aggregate() {
    let rejected = ScriptedServer::start([
        json("[1,2]"),
        ResponseScript::status(500, b"rejected-body".to_vec()),
    ])
    .expect("rejected body server");
    let client = offset_client(&rejected);
    let error = client
        .items(0, nz32(2))
        .collect_prefix(5)
        .await
        .expect_err("the second page is rejected");
    assert_eq!(error.http_status().map(|status| status.as_u16()), Some(500));
    let summary = error.traversal_summary().expect("rejected summary");
    // Five accepted page bytes plus the thirteen captured rejected bytes.
    assert_eq!(summary.acquired_response_bytes(), 5 + 13);
    assert_eq!(summary.pages_received(), 1);

    let budget = ScriptedServer::start([json("[1,2]"), json("[3,4]"), json("[5,6]")])
        .expect("byte budget server");
    let client = offset_client(&budget);
    let error = client
        .items(0, nz32(2))
        .with_options(
            &CallOptions::new().cap_response_bytes(NonZeroUsize::new(12).expect("nonzero")),
        )
        .collect_all(nz32(10))
        .await
        .expect_err("the third page exceeds the aggregate byte cap");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::CollectedResponseByteCeilingExceeded)
    );
    let summary = error.traversal_summary().expect("byte budget summary");
    assert_eq!(summary.pages_delivered(), 2);
    // The third page's five bytes were acquired and counted; the aggregate
    // then exceeded the cap and that page was withheld.
    assert_eq!(summary.acquired_response_bytes(), 15);
    assert_eq!(budget.request_count(), 3);
}

#[tokio::test]
async fn identity_budget_persists_across_pages() {
    let calibration = ScriptedServer::start([json("[1,2]")]).expect("calibration server");
    let client = offset_client(&calibration);
    let mut pages = client.items(0, nz32(2)).pages(3);
    pages.next_page().await.expect("page 0").expect("page");
    // After page 0 the traversal retains the initial and the validated next
    // identity; offsets 0, 2 and 4 have equal-length identities.
    let two_identities = pages.summary().retained_identity_bytes();
    assert!(two_identities > 0);
    assert_eq!(two_identities % 2, 0);
    let one_identity = two_identities / 2;
    drop(pages);

    let limited = ScriptedServer::start([json("[1,2]"), json("[3,4]"), json("[5]")])
        .expect("identity budget server");
    let proxy = proxy(&limited);
    let limit = NonZeroUsize::new(3 * one_identity - 1).expect("nonzero identity limit");
    let client = OffsetApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.pagination_identity_limit(limit.get()))
        .build()
        .expect("identity-limited client");
    let mut pages = client.items(0, nz32(2)).pages(5);
    assert_eq!(
        pages
            .next_page()
            .await
            .expect("page 0")
            .expect("page")
            .items(),
        [1, 2]
    );
    assert_eq!(pages.summary().retained_identity_bytes(), two_identities);
    let error = pages
        .next_page()
        .await
        .expect_err("the third identity exceeds the traversal budget");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::IdentityStorageCeilingExceeded)
    );
    assert_summary(
        &pages.summary(),
        StopReason::Failed,
        ContinuationKnowledge::Unknown,
        2,
        1,
        2,
    );
    assert_eq!(limited.request_count(), 2);
}

// ---------------------------------------------------------------------------
// Cancellation law (V3-P07)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dropping_an_unpolled_fetch_changes_nothing() {
    let server = ScriptedServer::start([json("[1]")]).expect("unpolled server");
    let client = offset_client(&server);
    let mut pages = client.items(0, nz32(2)).pages(3);
    drop(pages.next_page());
    let summary = pages.summary();
    assert_eq!(summary.stop_reason(), StopReason::NotStarted);
    assert_eq!(summary.operation_id(), None);
    assert_eq!(server.request_count(), 0);
    let page = pages.next_page().await.expect("page").expect("page");
    assert_eq!(page.items(), [1]);
    assert_eq!(pages.summary().stop_reason(), StopReason::Exhausted);
}

#[tokio::test]
async fn dropping_a_started_fetch_cancels_the_traversal_for_good() {
    let server = ScriptedServer::start([ResponseScript::DelayBeforeHeaders {
        delay: Duration::from_millis(1_500),
        status: 200,
        body: b"[1]".to_vec(),
    }])
    .expect("slow server");
    let client = offset_client(&server);
    let mut pages = client.items(0, nz32(2)).pages(3);
    let started = tokio::time::timeout(Duration::from_millis(200), pages.next_page()).await;
    assert!(
        started.is_err(),
        "the fetch was still in flight when dropped"
    );
    let summary = pages.summary();
    assert_eq!(summary.stop_reason(), StopReason::Cancelled);
    assert!(summary.operation_id().is_some());
    // Budgets are never replenished: the traversal cannot resume.
    assert!(
        pages
            .next_page()
            .await
            .expect("after cancellation")
            .is_none()
    );
    assert_eq!(pages.finish().stop_reason(), StopReason::Cancelled);
}

#[tokio::test]
async fn attached_cancellation_and_fetch_errors_are_reported_once() {
    let server =
        ScriptedServer::start([json("[1,2]"), json("[3,4]")]).expect("cancellation server");
    let client = offset_client(&server);
    let token = CancellationToken::new();
    let mut pages = client
        .items(0, nz32(2))
        .cancel_with(token.clone())
        .expect("one cancellation source")
        .pages(5);
    assert_eq!(
        pages
            .next_page()
            .await
            .expect("page 0")
            .expect("page")
            .items(),
        [1, 2]
    );
    token.cancel();
    let error = pages
        .next_page()
        .await
        .expect_err("the captured token cancels the traversal");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::Cancelled)
    );
    assert!(
        pages
            .next_page()
            .await
            .expect("after cancellation")
            .is_none()
    );
    assert_eq!(pages.summary().stop_reason(), StopReason::Cancelled);
    assert_eq!(server.request_count(), 1);

    let failing = ScriptedServer::start([ResponseScript::status(500, b"failed".to_vec())])
        .expect("failing server");
    let client = offset_client(&failing);
    let mut pages = client.items(0, nz32(2)).pages(5);
    let error = pages.next_page().await.expect_err("rejected first page");
    assert_eq!(error.http_status().map(|status| status.as_u16()), Some(500));
    assert!(pages.next_page().await.expect("after failure").is_none());
    assert!(pages.next_page().await.expect("still terminal").is_none());
    let summary = pages.finish();
    assert_eq!(summary.stop_reason(), StopReason::Failed);
    assert_eq!(
        summary.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(failing.request_count(), 1);
}

#[test]
fn collectors_carry_request_controls_before_polling() {
    let client = CollisionPaginationApi::new().expect("collision client");
    let collection = client
        .left()
        .items(0, nz32(1))
        .timeout(Duration::from_secs(9))
        .expect("finite timeout")
        .collect_prefix(2)
        .with_options(&CallOptions::new().cap_sends(3));
    let summary = RequestControls::control_summary(&collection);
    assert_eq!(summary.declared_timeout(), Some(Duration::from_secs(30)));
    assert_eq!(summary.selected_timeout(), Some(Duration::from_secs(9)));
    assert_eq!(summary.effective_timeout(), Some(Duration::from_secs(9)));
    assert_eq!(summary.send_cap(), Some(3));
    assert_eq!(summary.cancellation_sources(), 0);
    let collection = RequestControls::cancel_with(collection, CancellationToken::new())
        .expect("cancellation source")
        .with_options(&CallOptions::new().cap_sends(5));
    let summary = collection.control_summary();
    // Caps only narrow: a later, larger send cap keeps the smaller one.
    assert_eq!(summary.send_cap(), Some(3));
    assert_eq!(summary.cancellation_sources(), 1);
    let complete = client
        .right()
        .items(nz64(1), nz32(1))
        .with_options(&CallOptions::new().cap_replays(0))
        .collect_all(nz32(2));
    assert_eq!(complete.control_summary().replay_cap(), Some(0));
}

// ---------------------------------------------------------------------------
// Zero bounds (PAGE-02, V3-P04)
// ---------------------------------------------------------------------------

/// The zero-bound probe places a provider credential, and credentials are
/// only ever placed over HTTPS, so it needs the TLS-capable build.
#[cfg(feature = "default-tls")]
mod zero_bounds {
    use concord::SecretString;
    use concord::auth::{
        AuthError, CredentialMaterial, CredentialProvider, CredentialRequest, SecretCredential,
    };

    use super::*;

    // Zero-bound probe (PAGE-02): every role counts its calls.
    static ZERO_CONTROLLER_CALLS: AtomicUsize = AtomicUsize::new(0);
    static ZERO_BODY_PREPARES: AtomicUsize = AtomicUsize::new(0);
    static ZERO_PROVIDER_ACQUIRES: AtomicUsize = AtomicUsize::new(0);
    static ZERO_LIMITER_ACQUIRES: AtomicUsize = AtomicUsize::new(0);
    static ZERO_OBSERVED_EVENTS: AtomicUsize = AtomicUsize::new(0);

    pub struct ZeroInit {
        pub cursor: Option<String>,
    }

    pub struct ZeroController {
        cursor: Option<String>,
    }

    impl PaginationController<VendorPage> for ZeroController {
        type Item = u32;
        type Init = ZeroInit;
        type Cursor = Option<String>;

        const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

        fn start(init: ZeroInit) -> Result<Self, PaginationError> {
            ZERO_CONTROLLER_CALLS.fetch_add(1, Ordering::SeqCst);
            Ok(Self {
                cursor: init.cursor,
            })
        }

        fn cursor(&self) -> Option<String> {
            ZERO_CONTROLLER_CALLS.fetch_add(1, Ordering::SeqCst);
            self.cursor.clone()
        }

        fn advance(
            self,
            page: PageResponse<VendorPage>,
            limits: PageDecisionLimits,
        ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
            ZERO_CONTROLLER_CALLS.fetch_add(1, Ordering::SeqCst);
            Ok(PageStep::Done {
                items: bounded_items(page.into_page().items, limits)?,
            })
        }
    }

    #[derive(Clone)]
    pub struct ZeroBody(String);

    pub struct ZeroBodySpec;

    impl RequestRepresentation<ZeroBody> for ZeroBodySpec {
        fn prepare(
            input: ZeroBody,
            _context: RequestBodyContext<'_>,
        ) -> Result<PreparedBody, BodyError> {
            ZERO_BODY_PREPARES.fetch_add(1, Ordering::SeqCst);
            Ok(PreparedBody::bytes(Bytes::from(input.0)))
        }
    }

    impl PageRequestRepresentation<ZeroBody> for ZeroBodySpec {}

    pub struct ZeroToken(SecretString);

    impl CredentialMaterial for ZeroToken {}

    impl SecretCredential for ZeroToken {
        fn expose_secret(&self) -> &str {
            self.0.expose_secret()
        }
    }

    pub struct ZeroProvider;

    impl CredentialProvider for ZeroProvider {
        type Material = ZeroToken;

        fn acquire<'a>(
            &'a self,
            _context: CredentialRequest<'a>,
        ) -> concord::auth::BoxFuture<'a, Result<Self::Material, AuthError>> {
            ZERO_PROVIDER_ACQUIRES.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Ok(ZeroToken(
                    SecretString::try_new("zero-provider-sentinel").expect("secret"),
                ))
            })
        }
    }

    struct ZeroLimiter;

    impl RateLimiter for ZeroLimiter {
        fn capability(&self) -> LimiterCapability {
            LimiterCapability::new(Enforcement::Local, "concord.test.zero", 1)
        }

        fn acquire<'a>(
            &'a self,
            _request: RateLimitRequest<'a>,
        ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
            ZERO_LIMITER_ACQUIRES.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(RateLimitPermit::granted()) })
        }
    }

    struct ZeroObserver;

    impl OperationObserver for ZeroObserver {
        fn observe(&self, _event: OperationEvent<'_>) {
            ZERO_OBSERVED_EVENTS.fetch_add(1, Ordering::SeqCst);
        }
    }

    // Credentials are only ever placed over HTTPS, so this probe needs TLS.
    api! {
        client ZeroApi {
            timeout 30s;
            base "https://pagination-zero.example.test/v1/";

            credentials {
                credential probe = custom ZeroProvider;
            }

            auth { bearer probe; }

            POST search(query cursor?: String, body filter: ZeroBody via ZeroBodySpec) -> VendorPage via VendorPageCodec {
                id "pagination.zero.search";
                path "search";
                effect safe;
                rate_limit { id "pagination.zero"; key [origin, endpoint]; allow 10 per 1s; }
                paginate via ZeroController {
                    init ZeroInit { cursor: cursor };
                    cursor -> cursor;
                }
            }
        }
    }

    #[tokio::test]
    async fn zero_bounds_perform_no_operation_body_provider_controller_or_limiter_work() {
        let client = ZeroApi::builder()
            .probe(ZeroProvider)
            .configure_runtime(|runtime| {
                runtime
                    .rate_limiter(Arc::new(ZeroLimiter))
                    .operation_observer(Arc::new(ZeroObserver))
            })
            .build()
            .expect("zero client");
        let request = || client.search(ZeroBody("zero-body".to_owned()));
        let assert_no_work = || {
            assert_eq!(
                ZERO_CONTROLLER_CALLS.load(Ordering::SeqCst),
                0,
                "controller"
            );
            assert_eq!(ZERO_BODY_PREPARES.load(Ordering::SeqCst), 0, "body");
            assert_eq!(ZERO_PROVIDER_ACQUIRES.load(Ordering::SeqCst), 0, "provider");
            assert_eq!(ZERO_LIMITER_ACQUIRES.load(Ordering::SeqCst), 0, "limiter");
            assert_eq!(
                ZERO_OBSERVED_EVENTS.load(Ordering::SeqCst),
                0,
                "observation"
            );
        };
        let assert_zero_summary = |summary: &TraversalSummary| {
            assert_eq!(summary.operation_id(), None);
            assert_summary(
                summary,
                StopReason::PageBound,
                ContinuationKnowledge::Unknown,
                0,
                0,
                0,
            );
            assert_eq!(summary.requested_page_bound(), 0);
            assert_eq!(summary.effective_page_bound(), 0);
            assert_eq!(summary.application_attempts(), 0);
            assert_eq!(summary.quota_admissions(), 0);
            assert_eq!(summary.acquired_response_bytes(), 0);
            assert_eq!(
                summary.dispatch_knowledge(),
                DispatchKnowledge::NotDispatched
            );
        };

        let prefix = request()
            .collect_prefix(0)
            .await
            .expect("zero prefix is an empty explicit prefix");
        assert!(prefix.items().is_empty());
        assert_zero_summary(prefix.summary());
        assert_no_work();

        let mut pages = request().pages(0);
        assert_zero_summary(&pages.summary());
        assert!(pages.next_page().await.expect("zero pages").is_none());
        assert_zero_summary(&pages.finish());
        assert_no_work();

        // A nonzero traversal finished before its first poll records an
        // application stop without any identity or observation.
        let unstarted = request().pages(3).finish();
        assert_eq!(unstarted.operation_id(), None);
        assert_eq!(unstarted.stop_reason(), StopReason::Application);
        assert_eq!(unstarted.continuation(), ContinuationKnowledge::Unknown);
        assert_eq!(unstarted.requested_page_bound(), 3);
        assert_no_work();

        // The probes are live: a nonzero bound through an unreachable proxy runs
        // the controller, body, provider and limiter and is observed before the
        // transport fails.
        let unreachable = ZeroApi::builder()
            .probe(ZeroProvider)
            .configure_http(|http| {
                http.proxy(SafeProxy::parse("http://127.0.0.1:1").expect("closed-port proxy"))
            })
            .configure_runtime(|runtime| {
                runtime
                    .rate_limiter(Arc::new(ZeroLimiter))
                    .operation_observer(Arc::new(ZeroObserver))
            })
            .build()
            .expect("unreachable zero client");
        let error = unreachable
            .search(ZeroBody("zero-body".to_owned()))
            .collect_prefix(1)
            .await
            .expect_err("closed proxy port");
        assert!(error.traversal_summary().is_some());
        assert!(
            ZERO_CONTROLLER_CALLS.load(Ordering::SeqCst) > 0,
            "controller"
        );
        assert_eq!(ZERO_BODY_PREPARES.load(Ordering::SeqCst), 1, "body");
        assert_eq!(ZERO_PROVIDER_ACQUIRES.load(Ordering::SeqCst), 1, "provider");
        assert_eq!(ZERO_LIMITER_ACQUIRES.load(Ordering::SeqCst), 1, "limiter");
        assert!(
            ZERO_OBSERVED_EVENTS.load(Ordering::SeqCst) > 0,
            "observation"
        );
    }
}

/// §20.4 `aggregate_max_sends`: the trusted aggregate handoff ceiling is
/// configurable, and a per-call send cap only narrows it (never widens it).
#[tokio::test]
async fn configured_aggregate_send_ceiling_is_intersected_with_the_call_send_cap() {
    let client_for = |server: &ScriptedServer, aggregate: u32| {
        let proxy = proxy(server);
        OffsetApi::builder()
            .configure_http(move |http| http.proxy(proxy))
            .configure_runtime(move |runtime| runtime.aggregate_max_sends(aggregate))
            .build()
            .expect("client with an aggregate send ceiling")
    };

    // The configured ceiling (2) binds even though the call allows 5 sends.
    let server = ScriptedServer::start([json("[1,2]"), json("[3,4]"), json("[5]")])
        .expect("aggregate server");
    let client = client_for(&server, 2);
    let mut pages = client
        .items(0, nz32(2))
        .with_options(&CallOptions::new().cap_sends(5))
        .pages(5);
    for expected in [[1, 2], [3, 4]] {
        let page = pages.next_page().await.expect("page").expect("page");
        assert_eq!(page.items(), expected);
    }
    let error = pages
        .next_page()
        .await
        .expect_err("the configured aggregate ceiling is exhausted");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::AttemptCeilingExceeded)
    );
    assert_eq!(server.request_count(), 2);
    assert_eq!(pages.summary().application_attempts(), 2);

    // A narrower call send cap (1) wins over the configured ceiling (3) and
    // is reported as the caller's cap.
    let server = ScriptedServer::start([json("[1,2]"), json("[3,4]")]).expect("cap server");
    let client = client_for(&server, 3);
    let mut pages = client
        .items(0, nz32(2))
        .with_options(&CallOptions::new().cap_sends(1))
        .pages(5);
    pages.next_page().await.expect("page 0").expect("page");
    let error = pages
        .next_page()
        .await
        .expect_err("the call send cap is exhausted");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::SendCapExhausted)
    );
    assert_eq!(server.request_count(), 1);

    // Zero is invalid configuration reported by the root build.
    let error = OffsetApi::builder()
        .configure_runtime(|runtime| runtime.aggregate_max_sends(0))
        .build()
        .expect_err("zero aggregate ceiling");
    assert_eq!(error.input_field(), Some("aggregate_max_sends"));
}
