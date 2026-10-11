// §14.1: a controller is `Sized + Send + 'static` and owns its page-item
// constraint. A non-Send controller cannot implement the contract, and
// `CursorPagination<C>` requires the page to prove `HasNextCursor<C>`.
use concord::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use concord::pagination::{
    CursorInit, CursorPagination, PageDecisionLimits, PageResponse, PageStep,
    PaginationController, PaginationDescription, PaginationError,
};
use concord::{Bytes, api};

pub struct Lines;

impl DecodeBody for Lines {
    type Output = Vec<String>;

    const ACCEPT: Option<&'static str> = Some("text/plain");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let text = String::from_utf8(body.to_vec()).map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        Ok(text.lines().map(str::to_owned).collect())
    }
}

pub struct NonSendController {
    state: std::rc::Rc<u32>,
}

pub struct NonSendInit {
    pub page: u32,
}

impl PaginationController<Vec<String>> for NonSendController {
    type Item = String;
    type Init = NonSendInit;
    type Cursor = u32;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: Self::Init) -> Result<Self, PaginationError> {
        Ok(Self { state: std::rc::Rc::new(init.page) })
    }

    fn cursor(&self) -> Self::Cursor {
        *self.state
    }

    fn advance(
        self,
        page: PageResponse<Vec<String>>,
        _limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        Ok(PageStep::Done { items: page.into_page() })
    }
}

// `Vec<String>` carries no next cursor: `CursorPagination<String>` cannot
// interpret it.
api! {
    client MissingCursorExtraction {
        base "https://example.test";

        GET read(query cursor?: String) -> Vec<String> via crate::Lines {
            paginate via CursorPagination<String> {
                init CursorInit { cursor: cursor };
                cursor -> cursor;
            }
        }
    }
}

fn main() {
    let client = MissingCursorExtraction::new().unwrap();
    let _ = client.read().pages(1);
}
