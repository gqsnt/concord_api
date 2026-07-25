use runtime::api;

const _: usize = runtime::endpoint::MAX_FORMATTED_PUBLIC_VALUE_BYTES;
const _: usize = runtime::endpoint::MAX_RUNTIME_ENDPOINT_COMPONENTS;
const _: usize = runtime::endpoint::MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES;
const _: usize = runtime::auth::MAX_AUTH_PLACEMENTS;
const _: usize = runtime::rate_limit::MAX_RATE_LIMIT_ID_BYTES;
const _: usize = runtime::rate_limit::MAX_RATE_LIMIT_KEY_PARTS;
const _: usize = runtime::rate_limit::MAX_RATE_LIMIT_KEY_PART_NAME_BYTES;
const _: usize = runtime::rate_limit::MAX_RATE_LIMIT_KEY_BYTES;
const _: usize = runtime::rate_limit::MAX_RATE_LIMIT_WINDOWS_PER_RULE;
const _: usize = runtime::rate_limit::MAX_RATE_LIMIT_RULES_PER_PLAN;

pub struct WireCodec;

pub struct PageCodec;

impl runtime::codec::EncodeBody for WireCodec {
    type Input = String;

    const CONTENT_TYPE: Option<&'static str> = Some("application/x-wire");

    fn encode(
        input: Self::Input,
        _context: runtime::codec::EncodeContext<'_>,
    ) -> Result<runtime::Bytes, runtime::codec::CodecError> {
        Ok(runtime::Bytes::from(input))
    }
}

impl runtime::codec::DecodeBody for WireCodec {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/x-wire");

    fn decode(
        body: runtime::Bytes,
        _context: runtime::codec::DecodeContext<'_>,
    ) -> Result<Self::Output, runtime::codec::CodecError> {
        String::from_utf8(body.to_vec()).map_err(|_| {
            runtime::codec::CodecError::new(runtime::codec::CodecErrorKind::Decode)
        })
    }
}

impl runtime::codec::DecodeBody for PageCodec {
    type Output = Vec<u8>;

    const ACCEPT: Option<&'static str> = None;

    fn validate_content_type(
        _content_type: Option<&str>,
    ) -> Result<(), runtime::codec::CodecError> {
        Ok(())
    }

    fn decode(
        body: runtime::Bytes,
        _context: runtime::codec::DecodeContext<'_>,
    ) -> Result<Self::Output, runtime::codec::CodecError> {
        Ok(body.to_vec())
    }
}

pub struct RenamedController {
    pub cursor: Option<String>,
}

impl runtime::pagination::PaginationController<Vec<u8>> for RenamedController {
    fn next(self, _page: &Vec<u8>) -> Result<Option<Self>, runtime::pagination::PaginationError> {
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
        POST by_id(
            endpoint_value: String,
            verbose: Option<bool>,
            limit: u32 = 10,
            #[body] body: Text,
        ) -> Text {
            path [ep.endpoint_value];
            auth header "X-Api-Key" = application_key;
        }
        POST custom(#[body] body: crate::WireCodec) -> crate::WireCodec;
        POST stream_upload(#[body] body: Stream<OctetStream>) -> Bytes;
        GET stream_download() -> Stream<OctetStream>;
        POST multipart_upload(#[body] body: Multipart) -> NoContent;
        GET pages(cursor: Option<String>) -> crate::PageCodec {
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

    GET read() -> Bytes;
}

pub fn proof() -> Result<(), runtime::prelude::ClientBuildError> {
    let client = RenamedApi::builder()
        .tenant("tenant".to_owned())
        .api_key(
            runtime::SecretString::try_new("protected-api-key")
                .expect("bounded protected API key"),
        )
        .build()?;
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
    let runtime_endpoint = runtime::endpoint::RuntimeEndpoint::builder(
        runtime::endpoint::EndpointId::new("renamed_api.runtime")
            .expect("valid renamed runtime endpoint ID"),
        runtime::http::Method::GET,
    )
    .path_segment("prepared")
    .query_values("value", ["first", "second"])
    .response::<runtime::Text>()
    .build()
    .expect("valid renamed runtime endpoint");
    let _: runtime::endpoint::Request<_> = client.request(runtime_endpoint);
    let raw_request: renamed_api::resources::CustomRequest = client
        .resources("renamed-scope".to_owned())
        .custom("custom-body".to_owned());
    let _: runtime::dangerous::RawResponseRequest<_> = raw_request.raw_response();
    let _: renamed_api::resources::StreamUploadRequest = client
        .resources("renamed-scope".to_owned())
        .stream_upload(runtime::body::StreamBody::from_bytes(
            runtime::Bytes::from_static(b"renamed stream"),
        ));
    let _: renamed_api::resources::StreamDownloadRequest = client
        .resources("renamed-scope".to_owned())
        .stream_download();
    let _: runtime::pagination::PaginatedRequest<_> = client
        .resources("renamed-scope".to_owned())
        .pages()
        .paginate(runtime::prelude::PageLimit::Take(1));
    let multipart = runtime::body::MultipartBody::new()
        .part(
            "field",
            runtime::body::MultipartPart::text("renamed multipart")
                .expect("bounded renamed multipart text"),
        )
        .expect("valid renamed multipart recipe");
    let _: renamed_api::resources::MultipartUploadRequest = client
        .resources("renamed-scope".to_owned())
        .multipart_upload(multipart);
    let rate_client = RenamedRateApi::builder()
        .tenant("renamed-rate-key-sentinel".to_owned())
        .configure_runtime(|runtime| {
            runtime.rate_limiter(std::sync::Arc::new(
                runtime::rate_limit::NoopRateLimiter::new(),
            ))
        })
        .build()?;
    let _: renamed_rate_api::ReadRequest = rate_client.read();
    Ok(())
}
