use concord_core::advanced::{FormData, RawResponsePart};
use concord_macros::api;
use self::multipart_endpoint_execute_sse_api::MultipartEndpointExecuteSseApi;

api! {
    client MultipartEndpointExecuteSseApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Multipart<RawResponsePart>
}

async fn usage(api: MultipartEndpointExecuteSseApi) {
    let _ = api.download().execute_sse().await.unwrap();
}

fn main() {}
