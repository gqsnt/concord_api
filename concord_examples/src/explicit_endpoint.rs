use crate::minimal::{MinimalApi, User, endpoints};
#[cfg(feature = "dangerous-raw-response")]
use concord_core::dangerous::BuiltResponse;
use concord_core::prelude::*;

pub async fn explicit_endpoint_example(api: MinimalApi) -> Result<User, ApiClientError> {
    let endpoint = endpoints::users::GetUser::new(42);
    api.request(endpoint).execute().await
}

#[cfg(feature = "dangerous-raw-response")]
pub async fn explicit_endpoint_raw_example(
    api: MinimalApi,
) -> Result<BuiltResponse, ApiClientError> {
    let endpoint = endpoints::users::GetUser::new(42);
    api.request(endpoint).execute_raw_response().await
}
