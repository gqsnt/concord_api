use concord_core::advanced::{
    Bytes, CodecError, DecodeBody, DecodeContext, PaginationController, PaginationError,
    PaginationRequest,
};
use concord_macros::api;

pub struct NotIterable;
pub struct NotIterableCodec;

impl DecodeBody for NotIterableCodec {
    type Output = NotIterable;

    const ACCEPT: Option<&'static str> = None;

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode(_body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        Ok(NotIterable)
    }
}

#[derive(Clone)]
pub struct Controller;

impl PaginationController<NotIterable> for Controller {
    fn apply(&self, _request: &mut PaginationRequest<'_>) -> Result<(), PaginationError> {
        Ok(())
    }

    fn next(&self, _page: &NotIterable) -> Result<Option<Self>, PaginationError> {
        Ok(None)
    }
}

pub fn controller() -> Controller {
    Controller
}

api! {
    client MissingIntoIterator {
        base "https://example.test";
        GET Read() -> crate::NotIterableCodec {
            paginate custom crate::controller();
        }
    }
}

api! {
    client MissingCursorExtraction {
        base "https://example.test";
        GET Read(cursor: Option<String>) -> Json<Vec<u8>> {
            paginate CursorPagination<String> { cursor = ep.cursor }
        }
    }
}

fn main() {}
