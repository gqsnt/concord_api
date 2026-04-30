use concord_core::prelude::*;
use concord_macros::api;
use self::usage_params_api::UsageParamsApi;

api! {
    client UsageParamsApi { base https "example.com" }

    scope users {
        path ["users"]

        GET Get(id: u64)
            as get
            path [id]
            -> Json<String>
    }

    GET Search(q: String, filter?: String, count: u32 = 20)
        as search
        path ["search"]
        query {
            q
            filter
            count
        }
        -> Json<Vec<String>>
}

async fn param_usage(api: UsageParamsApi) -> Result<(), ApiClientError> {
    let _ = api.users().get(42).await?;

    let _ = api
        .search("zed".to_string())
        .filter("ranked".to_string())
        .filter_opt(Some("solo".to_string()))
        .filter_opt(None)
        .clear_filter()
        .count(100)
        .count_opt(Some(50))
        .count_opt(None)
        .clear_count()
        .await?;

    Ok(())
}

fn main() {}
