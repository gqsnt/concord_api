//! The configured SDK: [`ShopifyContext`] (HTTP client, API version, base URL) and
//! [`ShopContext`] (one shop and its token), which hand out the generated operations and own the
//! two things a declaration does not carry:
//!
//! - [`ShopContext::send`] executes a prepared operation once (`Prepared::receive`: any status,
//!   body unread) and checks `X-Shopify-API-Version` on the response head before the body is
//!   read: Shopify silently serves a retired version with a newer one ("falls forward"), and a
//!   pinned SDK must not decode another version's answer as its own; an accepted head then goes
//!   to the endpoint's own handling (`Received::decode`);
//! - [`ShopContext::result_download`] builds the native `GET` of a signed bulk-result URL, on
//!   another host and **without** the access token
//!   ([`bulk_export::open_result`](crate::bulk_export::open_result) opens its response).
//!
//! Like Concord's handles, contexts are immutable snapshots, cheap to clone.

use std::fmt;

use concord::reqwest::{self, Method, header};
use concord::{Prepared, ResponseHead};

use crate::api::{ShopifyAdmin, ShopifyAdminShop};
use crate::graphql::GraphqlResponse;
use crate::scalars::{AccessToken, ApiVersion, ShopDomain, SignedUrl};

/// The response header naming the version that served the request.
pub const X_SHOPIFY_API_VERSION: &str = "x-shopify-api-version";

/// The SDK configuration shared by every shop.
#[derive(Clone)]
pub struct ShopifyContext {
    admin: ShopifyAdmin,
}

impl ShopifyContext {
    /// The SDK over `http`, pinned to [`ApiVersion::PINNED`], against
    /// `https://{shop}.myshopify.com`.
    ///
    /// Configure `http` with `redirect(reqwest::redirect::Policy::none())`: the documented API
    /// does not redirect, and reqwest's default policy would forward the custom
    /// `X-Shopify-Access-Token` header to a redirect target (it strips only `Authorization`,
    /// `Cookie` and similar standard credentials).
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            admin: ShopifyAdmin::new(http),
        }
    }

    /// Replaces the base URL (a loopback test server mapped with
    /// `reqwest::ClientBuilder::resolve`, a gateway). It must keep a domain host: the shop is its
    /// first label (`http://myshopify.com:8080` reaches `{shop}.myshopify.com:8080`).
    ///
    /// # Errors
    ///
    /// Concord's error for a URL `with_base_url` refuses.
    pub fn with_base_url(mut self, url: impl AsRef<str>) -> Result<Self, concord::Error> {
        self.admin = self.admin.with_base_url(url)?;
        Ok(self)
    }

    /// Uses another API version (in the path, and as the expected `X-Shopify-API-Version`).
    #[must_use]
    pub fn with_version(mut self, version: ApiVersion) -> Self {
        self.admin = self.admin.set_version(version);
        self
    }

    /// The API version requests use.
    pub fn version(&self) -> ApiVersion {
        *self.admin.version()
    }

    /// The generated client handle.
    pub fn admin(&self) -> &ShopifyAdmin {
        &self.admin
    }

    /// The HTTP client.
    pub fn http(&self) -> &reqwest::Client {
        self.admin.http()
    }

    /// The context of one shop, with its token.
    /// Sends `x-shopify-access-token` with `AccessToken::expose()` verbatim
    /// on this shop's API requests; signed result downloads use `result_download` instead.
    pub fn shop(&self, shop: ShopDomain, token: &AccessToken) -> ShopContext {
        ShopContext {
            admin: self.admin.shop(shop, token.expose()),
        }
    }
}

impl fmt::Debug for ShopifyContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShopifyContext")
            .field("admin", &self.admin)
            .field("version", self.admin.version())
            .finish()
    }
}

/// One shop: the generated operations under `{shop}.myshopify.com` with its token, and the
/// helpers that need the same configuration.
#[derive(Clone)]
pub struct ShopContext {
    admin: ShopifyAdminShop,
}

impl ShopContext {
    /// The generated shop handle: `admin().products(ProductsQuery::new(..))` returns the
    /// generated request, with `prepare()`, `send()` and `set_variables(..)`.
    pub fn admin(&self) -> &ShopifyAdminShop {
        &self.admin
    }

    /// The shop.
    pub fn shop(&self) -> &ShopDomain {
        self.admin.shop()
    }

    /// The API version requests use.
    pub fn version(&self) -> ApiVersion {
        *self.admin.client().version()
    }

    /// The HTTP client.
    pub fn http(&self) -> &reqwest::Client {
        self.admin.client().http()
    }

