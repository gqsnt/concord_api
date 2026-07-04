use concord_core::advanced::{MultipartBody, MultipartStream, RawResponsePart};
use concord_core::prelude::Json;
use concord_macros::api;
use self::multipart_default_api::MultipartDefaultApi;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct UploadResult {
    ok: bool,
}

api! {
    client MultipartDefaultApi {
        base "https://example.com"
    }

    POST Upload(body: Multipart<RawResponsePart>)
        path ["upload"]
        -> Json<UploadResult>

    GET Download
        path ["download"]
        -> Multipart<RawResponsePart>

    POST Mirror(body: Multipart<RawResponsePart>)
        path ["mirror"]
        -> Multipart<RawResponsePart>
}

async fn usage(api: MultipartDefaultApi) {
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
