//! A downstream pagination controller implemented through façade contracts.
//!
//! Required feature: `json`. Mode: construction and deterministic local proof.
//! Documentation owner: `docs/guide/06-pagination-and-rate-limits.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use concord::pagination::PaginatedRequest;
use concord::pagination::{HasNextCursor, PaginationController, PaginationError};
use concord::{Bytes, PageLimit};

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

/// Custom cursor state bound to the generated endpoint snapshot.
pub struct ExampleController {
    /// Current optional cursor.
    pub cursor: Option<String>,
}

impl PaginationController<ExamplePage> for ExampleController {
    fn next(self, page: &ExamplePage) -> Result<Option<Self>, PaginationError> {
        Ok(page.next.as_ref().map(|cursor| Self {
            cursor: Some(cursor.clone()),
        }))
    }
}

/// Constructs a bounded request using the downstream controller.
pub fn construct(client: &PaginationApi) {
    let _: PaginatedRequest<_> = client.custom().paginate(PageLimit::Take(2));
}
