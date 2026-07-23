#![forbid(unsafe_code)]

use concord::api;

pub fn bounded_custom_retry() -> concord::dangerous::ReqwestRetryBuilder {
    reqwest::retry::never()
}

api! {
    client CustomRetryApi {
        base "https://custom-retry.example.test/";
        retry custom crate::bounded_custom_retry;
    }

    GET Health() -> NoContent;
}

fn main() -> Result<(), concord::prelude::ClientBuildError> {
    let client = CustomRetryApi::builder().build()?;
    let _: custom_retry_api::HealthRequest = client.health();
    Ok(())
}
