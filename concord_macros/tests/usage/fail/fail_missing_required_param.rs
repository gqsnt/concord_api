use concord_core::prelude::*;
use concord_macros::api;
use self::usage_missing_required_param_api::UsageMissingRequiredParamApi;

api! {
    client UsageMissingRequiredParamApi { base https "example.com" }

    scope users {
        path ["users"]

        GET Get(id: u64)
            as get
            path [id]
            -> Json<String>
    }
}

async fn bad_usage(api: UsageMissingRequiredParamApi) -> Result<(), ApiClientError> {
    let _ = api.users().get().await?;
    Ok(())
}

fn main() {}
