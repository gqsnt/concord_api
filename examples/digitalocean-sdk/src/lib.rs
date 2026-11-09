#![doc = include_str!("../README.md")]

mod error;
mod models;
mod pagination;

pub use concord;
pub use concord_pagination;
pub use error::{Error, HttpClass, Problem, ProviderError};
pub use models::*;
pub use pagination::{NextLink, Traversal};

use concord::reqwest::{self, header::AUTHORIZATION};
use concord::{Endpoint, Json, Received, SendWith};
use std::{fmt, time::Duration};

/// Maximum collected success body, in bytes (1 MiB).
pub const SUCCESS_BODY_LIMIT: usize = 1024 * 1024;
/// Maximum provider error document, in bytes (16 KiB).
pub const ERROR_BODY_LIMIT: usize = 16 * 1024;

concord::api! {
    /// Generated client. Prefer `connect` or `for_fixture` to configure safe transport.
    pub client DigitalOcean(#[sensitive] header(AUTHORIZATION) authorization: reqwest::header::HeaderValue) {
        base "https://api.digitalocean.com";
        path "v2";
        format Json;
        limit SUCCESS_BODY_LIMIT;
        send Policy -> Result<Response<_>, Error>;

        /// Validated page parameters, shared only by list endpoints.
        params Paging(query page?: PageNumber, query per_page?: PageSize);

        /// The droplet collection.
        scope droplets {
            path "droplets";
            /// List one page; absent pagination links mean completion.
            /// `set_tag_name` lists only the droplets carrying that tag.
            GET list(..Paging, query tag_name?: TagName) -> Droplets {
                page next url links?.pages?.next;
            }
        }
        /// One selected droplet, its action history and its snapshots.
        scope droplet(id: u64) {
            path "droplets/{id}";
            /// Retrieve this droplet.
            GET get() -> DropletDocument;
            /// List one page of this droplet's actions.
            GET actions(..Paging) -> Actions {
                path "actions";
                page next url links?.pages?.next;
            }
            /// Retrieve one action belonging to this droplet.
            GET action(action_id: u64) -> ActionDocument { path "actions/{action_id}"; }
            /// List one page of this droplet's snapshots and backups.
            GET snapshots(..Paging) -> Snapshots {
                path "snapshots";
                page next url links?.pages?.next;
            }
        }
    }
}

impl DigitalOcean {
    /// Sends `authorization: Bearer <token>`, built by `concord_auth::bearer`.
    ///
    /// Construct for the API host. This performs no provider call.
    pub fn connect(token: &str) -> Result<Self, Error> {
        Ok(Self::new(
            transport()?,
            DigitalOceanConfig {
                authorization: concord_auth::bearer(token).map_err(|_| Error::Credential)?,
            },
        ))
    }

    /// Construct for a local HTTP fixture origin (a literal loopback IP, with no base path).
    ///
    /// Links are trusted only within this exact origin and endpoint collection.
    /// Production hosts cannot be selected through this constructor.
    pub fn for_fixture(token: &str, origin: &str) -> Result<Self, Error> {
        let url = reqwest::Url::parse(origin).map_err(|_| Error::FixtureOrigin)?;
        let loopback = url
            .host_str()
            .and_then(|host| {
                host.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .ok()
            })
            .is_some_and(|ip| ip.is_loopback());
        if url.scheme() != "http"
            || !loopback
            || url.path() != "/"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::FixtureOrigin);
        }
        Self::connect(token)?
            .with_base_url(origin)
            .map_err(Error::from)
    }
}

fn transport() -> Result<reqwest::Client, Error> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .referer(false)
        .no_proxy()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| Error::TransportSetup)
}

/// One any-status execution with bounded success/error decoding and no automatic repetition.
#[derive(Debug, Default)]
pub struct Policy;

impl<T: Send> SendWith<T> for Policy {
    type Output = Result<Response<T>, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        Self::finish(endpoint.prepare()?.receive().await?).await
    }
}

impl Policy {
    pub(crate) async fn finish<T>(received: Received<T>) -> Result<Response<T>, Error> {
        if !received.status().is_success() {
            let (head, document) = received.document::<Json, Problem>(ERROR_BODY_LIMIT).await;
            return Err(Error::Provider(Box::new(ProviderError {
                meta: ResponseMeta(head),
                document,
            })));
        }
        let (value, head) = received.decode_with_meta().await?;
        Ok(Response {
            value,
            meta: ResponseMeta(head),
        })
    }
}

/// A decoded value and its response metadata. Debug withholds the value.
#[derive(concord::Debug)]
#[sensitive]
pub struct Response<T> {
    /// The provider's typed document.
    pub value: T,
    /// Status, operation identity and strictly parsed quota headers.
    #[public]
    pub meta: ResponseMeta,
}

/// Response metadata; formatting never includes header values.
#[derive(Debug)]
pub struct ResponseMeta(pub(crate) concord::ResponseMeta);

impl ResponseMeta {
    /// HTTP status, including on failed error-document decoding.
    pub fn status(&self) -> reqwest::StatusCode {
        self.0.status()
    }
    /// Declared endpoint identity.
    pub fn operation(&self) -> Option<concord::Operation> {
        self.0.operation()
    }
    /// Provider quota limit. No local quota estimate is inferred.
    pub fn limit(&self) -> Result<Option<u64>, concord::FieldError> {
        concord::field::unsigned(self.0.headers(), "ratelimit-limit")
    }
    /// Remaining provider quota.
    pub fn remaining(&self) -> Result<Option<u64>, concord::FieldError> {
        concord::field::unsigned(self.0.headers(), "ratelimit-remaining")
    }
    /// Unix seconds when the oldest counted request expires; not a whole-window reset.
    pub fn reset_epoch_seconds(&self) -> Result<Option<u64>, concord::FieldError> {
        concord::field::unsigned(self.0.headers(), "ratelimit-reset")
    }
    /// Numeric burst-limit delay in seconds. This SDK never sleeps or retries automatically.
    pub fn retry_after_seconds(&self) -> Result<Option<u64>, concord::FieldError> {
        concord::field::unsigned(self.0.headers(), "retry-after")
    }
}

/// A nonzero one-based page number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageNumber(std::num::NonZeroU32);
impl PageNumber {
    /// Refuse page zero before constructing an operation.
    pub fn new(value: u32) -> Result<Self, Error> {
        std::num::NonZeroU32::new(value)
            .map(Self)
            .ok_or(Error::PageNumber)
    }
}
impl fmt::Display for PageNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Number of records requested per page, between 1 and 200 inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSize(u16);
impl PageSize {
    /// Validate the documented provider range.
    pub fn new(value: u16) -> Result<Self, Error> {
        if (1..=200).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Error::PageSize)
        }
    }
}
impl fmt::Display for PageSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The name of a droplet tag, the `tag_name` filter of the droplet listing.
///
/// An empty value names no tag (how the provider treats it is undocumented; it could list every
/// droplet), so it is refused, as are whitespace and control characters. Other text is sent
/// percent-encoded; the provider decides whether a droplet carries that exact tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagName(String);
impl TagName {
    /// Refuse an empty name, whitespace and control characters before any request.
    pub fn new(value: &str) -> Result<Self, Error> {
        if value.is_empty() || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(Error::TagName);
        }
        Ok(Self(value.to_owned()))
    }
    /// The tag as the provider spells it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for TagName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
