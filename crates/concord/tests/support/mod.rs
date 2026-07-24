#![allow(dead_code)]

use std::marker::PhantomData;
use std::num::{NonZeroU32, NonZeroU64, NonZeroUsize};

use concord::codec::{Bytes, CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext};
use concord::pagination::{PaginationController, PaginationError, PaginationErrorKind};

/// Test-local codec deliberately named `Json` to prove spelling is not macro semantics.
pub struct Json<T>(PhantomData<fn() -> T>);

impl<T> EncodeBody for Json<T>
where
    T: serde::Serialize + Send + Sync + 'static,
{
    type Input = T;

    const CONTENT_TYPE: Option<&'static str> = Some("application/json");

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        serde_json::to_vec(&input)
            .map(Bytes::from)
            .map_err(|error| {
                drop(error);
                CodecError::new("test JSON encoding failed")
            })
    }
}

impl<T> DecodeBody for Json<T>
where
    T: serde::de::DeserializeOwned + Send + Sync + 'static,
{
    type Output = T;

    const ACCEPT: Option<&'static str> = Some("application/json");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type
            .and_then(|value| value.split(';').next())
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
        {
            Ok(())
        } else {
            Err(CodecError::new("test JSON media mismatch"))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        serde_json::from_slice(&body).map_err(|error| {
            drop(error);
            CodecError::new("test JSON decoding failed")
        })
    }
}

pub struct OffsetController {
    pub offset: u64,
    pub page_size: NonZeroU32,
}

impl<Page> PaginationController<Page> for OffsetController {
    fn next(self, _page: &Page) -> Result<Option<Self>, PaginationError> {
        let offset = self
            .offset
            .checked_add(u64::from(self.page_size.get()))
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        Ok(Some(Self {
            offset,
            page_size: self.page_size,
        }))
    }

    fn short_page_size(&self) -> Option<NonZeroUsize> {
        NonZeroUsize::new(
            usize::try_from(self.page_size.get()).expect("u32 page sizes fit supported targets"),
        )
    }
}

pub struct PageController {
    pub page: NonZeroU64,
    pub page_size: NonZeroU32,
}

impl<Page> PaginationController<Page> for PageController {
    fn next(self, _page: &Page) -> Result<Option<Self>, PaginationError> {
        let page = self
            .page
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        Ok(Some(Self {
            page,
            page_size: self.page_size,
        }))
    }

    fn short_page_size(&self) -> Option<NonZeroUsize> {
        NonZeroUsize::new(
            usize::try_from(self.page_size.get()).expect("u32 page sizes fit supported targets"),
        )
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
    Page: NextCursor<C>,
    C: Clone + Send + 'static,
{
    fn next(self, page: &Page) -> Result<Option<Self>, PaginationError> {
        Ok(page.next_cursor().cloned().map(|cursor| Self {
            cursor: Some(cursor),
        }))
    }
}
