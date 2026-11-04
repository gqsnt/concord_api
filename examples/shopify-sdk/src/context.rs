//! The configured SDK: [`ShopifyContext`] (HTTP client, API version, base URL) and
//! [`ShopContext`] (one shop and its token), which hand out the generated operations and own the
//! two things a declaration does not carry:
//!
//! - [`ShopContext::send`] executes a prepared operation and checks `X-Shopify-API-Version`
//!   before decoding: Shopify silently serves a retired version with a newer one ("falls
//!   forward"), and a pinned SDK must not decode another version's answer as its own;
//! - [`ShopContext::result_download`] builds the native `GET` of a signed bulk-result URL, on
//!   another host and **without** the access token.
//!
//! Like Concord's handles, contexts are immutable snapshots, cheap to clone.

use std::fmt;

use concord::Prepared;
use concord::reqwest::{self, Method, header};

use crate::api::{ShopifyAdmin, ShopifyAdminShop};
use crate::graphql::GraphqlResponse;
use crate::scalars::{AccessToken, ApiVersion, ShopDomain, SignedUrl};

/// The response header naming the version that served the request.
pub const X_SHOPIFY_API_VERSION: &str = "x-shopify-api-version";

/// The SDK configuration shared by every shop.
#[derive(Clone)]
pub struct ShopifyContext {
    http: reqwest::Client,
    admin: ShopifyAdmin,
    version: ApiVersion,
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
            admin: ShopifyAdmin::new(http.clone()),
            http,
            version: ApiVersion::PINNED,
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
        self.version = version;
        self
    }

    /// The API version requests use.
    pub fn version(&self) -> ApiVersion {
        self.version
    }

    /// The generated client handle.
    pub fn admin(&self) -> &ShopifyAdmin {
        &self.admin
    }

    /// The HTTP client.
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// The context of one shop, with its token.
    pub fn shop(&self, shop: ShopDomain, token: &AccessToken) -> ShopContext {
        ShopContext {
            admin: self.admin.shop(shop.clone(), token.expose()),
            http: self.http.clone(),
            version: self.version,
            shop,
        }
    }
}

impl fmt::Debug for ShopifyContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShopifyContext")
            .field("admin", &self.admin)
            .field("version", &self.version)
            .finish()
    }
}

/// One shop: the generated operations under `{shop}.myshopify.com` with its token, and the
/// helpers that need the same configuration.
#[derive(Clone)]
pub struct ShopContext {
    admin: ShopifyAdminShop,
    http: reqwest::Client,
    version: ApiVersion,
    shop: ShopDomain,
}

impl ShopContext {
    /// The generated shop handle: `admin().products(ProductsQuery::new(..))` returns the
    /// generated request, with `prepare()`, `send()` and `set_variables(..)`.
    pub fn admin(&self) -> &ShopifyAdminShop {
        &self.admin
    }

    /// The shop.
    pub fn shop(&self) -> &ShopDomain {
        &self.shop
    }

    /// The API version requests use.
    pub fn version(&self) -> ApiVersion {
        self.version
    }

    /// The HTTP client.
    pub fn http(&self) -> &reqwest::Client {
        &self.http
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
    /// served under another version or without the header (the body is not read).
    pub async fn send<D>(
        &self,
        prepared: Result<Prepared<GraphqlResponse<D>>, concord::Error>,
    ) -> Result<GraphqlResponse<D>, RequestError> {
        let (client, request, decoder) = prepared.map_err(RequestError::Concord)?.into_parts();
        let response = match client.execute(request).await {
            Ok(response) => response,
            // Concord's transport error (URL removed), through the endpoint's own decoder.
            Err(e) => return decoder.finish(Err(e)).await.map_err(RequestError::Concord),
        };
        if response.status().is_success() {
            let served = response
                .headers()
                .get(X_SHOPIFY_API_VERSION)
                .map(|v| v.to_str().ok().and_then(|v| v.parse::<ApiVersion>().ok()));
            match served {
                None => return Err(RequestError::VersionUnconfirmed),
                Some(None) => {
                    return Err(RequestError::VersionMismatch {
                        requested: self.version,
                        served: None,
                    });
                }
                Some(Some(served)) if served != self.version => {
                    return Err(RequestError::VersionMismatch {
                        requested: self.version,
                        served: Some(served),
                    });
                }
                Some(Some(_)) => {}
            }
        }
        decoder
            .decode(response)
            .await
            .map_err(RequestError::Concord)
    }

    /// The native `GET` of a signed bulk-result URL (`url` or `partialDataUrl`), to execute with
    /// [`http`](Self::http) and open with [`result_reader`](crate::bulk_export::result_reader).
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
            .field("version", &self.version)
            .finish()
    }
}

/// A GraphQL request that produced no envelope.
///
/// `Display`/`Debug` follow Concord's policy (endpoint, stage, status code; never the token, a
/// variable or the body).
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

impl fmt::Debug for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord(e) => f.debug_tuple("Concord").field(e).finish(),
            Self::VersionMismatch { requested, served } => f
                .debug_struct("VersionMismatch")
                .field("requested", requested)
                .field("served", served)
                .finish(),
            Self::VersionUnconfirmed => f.write_str("VersionUnconfirmed"),
        }
    }
}

impl std::error::Error for RequestError {}
