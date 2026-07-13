use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchDto {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUser {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

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
    client DocsDslApi {
        base "https://api.example.com"

        auth {
            secret token: String
            credential api_token = bearer(secret.token)
        }

        policies {
            rate_limit app {
                bucket application by [host] {
                    100 / 1m
                }
            }

            rate_limit match_bucket {
                bucket method by [host, endpoint, match_key] {
                    5 / 1s
                }
            }
        }

        profiles {
            profile read {
                auth bearer api_token
                rate_limit app
            }

            profile scoped_read {}

            profile match_read {
                rate_limit match_bucket
            }
        }

        default {
            profile read
        }
    }

    scope users {
        path ["users"]
        profile scoped_read

        GET Me(trace_id: String)
        path ["me"]
        headers {
            "X-Trace" = fmt["trace-", trace_id]
        }
        -> Json<User>
    }

    GET GetMatch(match_id: String, verbose?: bool)
    path ["matches", match_id]
    query {
        verbose
    }
    rate_limit key match_key = match_id
    profile match_read
    -> Json<MatchDto>

    GET Search(region?: String = "euw1".to_string())
    path ["search"]
    query {
        region
    }
    -> Json<Vec<User>>
}

api! {
    client DocsDslBodyApi {
        base "https://api.example.com"
    }

    POST CreateUser(body: Json<CreateUser>)
    path ["users"]
    -> Json<User>

    POST Login(body: Json<LoginRequest>)
    path ["login"]
    -> Json<AccessToken>
}

api! {
    client DocsDslPaginationApi {
        base "https://api.example.com"
    }

    GET ListItems(start: u64 = 0, count: u64 = 20)
    path ["items"]
    query {
        start
        count
    }
    paginate OffsetLimitPagination {
        offset = start,
        limit = count
    }
    -> Json<Vec<Item>>

    GET ListCursor(cursor?: String, count: u64 = 20)
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
