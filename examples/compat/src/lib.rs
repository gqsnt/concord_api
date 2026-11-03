//! Compact downstream compatibility declarations, exercised only on loopback.
use concord::{reqwest, CodecError, Decode, Encode};
use concord::{Form, Json};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// A typed success or status-error document.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Document {
    /// Fixture value.
    pub value: String,
}

concord::api! {
    client Metadata {
        base "http://compat.invalid";
        response Json;
        GET get() -> Document { path "document"; }
    }
}

/// Uses a private client inside this crate, returning the public native exit.
pub fn metadata(
    http: reqwest::Client,
    base: &str,
) -> Result<concord::Prepared<Document>, concord::Error> {
    Metadata::new(http).with_base_url(base)?.get().prepare()
}

/// A synthetic service's application envelope.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Envelope {
    /// Application success, independent of HTTP status.
    pub ok: bool,
    /// Application error detail.
    pub error: Option<String>,
}

/// Checks the synthetic envelope after JSON decoding.
pub struct Checked;
impl Decode<Envelope> for Checked {
    type Context = ();
    fn context(_: &concord::ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: concord::bytes::Bytes) -> Result<Envelope, CodecError> {
        let envelope = <concord::Json as Decode<Envelope>>::decode_body((), body)?;
        if !envelope.ok {
            return Err(CodecError::new("application rejected request")
                .with_source(std::io::Error::other(envelope.error.unwrap_or_default())));
        }
        Ok(envelope)
    }
}
concord::api! {
    /// Explicit application-error handling routes.
    pub client Application {
        base "http://compat.invalid";
        response Json;
        GET explicit() -> Envelope { path "explicit"; }
        GET checked() -> Envelope { path "checked"; response Checked; }
    }
}

/// A flat form.
#[derive(Clone, Serialize)]
pub struct Flat {
    /// A text value containing spaces and punctuation.
    pub title: String,
    /// Order number.
    pub order: u32,
}
/// A nested form, unsupported by the built-in form encoder.
#[derive(Clone, Serialize)]
pub struct Nested {
    /// Nested order metadata.
    pub metadata: Order,
}
/// Order metadata.
#[derive(Clone, Serialize)]
pub struct Order {
    /// Order number.
    pub order: u32,
}
/// A synthetic bracket-key convention, explicitly implemented by the caller.
pub struct Brackets;
impl Encode<Nested> for Brackets {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-www-form-urlencoded");
    fn encode(value: Nested) -> Result<reqwest::Body, CodecError> {
        let bytes = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("metadata[order]", &value.metadata.order.to_string())
            .finish();
        Ok(bytes.into())
    }
}
concord::api! {
    /// Flat and custom nested form bodies.
    pub client Forms {
        base "http://compat.invalid";
        request Form;
        POST flat(body form: Flat) { path "flat"; }
        POST nested(body form: Nested) { path "nested"; }
        POST brackets(body form: Nested) { path "brackets"; request Brackets; }
    }
}
concord::api! {
    /// A resource whose ordinary key is a single segment.
    pub client Resources {
        base "http://compat.invalid";
        PUT write(key: String) { path "objects/{key}"; }
    }
}

/// Writes a non-empty sequence of resource-key segments using the native request exit.
///
/// Empty items, `.`/`..` and control characters are rejected before URL mutation, to avoid
/// URL normalization or discarded input.
/// The native URL segment API encodes each item as data. After `into_parts`, native request
/// debugging exposes the URL, including any secret path or query values.
pub async fn write_resource_segments(
    api: &Resources,
    segments: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    if segments.is_empty()
        || segments
            .iter()
            .any(|s| s.is_empty() || *s == "." || *s == ".." || s.chars().any(char::is_control))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "resource key needs non-empty segments other than . and .., without controls",
        )
        .into());
    }
    let (client, mut request, decoder) = api.write("placeholder").prepare()?.into_parts();
    request
        .url_mut()
        .path_segments_mut()
        .map_err(|_| std::io::Error::other("base URL cannot hold path segments"))?
        .pop()
        .extend(segments.iter().copied());
    decoder.finish(client.execute(request).await).await?;
    Ok(())
}

concord::api! {
    /// Kubernetes-style server-side apply serialization.
    pub client Apply {
        base "http://compat.invalid";
        PATCH patch(query("fieldManager") manager: String, body document: serde_json::Value) {
            path "api/v1/namespaces/default/configmaps/demo";
            request Json "application/apply-patch+yaml";
        }
    }
}

