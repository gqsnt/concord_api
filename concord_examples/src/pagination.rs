use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CursorPage {
    pub items: Vec<Item>,
    pub next_cursor: Option<String>,
}

impl PageItems for CursorPage {
    type Item = Item;

    fn item_count_hint(&self) -> Option<usize> {
        Some(self.items.len())
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
    client PaginationApi {
        base "https://example.com"
    }

    GET ListOffset(start: u64 = 0, count: u64 = 2)
        as list_offset
        path ["offset-items"]
        query {
            start
            count
        }
        paginate OffsetLimitPagination {
            offset = start,
            limit = count
        }
        -> Json<Vec<Item>>

    GET ListCursor(cursor?: String, count: u64 = 2)
        as list_cursor
        path ["cursor-items"]
        query {
            cursor
            count
        }
        paginate CursorPagination<String> {
            cursor = cursor,
            per_page = count
        }
        -> Json<CursorPage>
}

api! {
    client PaginationAuthApi {
        base "https://example.com"

        secret token: String
        credential session = bearer(secret.token)
    }

    scope protected {
        auth bearer session

        GET ListProtected(start: u64 = 0, count: u64 = 2)
            as list_protected
            path ["protected-items"]
            query {
                start
                count
            }
            paginate OffsetLimitPagination {
                offset = start,
                limit = count
            }
            -> Json<Vec<Item>>
    }
}

pub use self::pagination_api::PaginationApi;
pub use self::pagination_auth_api::PaginationAuthApi;
