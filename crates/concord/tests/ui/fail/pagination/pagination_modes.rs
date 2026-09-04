use concord::{Binary, Bytes, Empty, Text, api};

#[path = "../../../support/mod.rs"]
mod support;
#[allow(unused_imports)]
use support::Json;

pub struct Controller {}

impl<Page> concord::pagination::PaginationController<Page> for Controller
where
    Page: Send + 'static,
{
    type Item = ();

    const DESCRIPTION: concord::pagination::PaginationDescription =
        concord::pagination::PaginationDescription::NONE;

    fn advance(
        self,
        _page: concord::pagination::PageResponse<Page>,
        _limits: concord::pagination::PageDecisionLimits,
    ) -> Result<
        concord::pagination::PageStep<Self, Self::Item>,
        concord::pagination::PaginationError,
    > {
        Ok(concord::pagination::PageStep::Done { items: Vec::new() })
    }
}

api! {
    client RequestBodyPagination {
        timeout 30s;
        base "https://example.test";


    POST read(body body: String via Text) -> Vec<u8> via crate::support::Json {
        effect safe;
        paginate via crate::Controller {}
    }
}}

api! {
    client BytesPagination {
        timeout 30s;
        base "https://example.test";


    GET read() -> Bytes via Binary {
        paginate via crate::Controller {}
    }
}}

api! {
    client NoContentPagination {
        timeout 30s;
        base "https://example.test";


    GET read() -> () via Empty {
        paginate via crate::Controller {}
    }
}}

api! {
    client StreamPagination {
        timeout 30s;
        base "https://example.test";


    GET read() -> ::concord::response::ByteStream via Binary {
        paginate via crate::Controller {}
    }
}}

fn main() {}
