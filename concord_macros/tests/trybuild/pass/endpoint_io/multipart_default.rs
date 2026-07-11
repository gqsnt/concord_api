use concord_core::advanced::MultipartBody;
use concord_core::prelude::Json;
use concord_macros::api;

api! {
    client MultipartDefaultApi { base "https://example.com" }

    POST Upload(body: Multipart<()>)
        path ["upload"]
        -> Json<UploadResult>
}

#[derive(serde::Deserialize)]
pub struct UploadResult;

async fn uses_form_data_request() {
    let api = multipart_default_api::MultipartDefaultApi::new();
    let _ = api
        .upload(MultipartBody::new().text("title", "hello"))
        .execute()
        .await;
}

fn main() {}
