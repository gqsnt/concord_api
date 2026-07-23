use runtime::api;

pub struct WireCodec;

pub struct PageCodec;

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

impl runtime::advanced::DecodeBody for PageCodec {
    type Output = Vec<u8>;

    const ACCEPT: Option<&'static str> = None;

    fn validate_content_type(
        _content_type: Option<&str>,
    ) -> Result<(), runtime::advanced::CodecError> {
        Ok(())
    }

    fn decode(
        body: runtime::advanced::Bytes,
        _context: runtime::advanced::DecodeContext<'_>,
    ) -> Result<Self::Output, runtime::advanced::CodecError> {
        Ok(body.to_vec())
    }
}

pub struct RenamedController {
    pub cursor: Option<String>,
}

impl runtime::advanced::PaginationController<Vec<u8>> for RenamedController {
    fn next(self, _page: &Vec<u8>) -> Result<Option<Self>, runtime::advanced::PaginationError> {
        Ok(None)
    }
}

api! {
    client RenamedApi {
        base "http://renamed.example.test";
        vars {
            tenant: String;
            locale: Option<String>;
            page_size: u32 = 25;
        }
        credentials {
            secret api_key: String;
            credential application_key = api_key(secret.api_key);
        }
    }

    scope resources(
        scope_value: String,
        region: Option<String>,
        scope_limit: u32 = 50,
    ) {
        path ["scopes", scope.scope_value, "resources"];
        POST ById(
            endpoint_value: String,
            verbose: Option<bool>,
            limit: u32 = 10,
            body: Text,
        ) -> Text {
            path [ep.endpoint_value];
            auth header "X-Api-Key" = application_key;
        }
        POST Custom(body: crate::WireCodec) -> crate::WireCodec;
        POST StreamUpload(body: Stream<OctetStream>) -> Bytes;
        GET StreamDownload() -> Stream<OctetStream>;
        POST MultipartUpload(body: Multipart) -> NoContent;
        GET Pages(cursor: Option<String>) -> crate::PageCodec {
            query { "cursor" = ep.cursor; }
            paginate crate::RenamedController {
                cursor: bind ep.cursor,
            }
        }
    }
}

api! {
    client RenamedRateApi {
        base "http://renamed-rate.example.test";
        vars {
            tenant: String;
        }
        rate_limit { 10 / 1m; }
    }

    GET Read() -> Bytes;
}

pub fn proof() -> Result<(), runtime::prelude::ClientBuildError> {
    let client = RenamedApi::new("tenant".to_owned(), "protected-api-key")?;
    let _ = client.credentials().application_key().credential();
    let request = client
        .resources("renamed-scope".to_owned())
        .region("eu".to_owned())
        .scope_limit(20)
        .reset_scope_limit()
        .by_id("renamed-endpoint".to_owned(), "renamed-body".to_owned())
        .verbose(true)
        .limit(20)
        .reset_limit();
    let _: renamed_api::resources::ByIdRequest = request;
    let endpoint = renamed_api::resources::ById::new(
        "renamed-scope".to_owned(),
        "renamed-endpoint".to_owned(),
        "renamed-body".to_owned(),
    );
    let _: renamed_api::resources::ByIdRequest = client.request(
        endpoint
            .region("eu".to_owned())
            .scope_limit(20)
            .reset_scope_limit()
            .verbose(true)
            .limit(20)
            .reset_limit(),
    );
    let prepared = runtime::advanced::PreparedEndpoint::builder(
        "RenamedApi::prepared",
        runtime::advanced::Method::GET,
    )
    .path_segment("prepared")
    .query_values("value", ["first", "second"])
    .response::<runtime::advanced::Text>()
    .build()
    .expect("valid renamed prepared endpoint");
    let _: runtime::Request<_> = client.request(prepared);
    let raw_request: renamed_api::resources::CustomRequest = client
        .resources("renamed-scope".to_owned())
        .custom("custom-body".to_owned());
    let _: runtime::dangerous::RawResponseRequest<_> = raw_request.raw_response();
    let _: renamed_api::resources::StreamUploadRequest = client
        .resources("renamed-scope".to_owned())
        .stream_upload(runtime::prelude::StreamBody::from_bytes(
            runtime::prelude::Bytes::from_static(b"renamed stream"),
        ));
    let _: renamed_api::resources::StreamDownloadRequest = client
        .resources("renamed-scope".to_owned())
        .stream_download();
    let _: runtime::PaginatedRequest<_> = client
        .resources("renamed-scope".to_owned())
        .pages()
        .paginate(runtime::prelude::PageLimit::Take(1));
    let multipart = runtime::advanced::MultipartBody::new()
        .part(
            "field",
            runtime::advanced::MultipartPart::text("renamed multipart"),
        )
        .expect("valid renamed multipart recipe");
    let _: renamed_api::resources::MultipartUploadRequest = client
        .resources("renamed-scope".to_owned())
        .multipart_upload(multipart);
    let rate_client = RenamedRateApi::builder()
        .tenant("renamed-rate-key-sentinel".to_owned())
        .runtime(|runtime| {
            runtime.rate_limiter(std::sync::Arc::new(
                runtime::advanced::NoopRateLimiter::new(),
            ))
        })
        .build()?;
    let _: renamed_rate_api::ReadRequest = rate_client.read();
    Ok(())
}
