use concord_macros::api;

pub struct WireCodec;

impl runtime::advanced::EncodeBody for WireCodec {
    type Input = String;

    const CONTENT_TYPE: Option<&'static str> = Some("application/x-wire");

    fn encode(
        input: Self::Input,
        _context: runtime::advanced::EncodeContext<'_>,
    ) -> Result<runtime::advanced::Bytes, runtime::advanced::CodecError> {
        Ok(runtime::advanced::Bytes::from(input))
    }
}

impl runtime::advanced::DecodeBody for WireCodec {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/x-wire");

    fn decode(
        body: runtime::advanced::Bytes,
        _context: runtime::advanced::DecodeContext<'_>,
    ) -> Result<Self::Output, runtime::advanced::CodecError> {
        String::from_utf8(body.to_vec())
            .map_err(|_| runtime::advanced::CodecError::new("wire decode failed"))
    }
}

api! {
    client RenamedApi {
        base "http://renamed.example.test";
        scope resources(scope_value: String) {
            path ["scopes", scope.scope_value, "resources"];
            POST ById(endpoint_value: String, body: Text) -> Text {
                path [ep.endpoint_value];
            }
            POST Custom(body: crate::WireCodec) -> crate::WireCodec;
            POST StreamUpload(body: Stream<OctetStream>) -> Bytes;
            GET StreamDownload() -> Stream<OctetStream>;
            POST MultipartUpload(body: Multipart) -> NoContent;
        }
    }
}

pub fn proof() -> Result<(), runtime::prelude::ClientBuildError> {
    let client = RenamedApi::new()?;
    let request = client
        .resources("renamed-scope".to_owned())
        .by_id("renamed-endpoint".to_owned(), "renamed-body".to_owned());
    let _: runtime::Request<_> = request;
    let endpoint = renamed_api::endpoints::resources::ById::new(
        "renamed-scope".to_owned(),
        "renamed-endpoint".to_owned(),
        "renamed-body".to_owned(),
    );
    let _: runtime::Request<_> = client.request(endpoint);
    let _: runtime::Request<_> = client
        .resources("renamed-scope".to_owned())
        .custom("custom-body".to_owned());
    let _: runtime::Request<_> = client
        .resources("renamed-scope".to_owned())
        .stream_upload(runtime::prelude::StreamBody::from_bytes(
            runtime::prelude::Bytes::from_static(b"renamed stream"),
        ));
    let _: runtime::Request<_> = client
        .resources("renamed-scope".to_owned())
        .stream_download();
    let multipart = runtime::advanced::MultipartBody::new().part(
        "field",
        runtime::advanced::MultipartPart::text("renamed multipart"),
    )
    .expect("valid renamed multipart recipe");
    let _: runtime::Request<_> = client
        .resources("renamed-scope".to_owned())
        .multipart_upload(multipart);
    Ok(())
}
