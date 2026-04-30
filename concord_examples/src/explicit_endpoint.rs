use crate::minimal::{MinimalApi, User, endpoints};
use concord_core::prelude::*;

pub async fn explicit_endpoint_example(api: MinimalApi) -> Result<User, ApiClientError> {
    let endpoint = endpoints::users::GetUser::new(42);
    api.request(endpoint).execute().await
}
