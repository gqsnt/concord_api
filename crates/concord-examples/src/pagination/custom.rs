//! A downstream pagination controller implemented through façade contracts.
//!
//! Required feature: `json`. Mode: construction and deterministic local execution.
//! Documentation owner: `docs/extensions.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.
//!
//! The controller has a public initialization type ([`ExampleStart`]) and
//! private state ([`ExampleController`]); the declaration maps its whole
//! `Option<String>` cursor onto the endpoint's `cursor?: String` slot.

// ANCHOR: pagination_controller_extension
use concord::Bytes;
use concord::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use concord::pagination::{
    CollectPrefix, HasNextCursor, PageDecisionLimits, PageResponse, PageStep, PaginationController,
    PaginationDescription, PaginationError, PaginationErrorKind,
};

use super::PaginationApi;

/// One cursor-bearing page for built-in and custom cursor pagination.
pub struct ExamplePage {
    items: Vec<u32>,
    next: Option<String>,
}

impl IntoIterator for ExamplePage {
    type Item = u32;
    type IntoIter = std::vec::IntoIter<u32>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl HasNextCursor<String> for ExamplePage {
    fn next_cursor(&self) -> Option<&String> {
        self.next.as_ref()
    }
}

/// Decoder for the compact `item,item|next` example page format.
pub struct ExamplePageCodec;

impl DecodeBody for ExamplePageCodec {
    type Output = ExamplePage;

    const ACCEPT: Option<&'static str> = Some("application/vnd.concord.page");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let text =
            std::str::from_utf8(&body).map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        let (items, next) = text
            .split_once('|')
            .ok_or_else(|| CodecError::new(CodecErrorKind::Decode))?;
        let items = items
            .split(',')
            .filter(|value| !value.is_empty())
            .map(|value| {
                value
                    .parse::<u32>()
                    .map_err(|_| CodecError::new(CodecErrorKind::Decode))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ExamplePage {
            items,
            next: (next != "-").then(|| next.to_owned()),
        })
    }
}

/// Public initialization built by `init ExampleStart { cursor: cursor }`.
pub struct ExampleStart {
    /// Cursor of the first request; `None` requests the first page.
    pub cursor: Option<String>,
}

/// Private cursor state; `Cursor = Option<String>`.
pub struct ExampleController {
    cursor: Option<String>,
}

impl PaginationController<ExamplePage> for ExampleController {
    type Item = u32;
    type Init = ExampleStart;
    type Cursor = Option<String>;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: Self::Init) -> Result<Self, PaginationError> {
        Ok(Self {
            cursor: init.cursor,
        })
    }

    fn cursor(&self) -> Self::Cursor {
        self.cursor.clone()
    }

    fn advance(
        self,
        page: PageResponse<ExamplePage>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let next = page.page().next.as_ref().map(|cursor| Self {
            cursor: Some(cursor.clone()),
        });
        let items = page.into_page().items;
        if items.len() > limits.remaining_items() {
            return Err(PaginationError::new(
                PaginationErrorKind::CollectedItemCeilingExceeded,
            ));
        }
        Ok(match next {
            Some(next) => PageStep::Continue { items, next },
            None => PageStep::Done { items },
        })
    }
}

/// Constructs a bounded prefix request via the downstream controller.
pub fn construct(client: &PaginationApi) {
    let _: CollectPrefix<u32> = client.custom().collect_prefix(2);
}
// ANCHOR_END: pagination_controller_extension
