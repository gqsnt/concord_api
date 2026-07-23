use concord_macros::api;

#[path = "../../support/mod.rs"]
mod support;
#[allow(unused_imports)]
use support::Json;

pub struct Controller {}

impl<Page> concord_core::advanced::PaginationController<Page> for Controller {
    fn next(
        self,
        _page: &Page,
    ) -> Result<Option<Self>, concord_core::advanced::PaginationError> {
        Ok(None)
    }
}

api! {
    client RequestBodyPagination {
        base "https://example.test";
    }

    POST Read(body: Text) -> Json<Vec<u8>> {
        paginate crate::Controller {}
    }
}

api! {
    client BytesPagination {
        base "https://example.test";
    }

    GET Read() -> Bytes {
        paginate crate::Controller {}
    }
}

api! {
    client NoContentPagination {
        base "https://example.test";
    }

    GET Read() -> NoContent {
        paginate crate::Controller {}
    }
}

api! {
    client StreamPagination {
        base "https://example.test";
    }

    GET Read() -> Stream<OctetStream> {
        paginate crate::Controller {}
    }
}

fn main() {}
