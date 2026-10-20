// REP-02 / §14.1 (positive half of fail/pagination/pagination_modes.rs):
// page eligibility depends on the complete `Buffered` driver and the
// controller's own page constraint, not on an iterable-output marker, so
// buffered `Bytes` and zero-byte `()` pages are page-capable with a compatible
// controller (v2 rejected them for lacking a page marker).
use std::num::NonZeroU32;

use concord::pagination::{
    PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
    PaginationError,
};
use concord::{Binary, Bytes, Empty, api};

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
    client BufferedPages {
        base "http://example.test";

        GET bytes(query page: u32 = 1) -> Bytes via Binary {
            paginate via crate::Once {
                init crate::OnceInit { page: page };
                cursor -> page;
            }
        }

        GET empty(query page: u32 = 1) -> () via Empty {
            paginate via crate::Once {
                init crate::OnceInit { page: page };
                cursor -> page;
            }
        }
    }
}

fn main() {
    let client = BufferedPages::new().expect("client");
    let _ = client.bytes().pages(1);
    let _ = client.bytes().collect_prefix(1);
    let _ = client.empty().pages(1);
    let _ = client.empty().collect_all(NonZeroU32::MIN);
}
