use concord_macros::api;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Page {
    pub items: Vec<String>,
}

pub struct HeaderCursorPagination;

api! {
    client BadCustomPaginationApi { base https "example.com" }

    GET List
        as list
        path ["items"]
        paginate HeaderCursorPagination {
            cursor = "next"
        }
        -> concord_core::prelude::Json<Page>
}

fn main() {}
