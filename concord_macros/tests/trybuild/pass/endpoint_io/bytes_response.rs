use concord_macros::api;
use self::bytes_response_api::BytesResponseApi;

api! {
    client BytesResponseApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Bytes
}

async fn usage(api: BytesResponseApi) {
    let _: ::bytes::Bytes = api.download().execute().await.unwrap();
}

fn main() {}