    /// Executes a prepared operation once and decodes its envelope after checking the served
    /// version. Pass the generated request's `prepare()` result:
    /// `shop.send(shop.admin().products(query).prepare()).await`.
    ///
    /// No retry, no throttling wait: a `THROTTLED` answer is an `Ok` envelope whose errors say so.
    ///
    /// # Errors
    ///
    /// [`RequestError::Concord`] for a build, transport, status, metadata or decode failure
    /// (Concord's own error, with its unread response for a status error);
    /// [`RequestError::VersionMismatch`] / [`RequestError::VersionUnconfirmed`] for a 2xx answer
    /// served under another version or without the header (refused on the head: the body is not
    /// read, and the response is dropped).
    pub async fn send<D>(
        &self,
        prepared: Result<Prepared<GraphqlResponse<D>>, concord::Error>,
    ) -> Result<GraphqlResponse<D>, RequestError> {
        // One execution; any status comes back with its body unread (a transport failure is
        // Concord's error, URL removed).
        let received = prepared
            .map_err(RequestError::Concord)?
            .receive()
            .await
            .map_err(RequestError::Concord)?;
        if received.status().is_success() {
            // A refusal drops the response unread: its connection is closed, not drained.
            self.check_served_version(&received.head())?;
        }
        received.decode().await.map_err(RequestError::Concord)
    }

    /// Shopify's served-version rule, applied to the head of a 2xx answer before its body is read.
    fn check_served_version(&self, head: &ResponseHead<'_>) -> Result<(), RequestError> {
        let served = head
            .headers()
            .get(X_SHOPIFY_API_VERSION)
            .map(|v| v.to_str().ok().and_then(|v| v.parse::<ApiVersion>().ok()));
        match served {
            None => Err(RequestError::VersionUnconfirmed),
            Some(Some(served)) if served == self.version() => Ok(()),
            Some(served) => Err(RequestError::VersionMismatch {
                requested: self.version(),
                served,
            }),
        }
    }

    /// Sets `accept: application/jsonl, */*;q=0.1` on the bulk download request.
    ///
    /// The native `GET` of a signed bulk-result URL (`url` or `partialDataUrl`), to execute with
    /// [`http`](Self::http) and open with [`open_result`](crate::bulk_export::open_result) and a
    /// [`result_reader`](crate::bulk_export::result_reader).
    ///
    /// The URL is used verbatim: its query is the signature, so it is neither declared nor
    /// rebuilt. The request carries no Shopify credential (the signature is the authorization;
    /// the token must not reach the storage host) and asks for no compression beyond reqwest's
    /// defaults. It is native: its `Debug` and URL expose the signature — do not log it.
    pub fn result_download(&self, url: &SignedUrl) -> reqwest::Request {
        let mut request = reqwest::Request::new(Method::GET, url.expose().clone());
        request.headers_mut().insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/jsonl, */*;q=0.1"),
        );
        request
    }
}

impl fmt::Debug for ShopContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShopContext")
            .field("admin", &self.admin)
            .field("version", self.admin.client().version())
            .finish()
    }
}

/// A GraphQL request that produced no envelope.
///
/// `Display`/`Debug` follow Concord's policy (endpoint, stage, status code; never the token, a
/// variable or the body).
#[derive(concord::Debug)]
pub enum RequestError {
    /// Concord's error: build, transport, status (unread response kept), metadata or decode.
    Concord(concord::Error),
    /// A 2xx answer served under another version than the one requested (`served: None`: an
    /// unparseable header). The body was not read.
    VersionMismatch {
        /// The pinned version.
        requested: ApiVersion,
        /// The version Shopify reported.
        served: Option<ApiVersion>,
    },
    /// A 2xx answer without `X-Shopify-API-Version`. The body was not read.
    VersionUnconfirmed,
}

impl RequestError {
    /// Concord's error, if that is what this is.
    pub fn concord(&self) -> Option<&concord::Error> {
        match self {
            Self::Concord(e) => Some(e),
            _ => None,
        }
    }
}

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord(e) => fmt::Display::fmt(e, f),
            Self::VersionMismatch {
                requested,
                served: Some(served),
            } => write!(f, "requested API version {requested}, served {served}"),
            Self::VersionMismatch {
                requested,
                served: None,
            } => {
                write!(
                    f,
                    "requested API version {requested}, served an unparseable version"
                )
            }
            Self::VersionUnconfirmed => f.write_str("the response does not state its API version"),
        }
    }
}

impl std::error::Error for RequestError {}
