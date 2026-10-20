// A custom controller remains usable for ordinary pagination; durable resume
// requires the separate, explicit ResumableController contract.
use concord::pagination::{
    Checkpoint, PageDecisionLimits, PageResponse, PageStep, PaginationController,
    PaginationDescription, PaginationError,
};
use concord::{Empty, api};

pub struct Once {
    page: u32,
}

pub struct OnceInit {
    pub page: u32,
}

impl PaginationController<()> for Once {
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
        _page: PageResponse<()>,
        _limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        Ok(PageStep::Done { items: Vec::new() })
    }
}

api! {
    client NonResumablePages {
        base "http://example.test";

        GET read(query page: u32 = 1) -> () via Empty {
            paginate via crate::Once {
                init crate::OnceInit { page: page };
                cursor -> page;
            }
        }
    }
}

fn resume(client: &NonResumablePages, checkpoint: &Checkpoint) {
    let _ = client.read().pages(1);
    let _ = client.read().resume_pages(checkpoint, 1);
}

fn main() {}
