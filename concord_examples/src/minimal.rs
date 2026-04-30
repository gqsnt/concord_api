use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
}

api! {
    client MinimalApi {
        base https "api.example.com"
    }

    scope users {
        path ["users"]

        GET GetUser(id: u64)
            as get_user
            path [id]
            -> Json<User>
    }
}

pub use self::minimal_api::{MinimalApi, endpoints};

pub async fn minimal_call_example(api: MinimalApi) -> Result<User, ApiClientError> {
    api.users().get_user(42).await
}
