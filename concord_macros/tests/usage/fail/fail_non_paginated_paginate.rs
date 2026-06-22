use concord_core::prelude::*;
use concord_macros::api;
use self::usage_non_paginated_api::UsageNonPaginatedApi;

api! {
    client UsageNonPaginatedApi {
        base "https://example.com"
    }

    GET Ping
        path ["ping"]
        -> Json<Vec<String>>
}

async fn bad_usage(api: UsageNonPaginatedApi) -> Result<(), ApiClientError> {
    let _ = api.ping().paginate().collect().await?;
    Ok(())
}

fn main() {}
