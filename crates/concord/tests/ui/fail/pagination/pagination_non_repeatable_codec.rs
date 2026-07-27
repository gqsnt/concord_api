use concord::api;

#[path = "../../../support/mod.rs"]
mod support;

pub struct Controller {}

impl<Page> concord::pagination::PaginationController<Page> for Controller {
    fn next(
        self,
        _page: &Page,
    ) -> Result<Option<Self>, concord::pagination::PaginationError> {
        Ok(None)
    }
}

api! {
    client NonRepeatableCodecApi {
        base "https://example.test";
    }

    POST read(
        #[body] body: crate::support::Json<String>,
    ) -> crate::support::Json<Vec<u8>> {
        paginate crate::Controller {}
    }
}

fn main() {}
