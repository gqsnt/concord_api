use concord_macros::api;

pub fn custom_retry() -> concord_core::dangerous::ReqwestRetryBuilder {
    reqwest::retry::never()
}

api! {
    client CustomRetryEnabledApi {
        base "http://custom-retry-enabled.example.test";
    }

    GET ping() -> NoContent;
}

pub fn proof() -> Result<(), concord_core::prelude::ClientBuildError> {
    let client = CustomRetryEnabledApi::builder()
        .configure_http(|http| http.dangerous_custom_retry(custom_retry()))
        .build()?;
    let _: custom_retry_enabled_api::PingRequest = client.ping();
    Ok(())
}
