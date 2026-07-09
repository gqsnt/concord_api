use concord_macros::api;
use concord_core::prelude::*;
use self::pending_request_execute_raw_unavailable_api::PendingRequestExecuteRawUnavailableApi;

api! {
    client PendingRequestExecuteRawUnavailableApi {
        base "https://example.com"
    }

    GET User
        path ["user"]
        -> Json<String>
}

async fn usage(api: PendingRequestExecuteRawUnavailableApi) {
    let _ = api.user().execute_raw().await.unwrap();
}

fn main() {}
