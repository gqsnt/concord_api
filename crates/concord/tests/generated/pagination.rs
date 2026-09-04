use std::cell::Cell;
#[cfg(feature = "json")]
use std::num::NonZeroUsize;
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use concord::Bytes;
#[cfg(feature = "json")]
use concord::Text;
use concord::api;
#[cfg(feature = "json")]
use concord::body::{
    BodyError, BodyFactory, BodyKind, LengthHint, PageRequestRepresentation, RequestBody,
    RequestBodyContext, RequestBodyMetadata, RequestContentType, RequestRepresentation,
    RequestStream,
};
use concord::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use concord::http::SafeProxy;
use concord::observation::{OperationEvent, OperationObserver};
use concord::pagination::{
    PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
    PaginationError, PaginationErrorKind,
};
use concord::rate_limit::{
    BoxFuture, RateLimitError, RateLimitPermit, RateLimitRequest, RateLimitResponse, RateLimiter,
};
use concord::response::{
    Buffered, PageResponseRepresentation, ResponseBodyError, ResponseBodyErrorKind, ResponseHead,
    ResponseRepresentation,
};
#[cfg(feature = "json")]
use concord::retry::{RetryDecision, RetryFailure, RetryPolicy};
use concord_test_support::server::{ResponseScript, ScriptedServer};

use crate::support::NextCursor;

fn nz32(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).expect("positive u32")
}

fn nz64(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).expect("positive u64")
}

fn bounded_items<T>(items: Vec<T>, limits: PageDecisionLimits) -> Result<Vec<T>, PaginationError> {
    if items.len() > limits.remaining_items() {
        return Err(PaginationError::new(
            PaginationErrorKind::CollectedItemCeilingExceeded,
        ));
    }
    Ok(items)
}

#[cfg(feature = "json")]
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

#[cfg(feature = "json")]
pub struct AcceptedSearchPageSpec;

#[cfg(feature = "json")]
impl ResponseRepresentation<Vec<u32>> for AcceptedSearchPageSpec {
    type Driver = Buffered;

    fn accept() -> Option<&'static str> {
        Some("application/json")
    }

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

#[cfg(feature = "json")]
impl PageResponseRepresentation<Vec<u32>> for AcceptedSearchPageSpec {}

#[cfg(feature = "json")]
#[derive(Clone)]
pub struct RepeatableFactoryInput {
    bytes: Bytes,
    metadata_calls: Arc<AtomicUsize>,
    prepare_calls: Arc<AtomicUsize>,
    create_calls: Arc<AtomicUsize>,
}

#[cfg(feature = "json")]
pub struct RepeatableFactorySpec;

#[cfg(feature = "json")]
impl RequestRepresentation<RepeatableFactoryInput> for RepeatableFactorySpec {
    fn metadata(
        input: &RepeatableFactoryInput,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        input.metadata_calls.fetch_add(1, Ordering::SeqCst);
        Ok(RequestBodyMetadata {
            length: LengthHint::Unknown,
            content_type: RequestContentType::None,
            kind: BodyKind::Stream,
        })
    }

    fn prepare(
        input: RepeatableFactoryInput,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        input.prepare_calls.fetch_add(1, Ordering::SeqCst);
        Ok(RequestBody::factory(RepeatableFactory {
            bytes: input.bytes,
            create_calls: input.create_calls,
        }))
    }
}

#[cfg(feature = "json")]
impl PageRequestRepresentation<RepeatableFactoryInput> for RepeatableFactorySpec {}

#[cfg(feature = "json")]
struct RepeatableFactory {
    bytes: Bytes,
    create_calls: Arc<AtomicUsize>,
}

#[cfg(feature = "json")]
impl BodyFactory for RepeatableFactory {
    fn create(&self) -> Result<RequestStream, BodyError> {
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        Ok(RequestStream::from_bytes(self.bytes.clone()))
    }
}

#[cfg(feature = "json")]
pub struct StableCycleController {
    pub cursor: String,
}

