use concord::api;
use concord::pagination::{
    PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
    PaginationError,
};

#[path = "../../../support/mod.rs"]
mod support;
#[allow(unused_imports)]
use support::Json;

pub struct NonSendController {
    pub state: std::rc::Rc<()>,
}

impl PaginationController<Vec<u8>> for NonSendController {
    type Item = u8;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn advance(
        self,
        _page: PageResponse<Vec<u8>>,
        _limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        Ok(PageStep::Done { items: Vec::new() })
    }
}

api! {
    client MissingCursorExtraction {
        timeout 30s;
        base "https://example.test";


    GET read(cursor: Option<String>) -> Vec<u8> via crate::support::Json {
        paginate via crate::support::CursorController<String> {
            cursor -> endpoint.cursor;
        }
    }
}}

api! {
    client NonSendControllerApi {
        timeout 30s;
        base "https://example.test";


    GET read(state: std::rc::Rc<()>) -> Vec<u8> via crate::support::Json {
        paginate via crate::NonSendController {
            state -> endpoint.state;
        }
    }
}}

fn main() {}
