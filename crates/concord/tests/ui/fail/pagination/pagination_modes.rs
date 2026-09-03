use concord::api;

#[path = "../../../support/mod.rs"]
mod support;
#[allow(unused_imports)]
use support::Json;

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
    client RequestBodyPagination {
        timeout off;
        base "https://example.test";


    POST read(body body: String using Text) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller {}
    }
}}

api! {
    client BytesPagination {
        timeout off;
        base "https://example.test";


    GET read() -> Bytes using Bytes {
        paginate with crate::Controller {}
    }
}}

api! {
    client NoContentPagination {
        timeout off;
        base "https://example.test";


    GET read() -> () using NoContent {
        paginate with crate::Controller {}
    }
}}

api! {
    client StreamPagination {
        timeout off;
        base "https://example.test";


    GET read() -> ::concord::response::ByteStream using Stream<OctetStream> {
        paginate with crate::Controller {}
    }
}}

fn main() {}
