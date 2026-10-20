//! Shared v3 helpers for the generated-surface integration tests.
//!
//! Every helper is an ordinary downstream implementation of a public Concord
//! extension contract; none of them uses private bridge items.

#![allow(dead_code)]

use std::num::{NonZeroU32, NonZeroU64};

use concord::body::{
    BodyError, BodyErrorKind, PageRequestRepresentation, PreparedBody, RequestBodyContext,
    RequestRepresentation,
};
use concord::http::SafeProxy;
use concord::pagination::{
    PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
    PaginationError, PaginationErrorKind,
};
use concord::response::{
    Buffered, ResponseBodyError, ResponseBodyErrorKind, ResponseDriver, ResponseHead,
    ResponseRepresentation,
};
use concord_test_support::server::ScriptedServer;

/// Test-local codec deliberately named `Json` to prove spelling is not macro semantics.
pub struct Json;

impl<T> RequestRepresentation<T> for Json
where
    T: serde::Serialize + Send + 'static,
{
    fn prepare(input: T, _context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        let bytes = serde_json::to_vec(&input).map_err(|error| {
            drop(error);
            BodyError::new(BodyErrorKind::Producer)
        })?;
        PreparedBody::bytes(bytes.into()).content_type("application/json")
    }
}

impl<T> PageRequestRepresentation<T> for Json where T: Clone + serde::Serialize + Send + 'static {}

impl<T> ResponseRepresentation<T> for Json
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
            .and_then(|value| value.split(';').next())
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
        {
            Ok(())
        } else {
            Err(ResponseBodyError::new(
                ResponseBodyErrorKind::InvalidMediaType,
            ))
        }
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        _head: &ResponseHead<'_>,
    ) -> Result<T, ResponseBodyError> {
        serde_json::from_slice(&body).map_err(|error| {
            drop(error);
            ResponseBodyError::new(ResponseBodyErrorKind::Decode)
        })
    }
}

/// Offset controller whose public fields are also its `init` data.
///
/// Declarations use `init OffsetController { offset: offset, page_size: limit };`
/// and `cursor -> offset;`.
pub struct OffsetController {
    pub offset: u64,
    pub page_size: NonZeroU32,
}

impl<Page> PaginationController<Page> for OffsetController
where
    Page: IntoIterator + Send + 'static,
    Page::Item: Send + 'static,
{
    type Item = Page::Item;
    type Init = Self;
    type Cursor = u64;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: Self) -> Result<Self, PaginationError> {
        Ok(init)
    }

    fn cursor(&self) -> u64 {
        self.offset
    }

    fn advance(
        self,
        response: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let items = collect_page_items(response.into_page(), limits)?;
        if items.len() < self.page_size.get() as usize {
            return Ok(PageStep::Done { items });
        }
        let offset = self
            .offset
            .checked_add(u64::from(self.page_size.get()))
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        Ok(PageStep::Continue {
            items,
            next: Self {
                offset,
                page_size: self.page_size,
            },
        })
    }
}

/// Page-number controller whose public fields are also its `init` data.
///
/// Declarations use `init PageController { page: page, page_size: size };`
/// and `cursor -> page;`.
pub struct PageController {
    pub page: NonZeroU64,
    pub page_size: NonZeroU32,
}

impl<Page> PaginationController<Page> for PageController
where
    Page: IntoIterator + Send + 'static,
    Page::Item: Send + 'static,
{
    type Item = Page::Item;
    type Init = Self;
    type Cursor = NonZeroU64;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: Self) -> Result<Self, PaginationError> {
        Ok(init)
    }

    fn cursor(&self) -> NonZeroU64 {
        self.page
    }

    fn advance(
        self,
        response: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let items = collect_page_items(response.into_page(), limits)?;
        if items.len() < self.page_size.get() as usize {
            return Ok(PageStep::Done { items });
        }
        let page = self
            .page
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        Ok(PageStep::Continue {
            items,
            next: Self {
                page,
                page_size: self.page_size,
            },
        })
    }
}

/// A decoded page that names its continuation cursor.
pub trait NextCursor<C> {
    fn next_cursor(&self) -> Option<&C>;
}

/// Cursor controller whose public field is also its `init` data.
///
/// Declarations use `init CursorController { cursor: after };` and
/// `cursor -> after;` for an optional `after` slot.
pub struct CursorController<C> {
    pub cursor: Option<C>,
}

impl<Page, C> PaginationController<Page> for CursorController<C>
where
    Page: NextCursor<C> + IntoIterator + Send + 'static,
    Page::Item: Send + 'static,
    C: Clone + Send + Sync + 'static,
{
    type Item = Page::Item;
    type Init = Self;
    type Cursor = Option<C>;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: Self) -> Result<Self, PaginationError> {
        Ok(init)
    }

    fn cursor(&self) -> Option<C> {
        self.cursor.clone()
    }

    fn advance(
        self,
        page: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let next = page.page().next_cursor().cloned().map(|cursor| Self {
            cursor: Some(cursor),
        });
        let items = collect_page_items(page.into_page(), limits)?;
        Ok(match next {
            Some(next) => PageStep::Continue { items, next },
            None => PageStep::Done { items },
        })
    }
}

fn collect_page_items<Page>(
    page: Page,
    limits: PageDecisionLimits,
) -> Result<Vec<Page::Item>, PaginationError>
where
    Page: IntoIterator,
{
    let mut items = Vec::new();
    for item in page {
        if items.len() == limits.remaining_items() {
            return Err(PaginationError::new(
                PaginationErrorKind::CollectedItemCeilingExceeded,
            ));
        }
        items.push(item);
    }
    Ok(items)
}

/// Routes a plain-HTTP generated client (no credentials) to a scripted
/// loopback server through the explicit, validated proxy configuration.
pub fn loopback_proxy(server: &ScriptedServer) -> SafeProxy {
    SafeProxy::parse(server.url("")).expect("loopback proxy")
}

/// Compile-time negative trait assertion (the static-assertions technique):
/// `<T as AmbiguousIfImpl<_>>::some_item` is ambiguous, and therefore fails to
/// compile, exactly when `T` implements the probed trait. Used for VALUE-05
/// (body-bearing requests are not `Clone`) and wrong-client binding.
#[macro_export]
macro_rules! assert_not_impl {
    ($ty:ty: $($probe:tt)+) => {
        const _: fn() = || {
            trait AmbiguousIfImpl<A> {
                fn some_item() {}
            }
            impl<T: ?Sized> AmbiguousIfImpl<()> for T {}
            struct Invalid;
            impl<T: ?Sized + $($probe)+> AmbiguousIfImpl<Invalid> for T {}
            let _ = <$ty as AmbiguousIfImpl<_>>::some_item;
        };
    };
}
