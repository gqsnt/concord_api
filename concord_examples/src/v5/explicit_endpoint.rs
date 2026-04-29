use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
}

api! {
    client ExplicitEndpointApi {
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

pub async fn advanced_explicit_endpoint(
    api: explicit_endpoint_api::ExplicitEndpointApi,
) -> Result<User, ApiClientError> {
    let endpoint = explicit_endpoint_api::endpoints::users::GetUser::new(42);

    api.request(endpoint).execute().await
}
