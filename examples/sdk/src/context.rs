//! The public boundary for Catalog operations this SDK does not declare.
//!
//! An application configures **one** [`CatalogContext`] — its `reqwest::Client`, the base URL,
//! the API key and the optional region — and obtains everything from it: the generated
//! [`Catalog`] and [`CatalogTenant`] handles for the declared endpoints, and native requests for
//! operations another crate adds ([`TenantContext::request`], [`CatalogContext::request`]). Both
//! come from the same values, so they cannot drift apart. Like Concord's handles, a context is an
//! immutable snapshot: a setter consumes it and returns a new one, and cloning is a
//! reference-count increment.
//!
//! ```
//! use concord_example_sdk::context::CatalogContext;
//! use concord_example_sdk::CatalogConfig;
//! use concord::reqwest::{self, Method};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let catalog = CatalogContext::new(reqwest::Client::new(), CatalogConfig { key: "k-1".into() })
//!     .with_base_url("https://edge.example.test/edge/catalog/v1")?
//!     .set_region("north");
//! let acme = catalog.tenant("acme/east");
//!
//! // A declared endpoint, through the generated handle:
//! let item = acme.catalog_tenant().item("item-001").prepare()?;
//! // An operation the SDK does not declare, from the same values:
//! let export = acme.request(Method::GET, &["exports", "items-2026"])?;
//!
//! assert_eq!(
//!     item.request().url().as_str(),
//!     "https://edge.example.test/edge/catalog/v1/tenants/acme%2Feast/items/item-001",
//! );
//! assert_eq!(
//!     export.url().as_str(),
//!     "https://edge.example.test/edge/catalog/v1/tenants/acme%2Feast/exports/items-2026",
//! );
//! assert_eq!(export.headers()["x-region"], "north");
//! assert!(export.headers()["x-api-key"].is_sensitive());
//! # Ok(()) }
//! ```
//!
//! # What a built request guarantees
//!
//! [`TenantContext::request`] and [`CatalogContext::request`] build a native
//! `reqwest::Request` with no I/O, following the rules Concord applies to the declared
//! endpoints:
//!
//! - **Base.** The base URL is validated by Concord itself ([`Catalog::with_base_url`]: `http`
//!   or `https`, a host, no port 0, user information, query or fragment) and its path is kept as
//!   a prefix: `https://h/edge/catalog/v1` plus `tenants/acme` is
//!   `https://h/edge/catalog/v1/tenants/acme`. One trailing slash of the base is ignored. Segments
//!   are appended to the prefix, never resolved against it (no relative-URL join).
//! - **Path data.** `tenants/{tenant}` and every segment you pass are each **one** path segment,
//!   encoded like a Concord path value: every byte outside the RFC 3986 unreserved set
//!   (`A-Z a-z 0-9 - . _ ~`) is percent-encoded (`/` is `%2F`, `%` is `%25`, space is `%20`,
//!   `ü` is `%C3%BC`). Segments are data, not pre-escaped text. An empty, `.` or `..` segment is a
//!   [`ContextError`] before any request exists, as it is a build error for a declared endpoint.
//! - **Headers.** `x-client: concord-example-sdk`, the key as `x-api-key` with the native
//!   sensitivity flag set (`HeaderValue::set_sensitive`), and `x-region` when a region is set. A
//!   key or region that is not a valid header value is a [`ContextError`].
//! - **Client.** [`TenantContext::http`] is the configured `reqwest::Client`. Its own defaults
//!   (default headers, timeouts, proxies) are applied by reqwest when it executes the request, so
//!   they are not visible on the built request beforehand.
//!
//! # What it does not do
//!
//! A context is not a generated endpoint, and claims no endpoint's identity:
//!
//! - The request has no `Accept`, body, query or timeout: the operation adds what it needs with
//!   reqwest's API.
//! - There is no response handling: the operation chooses its own, typically
//!   `concord::Decoder::<T>::new::<F>()` completed with `decoder.finish(http.execute(request).await)`,
//!   which applies the status and format rules and Concord's transport-error conversion (the URL
//!   removed). Such a decoder is **unlabeled**: its errors' `endpoint()` is `None`. Name the
//!   operation in your own error type rather than borrowing a declared endpoint's name.
//! - There is no Concord redaction: no endpoint metadata exists for these requests. The native
//!   request's `Debug` (and its `url()`) prints the full URL, tenant and segments included; only
//!   the key is masked (reqwest prints a flagged header as `Sensitive`). **Do not log a built
//!   request or its URL** when path data may be secret. [`ContextError`] and the `Debug` of the
//!   contexts never contain the key or path data; the contexts' `Debug` shows the base URL, the
//!   region and the tenant, as the generated handles' `Debug` does.
//!
//! # Keeping the two routes together
//!
//! Create generated handles from the context ([`CatalogContext::catalog`],
//! [`TenantContext::catalog_tenant`]). A handle changed afterwards (its own `set_region`, say) is
//! a separate snapshot that the context does not follow.

