use concord_core::prelude::*;
use concord_macros::api;
use self::usage_maybe_field_api::UsageMaybeFieldApi;

api! {
    client UsageMaybeFieldApi {
        base https "example.com"
    }

    GET Search(filter?: String)
        path ["search"]
        query {
            filter
        }
        -> Json<Vec<String>>
}

async fn bad_usage(api: UsageMaybeFieldApi) -> Result<(), ApiClientError> {
    let _ = api.search().maybe_filter(Some("ranked".to_string())).await?;
    Ok(())
}

fn main() {}
