use concord_core::advanced::{JsonSse, SseStream};
use concord_macros::api;
use self::sse_explicit_api::SseExplicitApi;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MyEvent {
    id: u64,
    msg: String,
}

api! {
    client SseExplicitApi {
        base "https://example.com"
    }

    GET Events
        path ["events"]
        -> Sse<MyEvent, JsonSse>
}

async fn usage(api: SseExplicitApi) {
    let _: SseStream<MyEvent> = api.events().execute_sse().await.unwrap();
    let _: SseStream<MyEvent> = api.events().execute().await.unwrap();
}

fn main() {}