#[cfg(feature = "json")]
impl PaginationController<Vec<u32>> for StableCycleController {
    type Item = u32;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

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

#[cfg(feature = "json")]
api! {
    client BodyPaginationApi {
        timeout 30s;
        base "http://pagination-body.example.test/v1/";


    POST json_pages(
        offset: u64,
        limit: std::num::NonZeroU32,
        body filter: crate::generated::pagination::CountedSearchInput via concord::Json,
    ) -> Vec<u32> via crate::generated::pagination::AcceptedSearchPageSpec {
        status [200..=299, 404];
        path ["json-pages"];
        query {
            "offset" = endpoint.offset;
            "limit" = endpoint.limit;
        }
        paginate via crate::support::OffsetController {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
        }
        effect safe;
    }

    POST factory_pages(
        offset: u64,
        limit: std::num::NonZeroU32,
        body body: crate::generated::pagination::RepeatableFactoryInput via crate::generated::pagination::RepeatableFactorySpec,
    ) -> Vec<u32> via crate::support::Json {
        path ["factory-pages"];
        query {
            "offset" = endpoint.offset;
            "limit" = endpoint.limit;
        }
        paginate via crate::support::OffsetController {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
        }
        effect safe;
    }

    POST stable_cycle(
        cursor: String,
        body body: String via Text,
    ) -> Vec<u32> via crate::support::Json {
        effect safe;
        path ["stable-cycle"];
        query { "cursor" = endpoint.cursor; }
        paginate via crate::generated::pagination::StableCycleController {
            cursor -> endpoint.cursor;
        }
    }
}}

api! {
    client OffsetApi {
        timeout 30s;
        base "http://pagination-offset.example.test/v1/";


    GET items(offset: u64, limit: std::num::NonZeroU32) -> Vec<u32> via crate::support::Json {
        path ["items"];
        query {
            "offset" = endpoint.offset;
            "limit" = endpoint.limit;
        }
        paginate via crate::support::OffsetController {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
        }
    }
}}

api! {
    client PageApi {
        timeout 30s;
        base "http://pagination-page.example.test/v1/";


    GET items(
        page: std::num::NonZeroU64,
        per_page: std::num::NonZeroU32,
    ) -> Vec<u32> via crate::support::Json {
        path ["items"];
        query {
            "page" = endpoint.page;
            "per_page" = endpoint.per_page;
        }
        paginate via crate::support::PageController {
            page -> endpoint.page;
            page_size -> endpoint.per_page;
        }
    }
}}

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

impl concord::RequestAtom for CountedOffset {
    fn write_atom(
        &self,
        output: &mut concord::BoundedAtomWriter<'_>,
    ) -> Result<(), concord::AtomError> {
        output.write(&self.0)
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
        type Item = u32;

        const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

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
}

use application_count_controller::ApplicationCountController;

api! {
    client ApplicationCountApi {
        timeout 30s;
        base "http://pagination-application-count.example.test/v1/";
        values {
            policy: crate::StablePolicy;
        }


    GET items(
        offset: crate::CountedOffset,
        limit: std::num::NonZeroU32,
    ) -> Vec<u32> via crate::support::Json {
        path ["items"];
        query {
            "offset" = endpoint.offset;
            "limit" = endpoint.limit;
        }
        paginate via ApplicationCountController {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
            policy = client.policy;
            attempts = 0;
            enabled = true;
            label = "count";
        }
    }
}}

pub struct VendorPageCodec;

pub struct CustomPageSpec;

impl ResponseRepresentation<Vec<u32>> for CustomPageSpec {
    type Driver = Buffered;

    fn accept() -> Option<&'static str> {
        Some("application/x-concord-custom-page")
    }

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

impl PageResponseRepresentation<Vec<u32>> for CustomPageSpec {}

api! {
    client CustomResponsePageApi {
        timeout 30s;
        base "http://pagination-custom-response.example.test/v1/";


    GET items(offset: u64, limit: std::num::NonZeroU32) -> Vec<u32> via crate::CustomPageSpec {
        path ["items"];
        query {
            "offset" = endpoint.offset;
            "limit" = endpoint.limit;
        }
        paginate via crate::support::OffsetController {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
        }
    }
}}

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

impl NextCursor<String> for VendorPage {
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

pub struct NonCloneItem(u32);
pub struct NonClonePageCodec;

impl DecodeBody for NonClonePageCodec {
    type Output = Vec<NonCloneItem>;

