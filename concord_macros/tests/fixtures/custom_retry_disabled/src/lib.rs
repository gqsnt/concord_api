use concord_macros::api;

fn custom_retry() {}

api! {
    client CustomRetryDisabledApi {
        base "http://custom-retry-disabled.example.test";
    }

    GET ping() -> NoContent;
}

pub fn proof() {
    let _ = CustomRetryDisabledApi::builder()
        .configure_http(|http| http.dangerous_custom_retry(custom_retry()))
        .build();
}
