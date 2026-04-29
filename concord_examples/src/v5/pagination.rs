use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
}

api! {
    client V5PaginationApi {
        base https "example.com"

        default {
            retry read
        }

        retry read {
            max_attempts 2
            methods [GET]
            on [429, 500]
            retry_after
        }
    }

    scope events {
        path ["events"]

        GET ListEvents(start: u64 = 0, count: u64 = 50, kind?: String)
            as list
            -> Json<Vec<Event>>
        {
            query {
                start
                count
                kind
            }
            paginate OffsetLimitPagination {
                offset = start,
                limit = count
            }
        }
    }
}

pub async fn collect_events(
    api: v5_pagination_api::V5PaginationApi,
) -> Result<Vec<Event>, ApiClientError> {
    api.events()
        .list()
        .kind("deploy".to_string())
        .paginate()
        .max_items(500)
        .collect()
        .await
}
