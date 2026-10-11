// PAGE-01 / V3-P02: `init` constructs exactly `Controller::Init` (fields and
// types checked by Rust), and `cursor -> slot` maps the controller's entire
// `Cursor` to the entire effective slot, with no hidden field extraction,
// `Option` wrapping or conversion.
use std::num::NonZeroU32;

use concord::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use concord::pagination::{
    CursorInit, CursorPagination, HasNextCursor, OffsetInit, OffsetPagination, PageDecisionLimits,
    PageResponse, PageStep, PaginationController, PaginationDescription, PaginationError,
};
use concord::{Bytes, api};

pub struct Rows {
    pub rows: Vec<String>,
    pub next: Option<String>,
}

impl HasNextCursor<String> for Rows {
    fn next_cursor(&self) -> Option<&String> {
        self.next.as_ref()
    }
}

impl IntoIterator for Rows {
    type Item = String;
    type IntoIter = std::vec::IntoIter<String>;

    fn into_iter(self) -> Self::IntoIter {
        self.rows.into_iter()
    }
}

pub struct RowsCodec;

impl DecodeBody for RowsCodec {
    type Output = Rows;

    const ACCEPT: Option<&'static str> = Some("text/plain");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let text = String::from_utf8(body.to_vec()).map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        Ok(Rows { rows: text.lines().map(str::to_owned).collect(), next: None })
    }
}

pub struct TupleInit {
    pub page: u32,
    pub token: Option<String>,
}

pub struct TuplePages {
    page: u32,
    token: Option<String>,
}

impl PaginationController<Rows> for TuplePages {
    type Item = String;
    type Init = TupleInit;
    type Cursor = (u32, Option<String>);

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: Self::Init) -> Result<Self, PaginationError> {
        Ok(Self { page: init.page, token: init.token })
    }

    fn cursor(&self) -> Self::Cursor {
        (self.page, self.token.clone())
    }

    fn advance(self, page: PageResponse<Rows>, _limits: PageDecisionLimits) -> Result<PageStep<Self, String>, PaginationError> {
        Ok(PageStep::Done { items: page.into_page().rows })
    }
}

pub struct NotController;

pub const PAGE_SIZE: NonZeroU32 = NonZeroU32::MIN;

// Unknown Init field.
api! {
    client UnknownInitField {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> crate::Rows via crate::RowsCodec {
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE, limit: 10 };
                cursor -> offset;
            }
        }
    }
}

// Missing Init field (no `..Default::default()`).
api! {
    client MissingInitField {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> crate::Rows via crate::RowsCodec {
            paginate via OffsetPagination {
                init OffsetInit { offset: offset };
                cursor -> offset;
            }
        }
    }
}

// Wrong Init field type.
api! {
    client WrongInitFieldType {
        base "https://example.test";

        GET read(query offset: String = "0") -> crate::Rows via crate::RowsCodec {
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

// Wrong Init type for the controller.
api! {
    client WrongInitType {
        base "https://example.test";

        GET read(query offset: u64 = 0, query cursor?: String) -> crate::Rows via crate::RowsCodec {
            paginate via OffsetPagination {
                init CursorInit { cursor: cursor };
                cursor -> offset;
            }
        }
    }
}

// Whole-slot cursor: `CursorPagination<String>` has `Cursor = Option<String>`,
// so the destination must be an optional `String` slot, not a total one.
api! {
    client TotalSlotForOptionalCursor {
        base "https://example.test";

        GET read(query after: String = "") -> crate::Rows via crate::RowsCodec {
            paginate via CursorPagination<String> {
                init CursorInit { cursor: crate::NO_CURSOR };
                cursor -> after;
            }
        }
    }
}

// Tuple cursor arity/types must correspond exactly.
api! {
    client WrongTupleCursor {
        base "https://example.test";

        GET read(query page: u32 = 1, query token?: u64) -> crate::Rows via crate::RowsCodec {
            paginate via crate::TuplePages {
                init crate::TupleInit { page: page, token: crate::NO_CURSOR };
                cursor -> (page, token);
            }
        }
    }
}

api! {
    client TupleForScalarCursor {
        base "https://example.test";

        GET read(query offset: u64 = 0, query other: u64 = 0) -> crate::Rows via crate::RowsCodec {
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE };
                cursor -> (offset, other);
            }
        }
    }
}

// A type that is not a controller.
api! {
    client NonControllerType {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> crate::Rows via crate::RowsCodec {
            paginate via crate::NotController {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

pub const NO_CURSOR: Option<String> = None;

fn main() {}
