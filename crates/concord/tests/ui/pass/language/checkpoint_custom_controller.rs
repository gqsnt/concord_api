// A custom controller can opt into portable checkpoints independently of the
// endpoint declaration, and restore through the generated request method.
use concord::pagination::{
    PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
    PaginationError, PaginationErrorKind, ResumableController,
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

impl ResumableController<()> for Once {
    fn save_state(&self) -> Vec<u8> {
        self.page.to_be_bytes().to_vec()
    }

    fn restore_state(state: &[u8]) -> Result<Self, PaginationError> {
        let bytes = state
            .try_into()
            .map_err(|_| PaginationError::new(PaginationErrorKind::Controller))?;
        Ok(Self {
            page: u32::from_be_bytes(bytes),
        })
    }
}

api! {
    client ResumablePages {
        base "http://example.test";

        GET read(query page: u32 = 1) -> () via Empty {
            paginate via crate::Once {
                init crate::OnceInit { page: page };
                cursor -> page;
            }
        }
    }
}

fn main() {
    let client = ResumablePages::new().expect("client");
    let checkpoint = client
        .read()
        .pages(1)
        .checkpoint()
        .expect("custom checkpoint");
    let resumed = client
        .read()
        .resume_pages(&checkpoint, 1)
        .expect("custom resume");
    assert_eq!(
        resumed.checkpoint().expect("restored checkpoint"),
        checkpoint
    );
}
