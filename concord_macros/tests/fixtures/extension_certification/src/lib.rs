#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::Duration;

use concord_core::advanced::{
    Auth, AuthError, AuthErrorKind, BeforeSend, BodyError, BodyFactory, BoxFuture, CodecError,
    CompletionEvent, Cooldown, CooldownScope, CredentialMaterial, CredentialProvider,
    CredentialRequest, DecodeBody,
    DecodeContext, EncodeBody, EncodeContext, HasNextCursor, HeaderName, HeaderValue, HookError,
    MediaType, Method, PaginationCompletionEvent, PaginationController, PaginationError,
    PreparedEndpoint, ProviderBody, ProviderBodyFactory, ProviderRequest, RateLimitError,
    RateLimitObserver, RateLimitPermit, RateLimitRequest, RateLimitResponse, RateLimitResponseHead,
    RateLimiter, RequestBody, ResponseHeadEvent, RuntimeHooks, SafeProxy, SecretCredential,
    StatusCode, Url,
};
#[cfg(feature = "json")]
use concord_core::advanced::Json;
#[cfg(feature = "multipart")]
use concord_core::advanced::{MultipartBody, MultipartFactory, MultipartPart};
use concord_core::prelude::{Bytes, ClientBuildError, Request, SecretString, StreamBody};

pub struct ExtensionMedia;

impl MediaType for ExtensionMedia {
    const MEDIA_TYPE: &'static str = "application/x-concord-extension";
}

pub struct ExtensionCodec;

impl EncodeBody for ExtensionCodec {
    type Input = String;

    const CONTENT_TYPE: Option<&'static str> = Some("application/x-concord-extension");

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        Ok(Bytes::from(input))
    }
}

impl DecodeBody for ExtensionCodec {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-extension");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type == Self::ACCEPT {
            Ok(())
        } else {
            Err(CodecError::new("extension media mismatch"))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new("extension UTF-8"))
    }
}

pub struct ExtensionBodyFactory;

impl BodyFactory for ExtensionBodyFactory {
    fn content_type(&self) -> Option<HeaderValue> {
        Some(HeaderValue::from_static(
            "application/x-concord-extension",
        ))
    }

    fn create(&self) -> Result<StreamBody, BodyError> {
        Ok(StreamBody::from_bytes(Bytes::from_static(b"factory")))
    }
}

#[cfg(feature = "multipart")]
pub struct ExtensionMultipartFactory;

#[cfg(feature = "multipart")]
impl MultipartFactory for ExtensionMultipartFactory {
    fn create(&self) -> Result<MultipartBody, BodyError> {
        MultipartBody::new().part("extension", MultipartPart::text("multipart"))
    }
}

#[derive(Clone)]
pub struct ExtensionCredential(SecretString);

impl CredentialMaterial for ExtensionCredential {}

impl SecretCredential for ExtensionCredential {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl std::fmt::Debug for ExtensionCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ExtensionCredential(<redacted>)")
    }
}

pub struct ExtensionProviderBody;

impl ProviderBodyFactory for ExtensionProviderBody {
    fn create(&self) -> Result<Bytes, AuthError> {
        Ok(Bytes::from_static(b"bounded-provider-input"))
    }
}

pub struct ExtensionProvider;

impl CredentialProvider for ExtensionProvider {
    type Material = ExtensionCredential;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async move {
            let request = ProviderRequest::new(
                Method::POST,
                Url::parse("https://provider.example.test/token")
                    .expect("fixed provider URL is valid"),
            )
            .header(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("application/octet-stream"),
            )
            .body(ProviderBody::Factory(Arc::new(ExtensionProviderBody)))
            .timeout(Duration::from_secs(2));
            let response = context.http.send(request).await?;
            let value = String::from_utf8(response.into_body().to_vec())
                .map_err(|_| AuthError::new(AuthErrorKind::Acquisition))?;
            Ok(ExtensionCredential(SecretString::new(value)))
        })
    }
}

pub struct ExtensionPageCodec;

pub struct ExtensionPage {
    items: Vec<u8>,
    next: Option<String>,
}

impl IntoIterator for ExtensionPage {
    type Item = u8;
    type IntoIter = std::vec::IntoIter<u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl HasNextCursor<String> for ExtensionPage {
    fn next_cursor(&self) -> Option<&String> {
        self.next.as_ref()
    }
}

impl DecodeBody for ExtensionPageCodec {
    type Output = ExtensionPage;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-page");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        Ok(ExtensionPage {
            items: body.to_vec(),
            next: None,
        })
    }
}

pub struct ExtensionController {
    pub cursor: Option<String>,
}

impl PaginationController<ExtensionPage> for ExtensionController {
    fn next(self, page: &ExtensionPage) -> Result<Option<Self>, PaginationError> {
        Ok(page.next_cursor().cloned().map(|cursor| Self {
            cursor: Some(cursor),
        }))
    }
}

pub struct ExtensionLimiter;