use std::fmt;
use std::sync::Arc;

use concord::reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use concord::reqwest::{self, Method, Url};

use crate::{Catalog, CatalogConfig, CatalogTenant};

/// The base URL declared in the `Catalog` declaration (`base "..."`), used until
/// [`CatalogContext::with_base_url`] replaces it.
const DECLARED_BASE: &str = "https://catalog.example.test/catalog/v1";

/// The declaration's service header, `headers { "x-client" = "concord-example-sdk"; }`.
const X_CLIENT: HeaderName = HeaderName::from_static("x-client");
const CLIENT_NAME: HeaderValue = HeaderValue::from_static("concord-example-sdk");
/// The declaration's `#[sensitive] header("x-api-key") key`.
const X_API_KEY: HeaderName = HeaderName::from_static("x-api-key");
/// The declaration's `header("x-region") region?`.
const X_REGION: HeaderName = HeaderName::from_static("x-region");

/// The Catalog configuration shared by the generated handles and the requests of operations
/// this SDK does not declare. See the [module documentation](self).
#[derive(Clone)]
pub struct CatalogContext {
    inner: Arc<Inner>,
}

#[derive(Clone)]
struct Inner {
    http: reqwest::Client,
    /// Accepted by Concord's `Catalog::with_base_url` (or the declared base).
    base: Url,
    key: String,
    region: Option<String>,
    /// The generated handle, configured with exactly the values above.
    catalog: Catalog,
}

impl CatalogContext {
    /// The context of `http` and the client's required values, against the declared base
    /// (`https://catalog.example.test/catalog/v1`) and without a region — the same arguments as
    /// [`Catalog::new`]. Builds no connection and validates no header value yet (a request
    /// does).
    pub fn new(http: reqwest::Client, config: CatalogConfig) -> Self {
        // Exhaustive on purpose: a required client value added to the declaration must be added
        // here too, or this stops compiling.
        let CatalogConfig { key } = &config;
        let key = key.clone();
        let base = Url::parse(DECLARED_BASE).expect("the declared Catalog base is a valid URL");
        let catalog = Catalog::new(http.clone(), config);
        Self {
            inner: Arc::new(Inner {
                http,
                base,
                key,
                region: None,
                catalog,
            }),
        }
    }

    /// Replaces the base URL, for example with a staging server, a gateway path
    /// (`https://edge.example.test/edge/catalog/v1`) or a local test server. The URL is validated
    /// by Concord's own [`Catalog::with_base_url`] and its path is kept as a prefix.
    ///
    /// # Errors
    ///
    /// The Concord error of [`Catalog::with_base_url`] (kind `Build`, no endpoint) for anything
    /// that is not an `http`/`https` URL with a host, without port 0, user information, query or
    /// fragment.
    pub fn with_base_url(mut self, url: impl AsRef<str>) -> Result<Self, concord::Error> {
        let url = url.as_ref();
        let inner = Arc::make_mut(&mut self.inner);
        inner.catalog = inner.catalog.clone().with_base_url(url)?;
        // Concord parsed it with the same `url` crate and accepted it.
        inner.base = Url::parse(url).expect("a base URL Concord accepted parses");
        Ok(self)
    }

