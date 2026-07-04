use concord_core::advanced::SseStream;
use concord_macros::api;
use self::sse_default_api::SseDefaultApi;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MyEvent {
    id: u64,
    msg: String,
}

api! {
    client SseDefaultApi {
        base "https://example.com"
    }

    GET Events
        path ["events"]
        -> Sse<MyEvent>
}

async fn usage(api: SseDefaultApi) {
    let _: SseStream<MyEvent> = api.events().execute_sse().await.unwrap();
    let _: SseStream<MyEvent> = api.events().execute().await.unwrap();
}

fn main() {}
