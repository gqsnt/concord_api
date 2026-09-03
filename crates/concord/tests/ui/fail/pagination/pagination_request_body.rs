use concord::api;
use concord::body::RequestBody;

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
    client ManagedPageBodyApi {
        timeout off;
        base "https://example.test";


    POST read(
        body body: RequestBody using RequestBody,
    ) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller {}
    }
}}

fn main() {}
