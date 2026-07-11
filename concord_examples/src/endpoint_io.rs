use bytes::Bytes;
use concord_core::advanced::{MultipartBody, OctetStream, StreamBody, StreamResponse};
use concord_core::prelude::{Json, Text};
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct UploadResult {
    pub id: u64,
}

api! {
    client EndpointIoApi {
        base "https://api.example.com"
    }

    GET JsonResponse
        as json_response
        path ["json"]
        -> Json<UploadResult>

    GET TextResponse
        as text_response
        path ["text"]
        -> Text<String>

    GET NoContentResponse
        as no_content_response
        path ["no-content"]
        -> NoContent

    GET BytesResponse
        as bytes_response
        path ["bytes"]
        -> Bytes

    POST UploadStream(body: Stream<OctetStream>)
        as upload_stream
        path ["stream", "upload"]
        -> Json<UploadResult>

    GET DownloadStream
        as download_stream
        path ["stream", "download"]
        -> Stream<OctetStream>

    POST UploadMultipart(body: Multipart<()>)
        as upload_multipart
        path ["multipart", "upload"]
        -> Json<UploadResult>
}

pub use self::endpoint_io_api::{EndpointIoApi, endpoints};

pub async fn json_example(
    api: EndpointIoApi,
) -> Result<UploadResult, concord_core::prelude::ApiClientError> {
    api.json_response().execute().await
}

pub async fn text_example(
    api: EndpointIoApi,
) -> Result<String, concord_core::prelude::ApiClientError> {
    api.text_response().execute().await
}

pub async fn no_content_example(
    api: EndpointIoApi,
) -> Result<(), concord_core::prelude::ApiClientError> {
    api.no_content_response().execute().await
}

pub async fn bytes_example(
    api: EndpointIoApi,
) -> Result<::bytes::Bytes, concord_core::prelude::ApiClientError> {
    api.bytes_response().execute().await
}

pub async fn stream_examples(
    api: EndpointIoApi,
) -> Result<UploadResult, concord_core::prelude::ApiClientError> {
    let request = api.upload_stream(StreamBody::from_bytes(Bytes::from_static(b"stream")));
    let _: StreamResponse<OctetStream> = api.download_stream().execute_stream().await?;
    request.execute().await
}

pub async fn multipart_form_data_example(
    api: EndpointIoApi,
) -> Result<UploadResult, concord_core::prelude::ApiClientError> {
    let body = MultipartBody::new()
        .text("title", "hello")
        .bytes("file", Bytes::from_static(b"abc"));
    api.upload_multipart(body).execute().await
}
