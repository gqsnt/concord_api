#![allow(dead_code)]

use std::num::{NonZeroU32, NonZeroU64};

use concord::body::{
    BodyError, BodyErrorKind, BodyKind, LengthHint, PageRequestRepresentation, RequestBody,
    RequestBodyContext, RequestBodyMetadata, RequestContentType, RequestRepresentation,
};
use concord::http::HeaderValue;
use concord::pagination::{
    PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
    PaginationError, PaginationErrorKind,
};
use concord::response::{
    Buffered, PageResponseRepresentation, ResponseBodyError, ResponseBodyErrorKind, ResponseDriver,
    ResponseHead, ResponseRepresentation,
};

/// Test-local codec deliberately named `Json` to prove spelling is not macro semantics.
pub struct Json;

impl<T> RequestRepresentation<T> for Json
where
    T: serde::Serialize + Send + 'static,
{
    fn metadata(
        _input: &T,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        Ok(RequestBodyMetadata {
            length: LengthHint::Unknown,
            content_type: RequestContentType::Exact(HeaderValue::from_static("application/json")),
            kind: BodyKind::Buffered,
        })
    }

    fn prepare(input: T, _context: RequestBodyContext<'_>) -> Result<RequestBody, BodyError> {
        let bytes = serde_json::to_vec(&input).map_err(|error| {
            drop(error);
            BodyError::new(BodyErrorKind::Producer)
        })?;
        RequestBody::bytes(bytes.into()).content_type("application/json")
    }
}

impl<T> PageRequestRepresentation<T> for Json where T: Clone + serde::Serialize + Send + 'static {}

impl<T> ResponseRepresentation<T> for Json
where
    T: serde::de::DeserializeOwned + Send + 'static,
{
    type Driver = Buffered;

    fn accept() -> Option<&'static str> {
        Some("application/json")
    }

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

impl<T> PageResponseRepresentation<T> for Json where T: serde::de::DeserializeOwned + Send + 'static {}

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

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

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

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

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

pub trait NextCursor<C> {
    fn next_cursor(&self) -> Option<&C>;
}

pub struct CursorController<C> {
    pub cursor: Option<C>,
}

impl<Page, C> PaginationController<Page> for CursorController<C>
where
    Page: NextCursor<C> + IntoIterator + Send + 'static,
    Page::Item: Send + 'static,
    C: Clone + Send + 'static,
{
    type Item = Page::Item;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

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
