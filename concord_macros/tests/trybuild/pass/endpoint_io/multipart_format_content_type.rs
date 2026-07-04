use concord_core::advanced::{
    ContentType, MultipartBody, MultipartFormat, MultipartStream, RawResponsePart,
};
use concord_core::prelude::Json;
use concord_macros::api;
use self::multipart_format_content_type_api::MultipartFormatContentTypeApi;

#[derive(Debug, Default, Clone, Copy)]
pub struct PipeMultipart;

impl ContentType for PipeMultipart {
    const CONTENT_TYPE: &'static str = "multipart/x-pipe";
}

api! {
    client MultipartFormatContentTypeApi {
        base "https://example.com"
    }

    POST Upload(body: Multipart<RawResponsePart, PipeMultipart>)
        path ["upload"]
        -> Json<String>

    GET Download
        path ["download"]
        -> Multipart<RawResponsePart, PipeMultipart>
}

async fn usage(api: MultipartFormatContentTypeApi) {
    let _request = api.upload(MultipartBody::new().text("name", "value"));
    let _response: MultipartStream<RawResponsePart> = api.download().execute_multipart().await.unwrap();
    let _also: MultipartStream<RawResponsePart> = api.download().execute().await.unwrap();
}

impl MultipartFormat for PipeMultipart {}

fn main() {}
