use concord_macros::api;
use self::bytes_response_map_api::BytesResponseMapApi;

api! {
    client BytesResponseMapApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Bytes
}

async fn usage(api: BytesResponseMapApi) {
    let _ = api.download().execute().await.unwrap();
}

fn main() {}