    const ACCEPT: Option<&'static str> = None;

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Ok(())
    }

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

pub struct OneNonClonePage {
    pub marker: Cell<()>,
}

#[allow(
    clippy::declare_interior_mutable_const,
    reason = "the fixture intentionally constructs a Send but non-Sync controller"
)]
pub const NOT_SYNC_MARKER: Cell<()> = Cell::new(());

impl PaginationController<Vec<NonCloneItem>> for OneNonClonePage {
    type Item = NonCloneItem;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

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

api! {
    client CursorApi {
        timeout 30s;
        base "http://pagination-cursor.example.test/v1/";


    GET items(cursor?: String) -> crate::generated::pagination::VendorPage via crate::VendorPageCodec {
        path ["items"];
        query {
            "cursor" = endpoint.cursor;
        }
        paginate via crate::support::CursorController<String> {
            cursor -> endpoint.cursor;
        }
    }
}}

static CUSTOM_VALIDATE_CALLS: AtomicUsize = AtomicUsize::new(0);
static CUSTOM_NEXT_CALLS: AtomicUsize = AtomicUsize::new(0);

pub struct VendorController {
    pub cursor: Option<String>,
    pub region: String,
}

impl PaginationController<VendorPage> for VendorController {
    type Item = u32;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn validate(&self) -> Result<(), PaginationError> {
        CUSTOM_VALIDATE_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn advance(
        self,
        page: PageResponse<VendorPage>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        CUSTOM_NEXT_CALLS.fetch_add(1, Ordering::SeqCst);
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

pub struct ScopeBindingController {
    pub cursor: Option<String>,
    pub region: String,
    pub repository: String,
}

impl PaginationController<VendorPage> for ScopeBindingController {
    type Item = u32;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

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

api! {
    client CustomApi {
        timeout 30s;
        base "http://pagination-custom.example.test/v1/";


    GET items(cursor?: String, region: String) -> crate::generated::pagination::VendorPage via crate::VendorPageCodec {
        path ["items"];
        query { "cursor" = endpoint.cursor; }
        headers { "x-region" = endpoint.region; }
        paginate via crate::VendorController {
            cursor -> endpoint.cursor;
            region = endpoint.region;
        }
    }
}}

pub struct ZeroController {}

impl PaginationController<VendorPage> for ZeroController {
    type Item = u32;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn advance(
        self,
        page: PageResponse<VendorPage>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        Ok(PageStep::Done {
            items: bounded_items(page.into_page().items, limits)?,
        })
    }
}

api! {
    client ZeroApi {
        timeout 30s;
        base "http://pagination-zero.example.test/v1/";


    GET items() -> crate::generated::pagination::VendorPage via crate::VendorPageCodec {
        paginate via crate::ZeroController {}
    }
}}

api! {
    client NonCloneApi {
        timeout 30s;
        base "http://pagination-non-clone.example.test/v1/";


    GET items() -> Vec<crate::generated::pagination::NonCloneItem> via crate::NonClonePageCodec {
        paginate via crate::OneNonClonePage {
            marker = crate::NOT_SYNC_MARKER;
        }
    }
}}

api! {
    client NestedPaginationApi {
        timeout 30s;
        base "http://pagination-nested.example.test/v1/";


    scope organizations(#[sensitive] region: String) {
        path ["organizations", organizations.region];
        scope repositories(repository: String) {
            path ["repositories", repositories.repository];
            GET fetch(cursor?: String) -> crate::generated::pagination::VendorPage via crate::VendorPageCodec {
                path ["entries"];
                query { "cursor" = endpoint.cursor; }
                headers { "x-region" = organizations.region; }
                paginate via crate::ScopeBindingController {
                    cursor -> endpoint.cursor;
                    region = organizations.region;
                    repository = repositories.repository;
                }
            }
        }
    }
}}

api! {
    client CollisionPaginationApi {
        timeout 30s;
        base "http://pagination-collision.example.test/v1/";


    scope left() {
        path ["left"];
        GET items(offset: u64, limit: std::num::NonZeroU32) -> Vec<u8> via crate::support::Json {
            paginate via crate::support::OffsetController {
                offset -> endpoint.offset;
                page_size -> endpoint.limit;
            }
        }
    }
    scope right() {
        path ["right"];
        GET items(
            page: std::num::NonZeroU64,
            per_page: std::num::NonZeroU32,
        ) -> Vec<u8> via crate::support::Json {
            paginate via crate::support::PageController {
                page -> endpoint.page;
                page_size -> endpoint.per_page;
            }
        }
    }
}}

api! {
    client PaginationRateApi {
        timeout 30s;
        base "http://pagination-rate.example.test/v1/";


    GET items(offset: u64, limit: std::num::NonZeroU32) -> Vec<u32> via crate::support::Json {
        subdomain replace [endpoint.offset];
        path ["items", endpoint.offset];
        query {
            "offset" = endpoint.offset;
            "limit" = endpoint.limit;
        }
        headers { "X-Page-Offset" = endpoint.offset; }
        rate_limit { id "concord.pagination.1"; key [endpoint]; cost 1;  allow 100 per 1s; }
        paginate via crate::support::OffsetController {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
        }
    }
}}

mod repeated_nested_expansion {
    use concord::api;

    api! {
        client NestedMacroPaginationApi {
            timeout 30s;
            base "http://pagination-nested-macro.example.test/v1/";


        GET items(offset: u64, limit: std::num::NonZeroU32) -> Vec<u8> via crate::support::Json {
            paginate via crate::support::OffsetController {
                offset -> endpoint.offset;
                page_size -> endpoint.limit;
            }
        }
    }}
}

#[derive(Default)]
struct PaginationHooks {
    before: Mutex<Vec<String>>,
    response: Mutex<Vec<String>>,
    completion: Mutex<Vec<String>>,
    aggregate: Mutex<Vec<GeneratedAggregateRecord>>,
}

type GeneratedAggregateRecord = (u32, usize, bool, Option<u16>);
type GeneratedRateKeyPart = (String, Vec<u8>);
type GeneratedLimiterContext = (u32, String, Option<String>, Vec<GeneratedRateKeyPart>);

#[derive(Default)]
struct PaginationLimiterProbe {
    contexts: Mutex<Vec<GeneratedLimiterContext>>,
    observations: Mutex<Vec<GeneratedLimiterContext>>,
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

impl RateLimiter for PaginationLimiterProbe {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let parts = request
            .plan
            .policies()
            .iter()
            .flat_map(|rule| rule.key().parts())
            .map(|part| (part.name().to_owned(), part.value().to_owned()))
            .collect();
        self.contexts.lock().expect("limiter contexts").push((
            request.page_index,
            request.observed_url.to_owned(),
            request.host.map(str::to_owned),
            parts,
        ));
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe(&self, response: RateLimitResponse<'_>) {
        let request = response.request;
        let parts = request
            .plan
            .policies()
            .iter()
            .flat_map(|rule| rule.key().parts())
            .map(|part| (part.name().to_owned(), part.value().to_owned()))
            .collect();
        self.observations
            .lock()
            .expect("limiter observations")
            .push((
                request.page_index,
                request.observed_url.to_owned(),
                request.host.map(str::to_owned),
                parts,
            ));
    }
}

impl OperationObserver for PaginationHooks {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::AttemptStarted(context) => {
                self.before
                    .lock()
                    .expect("before lock")
                    .push(format!("{} {:?}", context.observed_url, context.headers));
            }
            OperationEvent::ResponseHeadObserved(context) => {
                self.response.lock().expect("response lock").push(format!(
                    "{} content_length={:?} server_delay={:?}",
                    context.observed_url, context.content_length, context.server_delay
                ));
            }
            OperationEvent::OperationFinished(context) => {
                self.completion
                    .lock()
                    .expect("completion lock")
                    .push(context.observed_url.to_owned());
            }
            OperationEvent::PaginationFinished(context) => {
                self.aggregate.lock().expect("aggregate lock").push((
                    context.pages_completed,
                    context.items_collected,
                    context.recovered_authentication,
                    context.final_status.map(|status| status.as_u16()),
                ));
            }
            _ => {}
        }
    }
}

#[cfg(feature = "json")]
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

#[cfg(feature = "json")]
#[tokio::test]
async fn repeatable_json_body_is_prepared_once_per_page_and_reused_for_policy_retry() {
    let server = ScriptedServer::start([
        ResponseScript::status(503, b"retry".to_vec()),
        ResponseScript::ok(br#"[1,2]"#.to_vec()).with_header("content-type", "application/json"),
        ResponseScript::status(404, br#"[3]"#.to_vec())
            .with_header("content-type", "application/json"),
    ])
    .expect("repeatable JSON pagination server");
    let proxy = SafeProxy::parse(server.url("")).expect("repeatable JSON proxy");
    let probe = Arc::new(BodyPaginationProbe::default());
    let limiter: Arc<dyn RateLimiter> = probe.clone();
    let observation = probe.clone();
    let payload = "x".repeat(2_048);
    let encoded = format!("\"{payload}\"");
    let body_limit = NonZeroUsize::new(encoded.len()).expect("positive JSON body limit");
    let identity_limit =
        NonZeroUsize::new(1_024).expect("positive identity limit below stable body bytes");
    let client = BodyPaginationApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .retry(retry_rejected_statuses())
        .configure_runtime(move |runtime| {
            runtime
                .operation_observer(observation.clone())
                .operation_observer(observation.clone())
                .operation_observer(observation)
                .rate_limiter(limiter)
                .request_body_limit(body_limit)
                .pagination_identity_limit(identity_limit)
        })
        .build()
        .expect("repeatable JSON pagination client");
    let encodes = Arc::new(AtomicUsize::new(0));

    let items = client
        .json_pages(
            0,
            nz32(2),
            CountedSearchInput {
                value: payload,
                encodes: Arc::clone(&encodes),
            },
        )
        .paginate(concord::PageLimit::Prefix(3))
        .await
        .expect("body pagination including accepted non-2xx page");

    assert_eq!(items, [1, 2, 3]);
    assert_eq!(encodes.load(Ordering::SeqCst), 2);
    assert_eq!(probe.before.load(Ordering::SeqCst), 3);
    assert_eq!(probe.responses.load(Ordering::SeqCst), 3);
    assert_eq!(probe.acquisitions.load(Ordering::SeqCst), 3);
    assert_eq!(probe.observations.load(Ordering::SeqCst), 3);
    // Each page is a complete core operation and therefore emits its own
    // terminal observation in addition to the aggregate pagination event.
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

#[cfg(feature = "json")]
#[tokio::test]
async fn custom_page_body_factory_opts_in_and_reconstructs_once_per_visible_attempt() {
    let server = ScriptedServer::start([
        ResponseScript::status(503, b"retry".to_vec()),
        ResponseScript::ok(br#"[9]"#.to_vec()).with_header("content-type", "application/json"),
    ])
    .expect("repeatable factory pagination server");
    let proxy = SafeProxy::parse(server.url("")).expect("repeatable factory proxy");
    let client = BodyPaginationApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .retry(retry_rejected_statuses())
        .build()
        .expect("repeatable factory pagination client");
    let metadata_calls = Arc::new(AtomicUsize::new(0));
    let prepare_calls = Arc::new(AtomicUsize::new(0));
    let create_calls = Arc::new(AtomicUsize::new(0));
    let stable_body = Bytes::from_static(b"repeatable-factory-page-body");

    let items = client
        .factory_pages(
            0,
            nz32(2),
            RepeatableFactoryInput {
                bytes: stable_body.clone(),
                metadata_calls: Arc::clone(&metadata_calls),
                prepare_calls: Arc::clone(&prepare_calls),
                create_calls: Arc::clone(&create_calls),
            },
        )
        .paginate(concord::PageLimit::Prefix(2))
        .await
        .expect("custom repeatable factory page");

    assert_eq!(items, [9]);
    assert_eq!(metadata_calls.load(Ordering::SeqCst), 1);
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

#[cfg(feature = "json")]
#[tokio::test]
async fn stable_body_does_not_hide_repeated_public_page_identity() {
    let server = ScriptedServer::start([
        ResponseScript::ok(br#"[1]"#.to_vec()).with_header("content-type", "application/json")
    ])
    .expect("stable-body cycle server");
    let proxy = SafeProxy::parse(server.url("")).expect("stable-body cycle proxy");
    let client = BodyPaginationApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("stable-body cycle client");

    let error = client
        .stable_cycle("same".to_owned(), "stable request body".to_owned())
        .paginate(concord::PageLimit::Prefix(2))
        .await
        .expect_err("repeated public page identity must be rejected");

    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::RepeatedRequest)
    );
    assert_eq!(server.request_count(), 1);
}

#[tokio::test]
async fn generated_offset_pagination_is_directly_awaitable_and_stops_on_short_page() {
    let server = ScriptedServer::start([
        ResponseScript::ok(br#"[1,2]"#.to_vec()).with_header("content-type", "application/json"),
        ResponseScript::ok(br#"[3]"#.to_vec()).with_header("content-type", "application/json"),
    ])
    .expect("pagination server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let client = OffsetApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("generated pagination client");

    let items = client
        .items(0, nz32(2))
        .paginate(concord::PageLimit::Prefix(10))
        .await
        .expect("paginated items");
    assert_eq!(items, [1, 2, 3]);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].target().contains("offset=0&limit=2"));
    assert!(requests[1].target().contains("offset=2&limit=2"));
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
    let proxy = SafeProxy::parse(server.url("")).expect("custom page response proxy");
    let client = CustomResponsePageApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("custom page response client");

    let items = client
        .items(0, nz32(2))
        .paginate(concord::PageLimit::Prefix(10))
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
    let server = ScriptedServer::start([
        ResponseScript::ok(br#"[1,2]"#.to_vec()).with_header("content-type", "application/json"),
        ResponseScript::ok(br#"[3]"#.to_vec()).with_header("content-type", "application/json"),
    ])
    .expect("application count server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let client = ApplicationCountApi::builder()
        .set_policy(StablePolicy("stable-policy"))
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("application count client");

    let pagination = client
        .items(CountedOffset(0), nz32(2))
        .paginate(concord::PageLimit::Prefix(10));
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
    let soft_server = ScriptedServer::start([
        ResponseScript::ok(br#"[1,2]"#.to_vec()).with_header("content-type", "application/json"),
        ResponseScript::ok(br#"[3,4]"#.to_vec()).with_header("content-type", "application/json"),
    ])
    .expect("soft server");
    let soft_proxy = SafeProxy::parse(soft_server.url("")).expect("proxy");
    let soft_client = PageApi::builder()
        .configure_http(move |http| http.proxy(soft_proxy))
        .build()
        .expect("page client");
    let items = soft_client
        .items(nz64(1), nz32(2))
        .paginate(concord::PageLimit::Prefix(2))
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
        .configure_http(move |http| http.proxy(hard_proxy))
        .build()
        .expect("page client");
    let error = hard_client
        .items(nz64(1), nz32(2))
        .paginate(concord::prelude::PageLimit::CompleteWithin(
            std::num::NonZeroU32::MIN,
        ))
        .await
        .expect_err("continuation beyond the complete-within limit");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::CompletionLimitExceeded)
    );
    assert_eq!(hard_server.requests().len(), 1);
}

#[tokio::test]
async fn cursor_and_custom_empty_pages_follow_controller_authority_and_redact_continuations() {
    let cursor_server = ScriptedServer::start([
        ResponseScript::ok(b"|next-cursor".to_vec())
            .with_header("content-type", "application/x-concord-page"),
        ResponseScript::ok(b"2|-".to_vec())
            .with_header("content-type", "application/x-concord-page"),
    ])
    .expect("cursor server");
    let cursor_proxy = SafeProxy::parse(cursor_server.url("")).expect("proxy");
    let cursor_client = CursorApi::builder()
        .configure_http(move |http| http.proxy(cursor_proxy))
        .build()
        .expect("cursor client");
    let cursor_items = cursor_client
        .items()
        .paginate(concord::PageLimit::Prefix(5))
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
    let configured_hooks = hooks.clone();
    let custom_server = ScriptedServer::start([
        ResponseScript::ok(b"|custom-cursor-sentinel-6d91".to_vec())
            .with_header("content-type", "application/x-concord-page"),
        ResponseScript::ok(b"4|-".to_vec())
            .with_header("content-type", "application/x-concord-page"),
    ])
    .expect("custom server");
    let custom_proxy = SafeProxy::parse(custom_server.url("")).expect("proxy");
    let custom_client = CustomApi::builder()
        .configure_http(move |http| http.proxy(custom_proxy))
        .configure_runtime(move |runtime| {
            runtime
                .operation_observer(configured_hooks.clone())
                .operation_observer(configured_hooks.clone())
                .operation_observer(configured_hooks)
        })
        .build()
        .expect("custom client");
    let custom_items = custom_client
        .items("region-header-sentinel-b17".to_owned())
        .paginate(concord::PageLimit::Prefix(5))
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
async fn prefix_zero_skips_all_runtime_work_and_observation() {
    let hooks = Arc::new(PaginationHooks::default());
    let configured_hooks = hooks.clone();
    let client = ZeroApi::builder()
        .configure_runtime(move |runtime| {
            runtime
                .operation_observer(configured_hooks.clone())
                .operation_observer(configured_hooks.clone())
                .operation_observer(configured_hooks)
        })
        .build()
        .expect("zero client");
    let items = client
        .items()
        .paginate(concord::PageLimit::Prefix(0))
        .await
        .expect("zero pages");
    assert!(items.is_empty());
    assert!(hooks.before.lock().expect("before capture").is_empty());
    assert!(
        hooks
            .aggregate
            .lock()
            .expect("aggregate capture")
            .is_empty()
    );
}

#[tokio::test]
async fn built_in_overflow_is_ignored_on_short_stop_and_hard_cap_can_end_naturally() {
    let offset_server = ScriptedServer::start([
        ResponseScript::ok(br#"[5]"#.to_vec()).with_header("content-type", "application/json")
    ])
    .expect("offset overflow server");
    let offset_proxy = SafeProxy::parse(offset_server.url("")).expect("offset proxy");
    let offset_client = OffsetApi::builder()
        .configure_http(move |http| http.proxy(offset_proxy))
        .build()
        .expect("offset overflow client");
    let items = offset_client
        .items(u64::MAX, nz32(2))
        .paginate(concord::PageLimit::Prefix(2))
        .await
        .expect("short page overrides speculative offset overflow");
    assert_eq!(items, [5]);
    assert_eq!(offset_server.requests().len(), 1);

    let page_server = ScriptedServer::start([
        ResponseScript::ok(br#"[6]"#.to_vec()).with_header("content-type", "application/json")
    ])
    .expect("complete-within page server");
    let page_proxy = SafeProxy::parse(page_server.url("")).expect("page proxy");
    let page_client = PageApi::builder()
        .configure_http(move |http| http.proxy(page_proxy))
        .build()
        .expect("complete-within page client");
    let items = page_client
        .items(nz64(u64::MAX), nz32(2))
        .paginate(concord::prelude::PageLimit::CompleteWithin(
            std::num::NonZeroU32::MIN,
        ))
        .await
        .expect("short page naturally terminates at the completion limit");
    assert_eq!(items, [6]);
    assert_eq!(page_server.requests().len(), 1);
}

#[tokio::test]
async fn empty_present_cursor_is_distinct_and_items_need_not_clone() {
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
        .configure_http(move |http| http.proxy(cursor_proxy))
        .build()
        .expect("empty cursor client");
    let items = cursor_client
        .items()
        .paginate(concord::PageLimit::Prefix(3))
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
        .configure_http(move |http| http.proxy(item_proxy))
        .build()
        .expect("non-clone item client");
    let items = item_client
        .items()
        .paginate(concord::PageLimit::Prefix(1))
        .await
        .expect("non-clone items");
    assert_eq!(
        items.into_iter().map(|item| item.0).collect::<Vec<_>>(),
        [7, 8]
    );
}

#[tokio::test]
async fn explicit_endpoint_nested_scope_alias_and_repeated_helpers_have_parity() {
    let explicit_server = ScriptedServer::start([
        ResponseScript::ok(br#"[9]"#.to_vec()).with_header("content-type", "application/json")
    ])
    .expect("explicit server");
    let explicit_proxy = SafeProxy::parse(explicit_server.url("")).expect("explicit proxy");
    let explicit_client = OffsetApi::builder()
        .configure_http(move |http| http.proxy(explicit_proxy))
        .build()
        .expect("explicit client");
    let endpoint = OffsetApi::unbound().items(0, nz32(2));
    let explicit_items = explicit_client
        .request(endpoint)
        .paginate(concord::PageLimit::Prefix(1))
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
    let configured_nested_hooks = nested_hooks.clone();
    let nested_client = NestedPaginationApi::builder()
        .configure_http(move |http| http.proxy(nested_proxy))
        .configure_runtime(move |runtime| {
            runtime
                .operation_observer(configured_nested_hooks.clone())
                .operation_observer(configured_nested_hooks.clone())
                .operation_observer(configured_nested_hooks)
        })
        .build()
        .expect("nested client");
    let repositories = nested_client
        .organizations("region-a".to_owned())
        .repositories("repository-b".to_owned());
    let nested_items = repositories
        .fetch()
        .paginate(concord::PageLimit::Prefix(3))
        .await
        .expect("nested aliased pagination");
    assert_eq!(nested_items, [11]);
    let original_snapshot_items = repositories
        .fetch()
        .paginate(concord::PageLimit::Prefix(1))
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
            .contains("/organizations/region-a/repositories/repository-b/entries")
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
    let _: concord::pagination::PaginatedRequest<_> = collision_client
        .left()
        .items(0, nz32(1))
        .paginate(concord::PageLimit::Prefix(1));
    let _: concord::pagination::PaginatedRequest<_> = collision_client
        .right()
        .items(nz64(1), nz32(1))
        .paginate(concord::PageLimit::Prefix(1));

    let nested_macro_client =
        repeated_nested_expansion::NestedMacroPaginationApi::new().expect("nested macro client");
    let _: concord::pagination::PaginatedRequest<_> = nested_macro_client
        .items(0, nz32(1))
        .paginate(concord::PageLimit::Prefix(1));
}

#[tokio::test]
async fn generated_pagination_reuses_lexical_global_rate_plan_and_actual_hosts() {
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
    let configured_hooks = hooks.clone();
    let client = PaginationRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| {
            runtime
                .operation_observer(configured_hooks.clone())
                .operation_observer(configured_hooks.clone())
                .operation_observer(configured_hooks)
                .rate_limiter(configured_limiter)
        })
        .build()
        .expect("rate pagination client");
    let items = client
        .items(INITIAL.parse().expect("offset"), nz32(2))
        .paginate(concord::PageLimit::Prefix(5))
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
    assert_eq!(contexts[0].3.len(), 1);
    assert_eq!(contexts[0].3, contexts[1].3);
    assert_eq!(contexts[0].3[0].0, "endpoint");
    for (_, observed_url, _, parts) in contexts.iter() {
        let rendered_safe = format!("{observed_url}\n{:?}", hooks.before.lock().expect("hooks"));
        assert!(observed_url.contains("x.pagination-rate.example.test"));
        assert!(!rendered_safe.contains(INITIAL));
        assert!(!rendered_safe.contains(NEXT));
        assert!(
            parts
                .iter()
                .all(|(_, value)| value.as_slice() != b"<pagination>")
        );
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
