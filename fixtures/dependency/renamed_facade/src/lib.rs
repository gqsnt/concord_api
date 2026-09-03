use runtime::api;

const _: usize = runtime::endpoint::MAX_FORMATTED_PUBLIC_VALUE_BYTES;
const _: usize = runtime::endpoint::MAX_RUNTIME_ENDPOINT_COMPONENTS;
const _: usize = runtime::endpoint::MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES;
const _: usize = runtime::auth::MAX_AUTH_PLACEMENTS;
const _: usize = runtime::auth::MAX_AUTHORIZATION_SCHEME_BYTES;
const _: usize = runtime::rate_limit::MAX_RATE_LIMIT_ID_BYTES;
const _: usize = runtime::rate_limit::MAX_RATE_LIMIT_KEY_PARTS;
const _: usize = runtime::rate_limit::MAX_RATE_LIMIT_KEY_PART_NAME_BYTES;
const _: usize = runtime::rate_limit::MAX_RATE_LIMIT_KEY_BYTES;
const _: usize = runtime::rate_limit::MAX_RATE_LIMIT_WINDOWS_PER_POLICY;
const _: usize = runtime::rate_limit::MAX_RATE_LIMIT_POLICIES_PER_PLAN;
const _: u32 = runtime::retry::MAX_POLICY_RETRIES_PER_OPERATION;

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
        String::from_utf8(body.to_vec())
            .map_err(|_| runtime::codec::CodecError::new(runtime::codec::CodecErrorKind::Decode))
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
        timeout off;
        base "http://renamed.example.test";
        values {
            tenant: String;
            locale?: String;
            page_size: u32 = 25;
        }
        credentials {
            secret api_key;
            credential application_key = api_key(secret.api_key);
        }


    scope resources(
        scope_value: String,
        region?: String,
        scope_limit: u32 = 50,
    ) {
        path ["scopes", resources.scope_value, "resources"];
        POST by_id(
            endpoint_value: String,
            verbose?: bool,
            limit: u32 = 10,
            body body: String using ::runtime::Text,
        ) -> String using ::runtime::Text {
            path [endpoint.endpoint_value];
            auth { authorization "Token" = application_key; }
        }
        POST custom(body body: String using crate::WireCodec) -> String using crate::WireCodec {
            expose accepted_bytes;
        }
        POST stream_upload(
            body body: ::runtime::body::StreamBody using ::runtime::body::Stream<::runtime::codec::OctetStream>,
        ) -> ::runtime::Bytes using ::runtime::Bytes;
        GET stream_download() -> ::runtime::response::ByteStream using ::runtime::body::Stream<::runtime::codec::OctetStream>;
        POST multipart_upload(
            body body: ::runtime::body::MultipartBody using ::runtime::body::Multipart,
        ) -> () using ::runtime::NoContent;
        POST pages(
            cursor?: String,
            body filter: String using ::runtime::Text,
        ) -> Vec<u8> using crate::PageCodec {
            query { "cursor" = endpoint.cursor; }
            paginate with crate::RenamedController {
                cursor -> endpoint.cursor;
            }
        }
    }
}}

api! {
    client RenamedRateApi {
        timeout off;
        base "http://renamed-rate.example.test";
        values {
            tenant: String;
        }
        rate_limits {
            tenant_reads(tenant) {
                key [tenant];
                allow 10 per 1m;
            }
        }
        rate_limit { allow 10 per 1m; }


    GET read() -> ::runtime::Bytes using ::runtime::Bytes {
        rate_limit tenant_reads(tenant = client.tenant);
    }
}}

pub fn proof() -> Result<(), runtime::prelude::ClientBuildError> {
    let retry_policy = runtime::retry::RetryPolicy::builder()
        .without_budget()
        .classify_fn(|_| runtime::retry::RetryDecision::Stop)
        .build()
        .expect("valid renamed-facade retry policy");
    let client = RenamedApi::builder()
        .set_tenant("tenant".to_owned())
        .api_key(
            runtime::SecretString::try_new("protected-api-key").expect("bounded protected API key"),
        )
        .retry(retry_policy)
        .build()?;
    let _ = client.credentials().application_key().credential();
    let request = client
        .resources("renamed-scope".to_owned())
        .with_region("eu".to_owned())
        .set_scope_limit(20)
        .reset_scope_limit()
        .by_id("renamed-endpoint".to_owned(), "renamed-body".to_owned())
        .with_verbose(true)
        .set_limit(20)
        .reset_limit();
    let _: renamed_api::resources::ByIdRequest = request;
    let endpoint = RenamedApi::unbound()
        .resources("renamed-scope".to_owned())
        .with_region("eu".to_owned())
        .set_scope_limit(20)
        .reset_scope_limit()
        .by_id("renamed-endpoint".to_owned(), "renamed-body".to_owned())
        .with_verbose(true)
        .set_limit(20)
        .reset_limit();
    let _: renamed_api::resources::ByIdRequest = client.request(endpoint);
    let runtime_endpoint = runtime::endpoint::RuntimeEndpoint::builder(
        runtime::endpoint::EndpointId::new("renamed_api.runtime")
            .expect("valid renamed runtime endpoint ID"),
        runtime::http::Method::GET,
    )
    .path("prepared")
    .query_values("value", ["first", "second"])
    .response::<runtime::Text>()
    .build()
    .expect("valid renamed runtime endpoint");
    let _: runtime::endpoint::Request<_> = client.request(runtime_endpoint);
    let bytes_request: renamed_api::resources::CustomRequest = client
        .resources("renamed-scope".to_owned())
        .custom("custom-body".to_owned());
    let _: runtime::endpoint::AcceptedBytesRequest<_> = bytes_request.accepted_bytes();
    let _: renamed_api::resources::StreamUploadRequest =
        client.resources("renamed-scope".to_owned()).stream_upload(
            runtime::body::StreamBody::from_bytes(runtime::Bytes::from_static(b"renamed stream")),
        );
    let _: renamed_api::resources::StreamDownloadRequest = client
        .resources("renamed-scope".to_owned())
        .stream_download();
    let _: runtime::pagination::PaginatedRequest<_> = client
        .resources("renamed-scope".to_owned())
        .pages("renamed-page-body".to_owned())
        .paginate(runtime::prelude::PageLimit::Prefix(1));
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
        .set_tenant("renamed-rate-key-sentinel".to_owned())
        .configure_runtime(|runtime| {
            runtime.rate_limiter(std::sync::Arc::new(
                runtime::rate_limit::NoopRateLimiter::new(),
            ))
        })
        .build()?;
    let _: renamed_rate_api::ReadRequest = rate_client.read();
    Ok(())
}
