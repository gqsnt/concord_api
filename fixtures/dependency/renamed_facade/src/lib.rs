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
    cursor: Option<String>,
}

/// Public initialization of [`RenamedController`] (v3 controllers take a
/// public `Init` and map one whole `Cursor` slot).
pub struct RenamedStart {
    pub cursor: Option<String>,
}

impl runtime::pagination::PaginationController<Vec<u8>> for RenamedController {
    type Item = u8;
    type Init = RenamedStart;
    type Cursor = Option<String>;

    const DESCRIPTION: runtime::pagination::PaginationDescription =
        runtime::pagination::PaginationDescription::NONE;

    fn start(init: Self::Init) -> Result<Self, runtime::pagination::PaginationError> {
        Ok(Self {
            cursor: init.cursor,
        })
    }

    fn cursor(&self) -> Self::Cursor {
        self.cursor.clone()
    }

    fn advance(
        self,
        page: runtime::pagination::PageResponse<Vec<u8>>,
        _limits: runtime::pagination::PageDecisionLimits,
    ) -> Result<runtime::pagination::PageStep<Self, Self::Item>, runtime::pagination::PaginationError>
    {
        Ok(runtime::pagination::PageStep::Done {
            items: page.into_page(),
        })
    }
}

api! {
    client RenamedApi {
        timeout 30s;
        base "http://renamed.example.test/";
        // v3 values need a reachable semantic use; direct placements keep the
        // v2.1 caller surface (setters) this fixture exercises.
        values {
            header("X-Tenant") tenant: String;
            query locale?: String;
            query page_size: u32 = 25;
        }
        credentials {
            secret api_key;
            credential application_key = api_key(secret.api_key);
        }

        scope resources(
            scope_value: String,
            query region?: String,
            query scope_limit: u32 = 50,
        ) {
            path "scopes/{scope_value}/resources";

            POST by_id(
                endpoint_value: String,
                query verbose?: bool,
                query limit: u32 = 10,
                body body: String via ::runtime::Text,
            ) -> String via ::runtime::Text {
                path "{endpoint_value}";
                auth { authorization "Token" = application_key; }
            }

            POST custom(body body: String via crate::WireCodec) -> String via crate::WireCodec {
                expose accepted_bytes;
            }

            POST stream_upload(
                body body: ::runtime::body::RequestStream via ::runtime::Binary,
            ) -> ::runtime::Bytes via ::runtime::Binary;

            GET stream_download() -> ::runtime::response::ByteStream via ::runtime::Binary;

            POST multipart_upload(
                body body: ::runtime::body::MultipartBody via ::runtime::body::Multipart,
            ) -> () via ::runtime::Empty;

            POST pages(
                query cursor?: String,
                body filter: String via ::runtime::Text,
            ) -> Vec<u8> via crate::PageCodec {
                effect safe;
                paginate via crate::RenamedController {
                    init crate::RenamedStart { cursor: cursor };
                    cursor -> cursor;
                }
            }
        }
    }
}

api! {
    client RenamedRateApi {
        timeout 30s;
        base "http://renamed-rate.example.test/";
        values {
            tenant: String;
        }
        rate_limits {
            policy tenant_reads(tenant) {
                id "renamed.tenant.read";
                // v3 labels are explicit; `"tenant"` is the v2.1 implicit label.
                key ["tenant" = tenant];
                cost 1;
                allow 10 per 1m;
            }
        }
        rate_limit {
            id "renamed.global.read";
            key [origin];
            cost 1;
            allow 10 per 1m;
        }

        GET read() -> ::runtime::Bytes via ::runtime::Binary {
            rate_limit tenant_reads(tenant = client.tenant);
        }
    }
}

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
    // The v3 credential facade is local management (clear/replace).
    let _: Result<(), runtime::auth::AuthError> = client.credentials().clear_application_key();
    // v2.1 exact optional setters `set_x(Some(..))` are v3 `replace_x` (§25.2).
    let request = client
        .resources("renamed-scope".to_owned())
        .replace_region(Some("eu".to_owned()))
        .set_scope_limit(20)
        .reset_scope_limit()
        .by_id("renamed-endpoint".to_owned(), "renamed-body".to_owned())
        .replace_verbose(Some(true))
        .set_limit(20)
        .reset_limit();
    let _: renamed_api::resources::ByIdRequest = request;
    let endpoint = RenamedApi::unbound()
        .resources("renamed-scope".to_owned())
        .replace_region(Some("eu".to_owned()))
        .set_scope_limit(20)
        .reset_scope_limit()
        .by_id("renamed-endpoint".to_owned(), "renamed-body".to_owned())
        .replace_verbose(Some(true))
        .set_limit(20)
        .reset_limit();
    let _: renamed_api::resources::ByIdRequest = client.request(endpoint);
    let runtime_endpoint = runtime::endpoint::RuntimeEndpoint::builder(
        runtime::endpoint::EndpointId::new("renamed_api.runtime")
            .expect("valid renamed runtime endpoint ID"),
        runtime::http::Method::GET,
    )
    .path("prepared")
    .query_each("value", ["first", "second"])
    .response::<runtime::Text, String>()
    .build()
    .expect("valid renamed runtime endpoint");
    // v3 binds runtime endpoints at the owner projection.
    let _: runtime::endpoint::BoundRuntimeRequest<runtime::Text, String> = client
        .bind(runtime_endpoint)
        .expect("renamed runtime endpoint binds to the generated client");
    let bytes_request: renamed_api::resources::CustomRequest = client
        .resources("renamed-scope".to_owned())
        .custom("custom-body".to_owned());
    // v3 erases the endpoint type behind the accepted-bytes terminal.
    let _: runtime::endpoint::AcceptedBytesRequest = bytes_request.accepted_bytes();
    let _: renamed_api::resources::StreamUploadRequest = client
        .resources("renamed-scope".to_owned())
        .stream_upload(runtime::body::RequestStream::from_bytes(
            runtime::Bytes::from_static(b"renamed stream"),
        ));
    let _: renamed_api::resources::StreamDownloadRequest = client
        .resources("renamed-scope".to_owned())
        .stream_download();
    // v2.1 `.paginate(PageLimit::Prefix(1))` is the v3 bounded prefix terminal.
    let _: runtime::pagination::CollectPrefix<_> = client
        .resources("renamed-scope".to_owned())
        .pages("renamed-page-body".to_owned())
        .collect_prefix(1);
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
