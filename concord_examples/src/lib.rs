#![forbid(unsafe_code)]
//! Public extension types shared by the compile-checked Concord examples.

use std::fmt;
use std::time::Duration;

use concord_core::advanced::{
    AuthError, AuthErrorKind, BoxFuture, CodecError, CredentialMaterial, CredentialProvider,
    CredentialRequest, DecodeBody, DecodeContext, DefaultRateLimitObserver, EncodeBody,
    EncodeContext, HasNextCursor, HeaderName, HeaderValue, Method, PaginationController,
    PaginationError, PaginationRequest, ProviderBody, ProviderRequest, RateLimitObserver,
    RateLimitResponseHead, SecretCredential, Url,
};
use concord_core::prelude::{Bytes, SecretString};

/// A complete buffered custom media codec.
pub struct ExampleCodec;

impl EncodeBody for ExampleCodec {
    type Input = String;

    const CONTENT_TYPE: Option<&'static str> = Some("application/vnd.concord.example");

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        Ok(Bytes::from(input))
    }
}

impl DecodeBody for ExampleCodec {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/vnd.concord.example");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type == Self::ACCEPT {
            Ok(())
        } else {
            Err(CodecError::new("example media mismatch"))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new("example UTF-8 decode"))
    }
}

/// Redacted credential material returned by the custom provider example.
#[derive(Clone)]
pub struct ExampleToken(SecretString);

impl CredentialMaterial for ExampleToken {}

impl SecretCredential for ExampleToken {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl fmt::Debug for ExampleToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ExampleToken(<redacted>)")
    }
}

/// Decoder used by the endpoint-backed credential example.
pub struct ExampleTokenCodec;

impl DecodeBody for ExampleTokenCodec {
    type Output = ExampleToken;

    const ACCEPT: Option<&'static str> = Some("application/vnd.concord.token");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let token = String::from_utf8(body.to_vec())
            .map_err(|_| CodecError::new("example token UTF-8 decode"))?;
        Ok(ExampleToken(SecretString::new(token)))
    }
}

/// A custom provider that demonstrates Concord's isolated provider HTTP capability.
pub struct ExampleProvider;

impl CredentialProvider for ExampleProvider {
    type Material = ExampleToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async move {
            let request = ProviderRequest::new(
                Method::POST,
                Url::parse("https://identity.example.test/token")
                    .expect("fixed provider URL is valid"),
            )
            .header(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            )
            .body(ProviderBody::Bytes(Bytes::from_static(
                b"grant_type=client_credentials",
            )))
            .timeout(Duration::from_secs(2));
            let response = context.http.send(request).await?;
            if !response.status().is_success() {
                return Err(AuthError::new(AuthErrorKind::Acquisition));
            }
            let token = String::from_utf8(response.into_body().to_vec())
                .map_err(|_| AuthError::new(AuthErrorKind::Acquisition))?;
            Ok(ExampleToken(SecretString::new(token)))
        })
    }
}

/// One cursor-bearing page for custom and built-in pagination examples.
pub struct ExamplePage {
    items: Vec<u32>,
    next: Option<String>,
}

impl IntoIterator for ExamplePage {
    type Item = u32;
    type IntoIter = std::vec::IntoIter<u32>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl HasNextCursor<String> for ExamplePage {
    fn next_cursor(&self) -> Option<&String> {
        self.next.as_ref()
    }
}

/// Decoder for the compact `item,item|next` example page format.
pub struct ExamplePageCodec;

impl DecodeBody for ExamplePageCodec {
    type Output = ExamplePage;

    const ACCEPT: Option<&'static str> = Some("application/vnd.concord.page");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let text = std::str::from_utf8(&body).map_err(|_| CodecError::new("page UTF-8"))?;
        let (items, next) = text
            .split_once('|')
            .ok_or_else(|| CodecError::new("page shape"))?;
        let items = items
            .split(',')
            .filter(|value| !value.is_empty())
            .map(|value| {
                value
                    .parse::<u32>()
                    .map_err(|_| CodecError::new("page item"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ExamplePage {
            items,
            next: (next != "-").then(|| next.to_owned()),
        })
    }
}

/// Custom cursor controller used by the canonical custom pagination example.
#[derive(Clone)]
pub struct ExampleController {
    cursor: Option<String>,
}

/// Creates one custom pagination controller from the endpoint cursor.
pub fn example_controller(cursor: &Option<String>) -> ExampleController {
    ExampleController {
        cursor: cursor.clone(),
    }
}

impl PaginationController<ExamplePage> for ExampleController {
    fn apply(&self, request: &mut PaginationRequest<'_>) -> Result<(), PaginationError> {
        match &self.cursor {
            Some(cursor) => request.set_query("cursor", cursor),
            None => request.remove_query("cursor"),
        }
    }

    fn next(&self, page: &ExamplePage) -> Result<Option<Self>, PaginationError> {
        Ok(page.next.as_ref().map(|cursor| Self {
            cursor: Some(cursor.clone()),
        }))
    }
}

/// Observer that applies Concord's bounded `Retry-After` behavior.
#[derive(Default)]
pub struct RetryAfterObserver;

impl RateLimitObserver for RetryAfterObserver {
    fn observe(
        &self,
        response: &RateLimitResponseHead<'_>,
    ) -> Option<concord_core::advanced::Cooldown> {
        DefaultRateLimitObserver.observe(response)
    }
}