    /// Sets the region, sent as `x-region` by the generated endpoints and by built requests.
    pub fn set_region(self, region: impl Into<String>) -> Self {
        self.maybe_region(Some(region.into()))
    }

    /// Sets whether a region is sent: `Some(r)` is [`set_region(r)`](Self::set_region); `None`
    /// sends no `x-region` (the service's default region).
    pub fn maybe_region(mut self, region: Option<String>) -> Self {
        let inner = Arc::make_mut(&mut self.inner);
        inner.catalog = inner.catalog.clone().maybe_region(region.clone());
        inner.region = region;
        self
    }

    /// The generated client handle, configured with this context's values.
    pub fn catalog(&self) -> &Catalog {
        &self.inner.catalog
    }

    /// The context of one tenant, and the generated [`CatalogTenant`] handle for it. The tenant
    /// is path data, checked when a request is built.
    pub fn tenant(&self, tenant: impl Into<String>) -> TenantContext {
        let tenant = tenant.into();
        TenantContext {
            handle: self.inner.catalog.tenant(tenant.as_str()),
            context: self.clone(),
            tenant,
        }
    }

    /// The `reqwest::Client` to execute built requests with.
    pub fn http(&self) -> &reqwest::Client {
        &self.inner.http
    }

    /// The base URL requests are built against.
    pub fn base_url(&self) -> &Url {
        &self.inner.base
    }

    /// The region, if one is set.
    pub fn region(&self) -> Option<&str> {
        self.inner.region.as_deref()
    }

    /// A native request for `method` and the base-relative path `segments` (for example
    /// `["accounts", "42", "summary"]`), with the service headers. With no segments the URL is
    /// the base URL as given. See the [module documentation](self) for the guarantees.
    ///
    /// # Errors
    ///
    /// [`ContextError::Segment`] if a segment is empty, `.` or `..`;
    /// [`ContextError::HeaderValue`] if the key or region is not a valid header value. Nothing
    /// is sent either way.
    pub fn request(
        &self,
        method: Method,
        segments: &[&str],
    ) -> Result<reqwest::Request, ContextError> {
        self.build(method, None, segments)
    }

    fn build(
        &self,
        method: Method,
        tenant: Option<&str>,
        segments: &[&str],
    ) -> Result<reqwest::Request, ContextError> {
        let inner = &*self.inner;
        let mut url = inner.base.clone();
        if tenant.is_some() || !segments.is_empty() {
            let base_path = inner.base.path();
            let mut path = base_path.strip_suffix('/').unwrap_or(base_path).to_owned();
            if let Some(tenant) = tenant {
                if is_dot_or_empty(tenant) {
                    return Err(ContextError::Tenant);
                }
                push_segment(&mut path, "tenants");
                push_segment(&mut path, tenant);
            }
            for (index, segment) in segments.iter().enumerate() {
                if is_dot_or_empty(segment) {
                    return Err(ContextError::Segment { index });
                }
                push_segment(&mut path, segment);
            }
            url.set_path(&path);
        }
        let mut headers = HeaderMap::new();
        headers.insert(X_CLIENT, CLIENT_NAME);
        let mut key = HeaderValue::from_str(&inner.key)
            .map_err(|_| ContextError::HeaderValue { name: "x-api-key" })?;
        key.set_sensitive(true);
        headers.insert(X_API_KEY, key);
        if let Some(region) = &inner.region {
            let region = HeaderValue::from_str(region)
                .map_err(|_| ContextError::HeaderValue { name: "x-region" })?;
            headers.insert(X_REGION, region);
        }
        let mut request = reqwest::Request::new(method, url);
        *request.headers_mut() = headers;
        Ok(request)
    }
}

