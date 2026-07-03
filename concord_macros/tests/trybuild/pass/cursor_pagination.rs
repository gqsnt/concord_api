use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CursorPage {
    pub items: Vec<String>,
    pub next_cursor: Option<String>,
}

impl PageItems for CursorPage {
    type Item = String;

    fn item_count(&self) -> usize {
        self.items.len()
    }

    fn into_items(self) -> Vec<Self::Item> {
        self.items
    }
}

impl HasNextCursor for CursorPage {
    type Cursor = String;

    fn next_cursor(&self) -> Option<Self::Cursor> {
        self.next_cursor.clone()
    }
}

api! {
    client CursorEndpointStateApi {
        base "https://example.com"
    }

    GET List(cursor?: String, count: u64 = 2)
        as list
        headers {
            "X-Cursor" = cursor,
            "X-Count" = count,
        }
        paginate CursorPagination<String> {
            cursor = cursor,
            per_page = count
        }
        -> Json<CursorPage>
}

fn main() {}
