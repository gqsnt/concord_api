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
    client MultipartPageBodyApi {
        timeout off;
        base "https://example.test";


    POST read(
        body body: ::concord::body::MultipartBody using Multipart,
    ) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller {}
    }
}}

fn main() {}
