// REP-02 / §14.1: page eligibility depends on the complete `Buffered` driver
// (plus the controller's own page constraint), not on an iterable-output
// marker. Streaming and discard drivers cannot page. (v2 additionally
// rejected buffered `Bytes`/`()` pages for lacking a page marker; v3 accepts
// them with a compatible controller — see pass/language/buffered_page_modes.rs.)
use concord::pagination::{
    PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
    PaginationError,
};
use concord::{Binary, ByteStream, Discard, api};

pub struct Once {
    page: u32,
}

pub struct OnceInit {
    pub page: u32,
}

impl<Page> PaginationController<Page> for Once
where
    Page: Send + 'static,
{
    type Item = ();
    type Init = OnceInit;
    type Cursor = u32;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: Self::Init) -> Result<Self, PaginationError> {
        Ok(Self { page: init.page })
    }

    fn cursor(&self) -> Self::Cursor {
        self.page
    }

    fn advance(
        self,
        _page: PageResponse<Page>,
        _limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        Ok(PageStep::Done { items: Vec::new() })
    }
}

api! {
    client StreamPagination {
        base "https://example.test";

        GET read(query page: u32 = 1) -> ByteStream via Binary {
            paginate via crate::Once {
                init crate::OnceInit { page: page };
                cursor -> page;
            }
        }
    }
}

api! {
    client DiscardPagination {
        base "https://example.test";

        GET read(query page: u32 = 1) -> () via Discard {
            paginate via crate::Once {
                init crate::OnceInit { page: page };
                cursor -> page;
            }
        }
    }
}

fn main() {}