impl fmt::Debug for CatalogContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CatalogContext")
            .field("base", &self.inner.base.as_str())
            .field("key", &format_args!("<redacted>"))
            .field("region", &self.inner.region)
            .finish()
    }
}

/// One tenant of a [`CatalogContext`]: native requests under `tenants/{tenant}`, and the
/// generated [`CatalogTenant`] handle for the same tenant. Created by
/// [`CatalogContext::tenant`]; an immutable snapshot of that context.
#[derive(Clone)]
pub struct TenantContext {
    context: CatalogContext,
    tenant: String,
    handle: CatalogTenant,
}

impl TenantContext {
    /// The tenant, as given (decoded text, not its wire form).
    pub fn tenant(&self) -> &str {
        &self.tenant
    }

    /// The Catalog context this tenant was created from.
    pub fn context(&self) -> &CatalogContext {
        &self.context
    }

    /// The generated handle of this tenant, for the declared endpoints.
    pub fn catalog_tenant(&self) -> &CatalogTenant {
        &self.handle
    }

    /// The `reqwest::Client` to execute built requests with.
    pub fn http(&self) -> &reqwest::Client {
        self.context.http()
    }

    /// A native request for `method` and the path `tenants/{tenant}/` followed by `segments`
    /// (for example `["exports", "items-2026"]`), each one encoded as one segment, with the
    /// service headers. See the [module documentation](self) for the guarantees.
    ///
    /// # Errors
    ///
    /// [`ContextError::Tenant`] if the tenant, or [`ContextError::Segment`] if a segment, is
    /// empty, `.` or `..`; [`ContextError::HeaderValue`] if the key or region is not a valid
    /// header value. Nothing is sent either way.
    pub fn request(
        &self,
        method: Method,
        segments: &[&str],
    ) -> Result<reqwest::Request, ContextError> {
        self.context.build(method, Some(&self.tenant), segments)
    }
}

impl fmt::Debug for TenantContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TenantContext")
            .field("tenant", &self.tenant)
            .field("context", &self.context)
            .finish()
    }
}

/// A request a context could not build. Nothing was sent. `Display` and `Debug` name the
/// failing part only: never the key, the tenant or a segment's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContextError {
    /// The tenant is empty, `.` or `..`.
    Tenant,
    /// The path segment at `index` (0-based, of the slice passed to `request`) is empty, `.` or
    /// `..`.
    Segment {
        /// Its position in the slice.
        index: usize,
    },
    /// A configured header value is not a valid HTTP header value.
    HeaderValue {
        /// The header: `x-api-key` or `x-region`.
        name: &'static str,
    },
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Catalog context: request not built: ")?;
        match self {
            Self::Tenant => f.write_str("tenant is empty, `.` or `..`"),
            Self::Segment { index } => write!(f, "path segment {index} is empty, `.` or `..`"),
            Self::HeaderValue { name } => write!(f, "{name} value is not a valid header value"),
        }
    }
}

impl std::error::Error for ContextError {}

/// Concord refuses these path segments (they would be removed or resolved by URL handling).
fn is_dot_or_empty(segment: &str) -> bool {
    segment.is_empty() || segment == "." || segment == ".."
}

/// Appends `/` and `value` as one path segment, encoded as Concord encodes a path value: every
/// byte outside RFC 3986 unreserved (`A-Z a-z 0-9 - . _ ~`) as `%XX`, uppercase hex.
fn push_segment(path: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    path.push('/');
    for &byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            path.push(char::from(byte));
        } else {
            path.push('%');
            path.push(char::from(HEX[usize::from(byte >> 4)]));
            path.push(char::from(HEX[usize::from(byte & 0x0F)]));
        }
    }
}
