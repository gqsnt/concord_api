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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
}

#[derive(Debug, Clone)]
pub struct GuideAccessToken(pub String);

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
    client DocsDslApi {
        base "https://api.example.com"

        auth {
            secret token: String
            credential api_token = bearer(secret.token)
        }

        policies {
            retry read {
                max_attempts 2
                methods [GET]
                on [429, 500, 502, 503, 504]
                retry_after
            }

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

        behaviors {
            behavior read {
                auth bearer api_token
                retry read
                rate_limit app
            }

            behavior scoped_read {
                retry read
            }

            behavior match_read {
                rate_limit match_bucket
            }
        }

        defaults {
            behavior read
        }
    }

    scope users {
        path ["users"]
        behavior scoped_read

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
    behavior match_read
    -> Json<MatchDto>
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
    -> Json<LoginResponse>
    map GuideAccessToken { GuideAccessToken(r.access_token) }
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
    paginate CursorPagination {
        cursor = cursor,
        per_page = count
    }
    -> Json<CursorPage>
}
