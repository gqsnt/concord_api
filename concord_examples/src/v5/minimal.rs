use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
}

api! {
    client MinimalApi {
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

    scope users {
        path ["users"]

        GET GetUser(id: u64)
            as get
            path [id]
            -> Json<User>
    }
}

pub async fn facade_first(api: minimal_api::MinimalApi) -> Result<User, ApiClientError> {
    api.users().get(42).await
}