impl RateLimiter for ExtensionLimiter {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let _bounded_rule_count = request.plan.rules().len();
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe<'a>(&'a self, response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
        let _status = response.status;
        Box::pin(async {})
    }
}

#[derive(Default)]
pub struct ExtensionObserver;

impl RateLimitObserver for ExtensionObserver {
    fn observe(&self, response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        (response.status == StatusCode::TOO_MANY_REQUESTS).then_some(Cooldown {
            duration: Duration::from_secs(1),
            scope: CooldownScope::CurrentBuckets,
        })
    }
}

pub struct ExtensionHooks;

impl RuntimeHooks for ExtensionHooks {
    fn before_send<'a>(
        &'a self,
        _context: BeforeSend<'a>,
    ) -> BoxFuture<'a, Result<(), HookError>> {
        Box::pin(async { Ok(()) })
    }

    fn on_response_head<'a>(&'a self, _context: ResponseHeadEvent<'a>) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    fn on_complete<'a>(&'a self, _context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    fn on_pagination_complete<'a>(
        &'a self,
        _context: PaginationCompletionEvent<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}

concord_macros::api! {
    client ExtensionApi {
        base "http://extension.example.test/v1/";
        credentials {
            credential session = custom crate::ExtensionCredential;
        }
        rate_limits {
            workload {
                by [endpoint];
                10 / 1s;
            }
            observe crate::ExtensionObserver;
        }
        rate_limit workload;

        GET Protected() -> Bytes {
            auth header "X-Session" = session challenge unauthorized;
        }

        GET Pages(cursor: Option<String>) -> crate::ExtensionPageCodec {
            query { "cursor" = ep.cursor; }
            paginate crate::ExtensionController {
                cursor: bind ep.cursor,
            }
        }
    }
}

/// Compiles every stable extension workflow without private or native integration imports.
pub fn certify_public_extensions() -> Result<(), ClientBuildError> {
    let client = ExtensionApi::builder()
        .session_provider(ExtensionProvider)
        .http(|http| {
            http.proxy(
                SafeProxy::parse("http://127.0.0.1:43119")
                    .expect("fixed credential-free proxy origin"),
            )
        })
        .runtime(|runtime| {
            runtime
                .rate_limiter(Arc::new(ExtensionLimiter))
                .hooks(Arc::new(ExtensionHooks))
        })
        .build()?;

    let credential = client.credentials().session().credential();
    let _: Request<_> = client.protected();
    let _: Auth = Auth::header(HeaderName::from_static("x-prepared-session"), credential.clone());

    let buffered = PreparedEndpoint::builder("ExtensionApi::prepared_buffered", Method::POST)
        .path_segment("buffered")
        .body_codec::<ExtensionCodec>("bounded-input".to_owned())
        .auth(Auth::header(
            HeaderName::from_static("x-prepared-session"),
            credential,
        ))
        .response::<ExtensionCodec>()
        .build()
        .expect("valid buffered prepared endpoint");
    let _: Request<_> = client.request(buffered);

    let bytes = PreparedEndpoint::builder("ExtensionApi::prepared_bytes", Method::POST)
        .path_segment("bytes")
        .body(RequestBody::factory(ExtensionBodyFactory))
        .response_bytes()
        .build()
        .expect("valid bytes prepared endpoint");
    let _: Request<_> = client.request(bytes);

    let no_content = PreparedEndpoint::builder("ExtensionApi::prepared_none", Method::DELETE)
        .path_segment("none")
        .build()
        .expect("valid no-content prepared endpoint");
    let _: Request<_> = client.request(no_content);

    let stream = PreparedEndpoint::builder("ExtensionApi::prepared_stream", Method::GET)
        .path_segment("stream")
        .response_stream::<ExtensionMedia>()
        .build()
        .expect("valid streaming prepared endpoint");
    let _: Request<_> = client.request(stream);

    let page = ExtensionPage {
        items: vec![1, 2],
        next: Some("next".to_owned()),
    };
    let controller = ExtensionController { cursor: None };
    let _next_cursor: Option<&String> = page.next_cursor();
    let _next_controller: Option<ExtensionController> = controller
        .next(&page)
        .expect("bounded controller transition");

    #[cfg(feature = "json")]
    {
        let json = PreparedEndpoint::builder("ExtensionApi::prepared_json", Method::POST)
            .body_codec::<Json<Vec<u32>>>(vec![1, 2, 3])
            .response::<Json<Vec<u32>>>()
            .build()
            .expect("valid JSON prepared endpoint");
        let _: Request<_> = client.request(json);
    }

    #[cfg(feature = "multipart")]
    {
        let multipart = PreparedEndpoint::builder("ExtensionApi::prepared_multipart", Method::POST)
            .body(RequestBody::multipart_factory(ExtensionMultipartFactory))
            .build()
            .expect("valid multipart prepared endpoint");
        let _: Request<_> = client.request(multipart);
    }

    Ok(())
}
