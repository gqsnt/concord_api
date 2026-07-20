use concord_core::advanced::{
    Bytes, CodecError, DecodeBody, DecodeContext, PaginationController, PaginationError,
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

pub struct Controller {}

impl PaginationController<NotIterable> for Controller {
    fn next(self, _page: &NotIterable) -> Result<Option<Self>, PaginationError> {
        Ok(None)
    }
}

pub struct NonSendController {
    pub state: std::rc::Rc<()>,
}

impl PaginationController<Vec<u8>> for NonSendController {
    fn next(self, _page: &Vec<u8>) -> Result<Option<Self>, PaginationError> {
        Ok(None)
    }
}

api! {
    client MissingIntoIterator {
        base "https://example.test";
        GET Read() -> crate::NotIterableCodec {
            paginate crate::Controller {}
        }
    }
}

api! {
    client MissingCursorExtraction {
        base "https://example.test";
        GET Read(cursor: Option<String>) -> Json<Vec<u8>> {
            paginate concord_core::advanced::CursorPagination<String> {
                cursor: bind ep.cursor,
            }
        }
    }
}

api! {
    client NonSendControllerApi {
        base "https://example.test";
        GET Read(state: std::rc::Rc<()>) -> Json<Vec<u8>> {
            paginate crate::NonSendController {
                state: bind ep.state,
            }
        }
    }
}

fn main() {}
