use concord_core::advanced::{Mixed, MultipartBody, MultipartStream, RawResponsePart};
use concord_core::prelude::Json;
use concord_macros::api;
use self::multipart_mixed_api::MultipartMixedApi;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct UploadResult {
    ok: bool,
}

api! {
    client MultipartMixedApi {
        base "https://example.com"
    }

    POST Upload(body: Multipart<RawResponsePart, Mixed>)
        path ["upload"]
        -> Json<UploadResult>

    GET Download
        path ["download"]
        -> Multipart<RawResponsePart, Mixed>

    POST Mirror(body: Multipart<RawResponsePart, Mixed>)
        path ["mirror"]
        -> Multipart<RawResponsePart, Mixed>
}

async fn usage(api: MultipartMixedApi) {
    let _ = api
        .upload(MultipartBody::new().text("name", "value"))
        .execute()
        .await
        .unwrap();

    let _: MultipartStream<RawResponsePart> = api.download().execute_multipart().await.unwrap();
    let _: MultipartStream<RawResponsePart> = api.download().execute().await.unwrap();
    let _: MultipartStream<RawResponsePart> = api
        .mirror(MultipartBody::new().text("name", "value"))
        .execute_multipart()
        .await
        .unwrap();
}

fn main() {}
