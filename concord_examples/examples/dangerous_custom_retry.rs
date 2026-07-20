#![forbid(unsafe_code)]

use concord_core::Request;
use concord_macros::api;

pub fn bounded_custom_retry() -> concord_core::dangerous::ReqwestRetryBuilder {
    reqwest::retry::never()
}

api! {
    client CustomRetryApi {
        base "https://custom-retry.example.test/";
        retry custom crate::bounded_custom_retry;
        GET Health() -> NoContent;
    }
}

fn main() -> Result<(), concord_core::prelude::ClientBuildError> {
    let client = CustomRetryApi::builder().build()?;
    let _: Request<_> = client.health();
    Ok(())
}
