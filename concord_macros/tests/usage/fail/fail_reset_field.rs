use concord_core::prelude::*;
use concord_macros::api;
use self::usage_reset_field_api::UsageResetFieldApi;

api! {
    client UsageResetFieldApi {
        base "https://example.com"
    }

    GET Search(count: u32 = 20)
        path ["search"]
        query {
            count
        }
        -> Json<Vec<String>>
}

async fn bad_usage(api: UsageResetFieldApi) -> Result<(), ApiClientError> {
    let _ = api.search().reset_count().await?;
    Ok(())
}

fn main() {}