static NEXT: AtomicU64 = AtomicU64::new(1);
fn new_key() -> String {
    format!("fixture-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}
concord::api! {
    /// Endpoint initializers run per request handle.
    pub client Fresh {
        base "http://compat.invalid";
        POST charge(header("idempotency-key") key: String = new_key()) { path "charge"; }
    }
}
concord::api! {
    /// The pitfall: a root initializer runs once per client construction.
    pub client Shared(header("idempotency-key") key: String = new_key()) {
        base "http://compat.invalid";
        POST charge() { path "charge"; }
    }
}
concord::api! {
    /// Root refresh creates a new snapshot.
    pub client Refresh(#[sensitive] header("authorization") token: String) {
        base "http://compat.invalid";
        scope account {
            path "account";
            GET get() { path "value"; }
        }
    }
}

use reqwest::header::{HeaderName, CONTENT_TYPE};
/// A native alias of Content-Type.
pub const CT: HeaderName = CONTENT_TYPE;
/// A re-export retains the native header identity.
pub use reqwest::header::AUTHORIZATION as GATE;
/// A misleading identifier still resolves to its actual native name.
pub mod local {
    use concord::reqwest::header::HeaderName;
    /// This is x-other, regardless of the Rust identifier.
    pub const AUTHORIZATION: HeaderName = HeaderName::from_static("x-other");
}
concord::api! {
    /// Native and literal header composition and redaction.
    pub client Headers(#[sensitive] header(GATE) token: String) {
        base "http://compat.invalid";
        request Json;
        headers { "content-type" = "text/plain"; local::AUTHORIZATION = "ordinary"; }
        #[cfg(any())]
        POST disabled(body value: DoesNotExist) { request MissingCodec; }
        POST removed(#[sensitive] secret: String, body value: Flat) {
            path "removed";
            headers { remove CT; override GATE = secret; }
        }
        POST replaced(#[sensitive] secret: String, body value: Flat) {
            path "replaced";
            headers { override CT = "application/custom"; override "authorization" = secret; }
        }
    }
}

concord::api! {
    /// A literal `content-type` inherited by every endpoint: it is the request's `Content-Type`,
    /// with or without a body, replacing the format's own media type (author guide, §5.3).
    pub client Relabel {
        base "http://compat.invalid";
        request Json;
        headers { "content-type" = "application/vnd.compat+json"; }
        POST maybe(body value?: Flat) { path "maybe"; }
    }
}

/// A response format for an endpoint that answers either JSON or nothing (`200` with a document,
/// `204 No Content` without). JSON itself has no empty document and `Json` requires a JSON
/// `Content-Type`, so this policy is the application's own: a `204` needs no `Content-Type` and
/// decodes to `None`; any other 2xx must pass `Json`'s head check and decode as JSON (`null` is
/// `None` too). The head decides, before the body is read; `Json` itself stays strict.
pub struct JsonOrEmpty;

impl<T: serde::de::DeserializeOwned> Decode<Option<T>> for JsonOrEmpty {
    /// Whether the response was a `204 No Content`.
    type Context = bool;
    const ACCEPT: Option<&'static str> = <concord::Json as Decode<Option<T>>>::ACCEPT;

    fn check_content_type(head: &concord::ResponseHead<'_>) -> Result<(), CodecError> {
        if head.status() == concord::reqwest::StatusCode::NO_CONTENT {
            return Ok(());
        }
        <concord::Json as Decode<Option<T>>>::check_content_type(head)
    }

    fn context(head: &concord::ResponseHead<'_>) -> Result<bool, CodecError> {
        Ok(head.status() == concord::reqwest::StatusCode::NO_CONTENT)
    }

    fn decode_body(no_content: bool, body: concord::bytes::Bytes) -> Result<Option<T>, CodecError> {
        if no_content {
            return Ok(None);
        }
        <concord::Json as Decode<Option<T>>>::decode_body((), body)
    }
}

/// What is playing, if anything.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Playback {
    /// The track.
    pub track: String,
}

concord::api! {
    /// A synthetic player API: `GET /playing` answers `200` with JSON, or `204` when idle.
    pub client Player {
        base "http://compat.invalid";
        GET playing() -> Option<Playback> {
            path "playing";
            response JsonOrEmpty;
        }
    }
}
