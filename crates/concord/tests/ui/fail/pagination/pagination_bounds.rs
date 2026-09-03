use concord::api;
use concord::codec::{Bytes, CodecError, DecodeBody, DecodeContext};
use concord::pagination::{PaginationController, PaginationError};

#[path = "../../../support/mod.rs"]
mod support;
#[allow(unused_imports)]
use support::Json;

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
        timeout off;
        base "https://example.test";


    GET read() -> crate::NotIterable using crate::NotIterableCodec {
        paginate with crate::Controller {}
    }
}}

api! {
    client MissingCursorExtraction {
        timeout off;
        base "https://example.test";


    GET read(cursor: Option<String>) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::support::CursorController<String> {
            cursor -> endpoint.cursor;
        }
    }
}}

api! {
    client NonSendControllerApi {
        timeout off;
        base "https://example.test";


    GET read(state: std::rc::Rc<()>) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::NonSendController {
            state -> endpoint.state;
        }
    }
}}

fn main() {}
