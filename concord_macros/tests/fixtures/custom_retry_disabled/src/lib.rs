use concord_macros::api;

fn custom_retry() {}

api! {
    client CustomRetryDisabledApi {
        base "http://custom-retry-disabled.example.test";
        retry custom crate::custom_retry;
    }

    GET Ping() -> NoContent;
}
