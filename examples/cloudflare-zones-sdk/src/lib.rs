//! A bounded slice of the Cloudflare API v4: list the zones of an account and fetch
//! one zone; list, fetch and create a zone's DNS records. Tests run against local fixtures
//! only.
//!
//! The declared client is [`Cloudflare`]. Navigation follows Cloudflare's resources:
//!
//! ```text
//! cf.zones().list()                         GET /client/v4/zones
//! cf.zone(zone_id).get()                    GET /client/v4/zones/{zone_id}
//! cf.zone(zone_id).dns_records().list()     GET /client/v4/zones/{zone_id}/dns_records
//! cf.zone(zone_id).dns_record(id).get()     GET /client/v4/zones/{zone_id}/dns_records/{id}
//! cf.zone(zone_id).dns_records().create(record) POST /client/v4/zones/{zone_id}/dns_records
//! ```
//!
//! Each generated `send()` goes through [`SendPolicy`] and returns
//! `Result<Response<T>, Error>` (single objects) or `Result<PageResponse<T>, Error>` (one page
//! of a listing). `success: false` is always [`Error::Provider`], never an empty result. The
//! listings also walk every page: `cf.zones().list().pages()` ([`ZonePages`]). See the package
//! README for failure semantics and what is excluded.

mod create;
mod error;
mod models;
mod pagination;
mod policy;
mod ratelimit;
pub mod wire;

pub use concord;
pub use concord_auth;
pub use concord_pagination;

pub use create::{NewDnsRecord, RecordInputError, RecordTtl};
pub use error::{EnvelopeError, EnvelopeProblem, Error, ProviderError, ResponseMeta};
pub use models::{
    AccountId, Direction, DnsRecord, RecordId, RecordOrder, RecordType, RecordsPerPage, Zone,
    ZoneAccount, ZoneId, ZoneOrder, ZoneStatus, ZoneType, ZonesPerPage,
};
pub use pagination::{PageMeta, RecordPages, ZonePages};
pub use policy::{PageResponse, Response, SendPolicy};
pub use ratelimit::{RateLimit, RateLimitHeaderError, RateLimitPolicy, RateLimits, RetryAfter};
pub use wire::{ErrorEnvelope, PageInfo, ResponseInfo};

use std::num::NonZeroU32;

use concord::Json;
use concord::reqwest::{self, header::AUTHORIZATION, header::HeaderValue};

use wire::{Paged, Single};

/// The largest successful body read (16 MiB): a longer one is [`Error::Concord`] with
/// `ErrorKind::Limit`, and nothing more is read. An SDK bound, not a Cloudflare figure: a List
/// DNS Records page with a very large `per_page` can exceed it; ask for smaller pages.
pub const SUCCESS_BODY_LIMIT: usize = 16 * 1024 * 1024;

/// The largest error body read (64 KiB) for a non-2xx response.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

concord::api! {
    /// Cloudflare API v4, `https://api.cloudflare.com/client/v4`, with an API token.
    pub client Cloudflare(
        /// `Bearer {api_token}`; build it with [`Cloudflare::connect`] or
        /// `concord_auth::bearer(token)` (sensitive, validated).
        #[sensitive] header(AUTHORIZATION) authorization: HeaderValue,
        /// What generated `send()` does; [`SendPolicy::default`] sends each request once.
        mut policy: SendPolicy,
    ) {
        base "https://api.cloudflare.com";
        path "client/v4";
        format Json;
        limit SUCCESS_BODY_LIMIT;
        send policy;

        /// `page` of a page-numbered listing (minimum 1). The page walkers set it.
        params Paging(query page?: NonZeroU32);

        /// The zones collection.
        scope zones {
            path "zones";

            /// List Zones. Filter by account with `set_account_id`; walk every page with
            /// `.pages()`.
            GET list(
                /// `account.id`: only the zones of this account.
                query("account.id") account_id?: AccountId,
                /// `name`: a domain name (Cloudflare accepts filter operators in the value).
                query name?: String,
                /// `status`.
                query status?: ZoneStatus,
                /// `order`: the field to order zones by.
                query order?: ZoneOrder,
                /// `direction` of `order`.
                query direction?: Direction,
                ..Paging,
                /// `per_page`: 5 to 50.
                query per_page?: ZonesPerPage,
            ) -> Paged<Zone>;
        }

        /// One zone.
        scope zone(zone_id: ZoneId) {
            path "zones/{zone_id}";

            /// Zone Details.
            GET get() -> Single<Zone>;

            /// The zone's DNS records.
            scope dns_records {
                path "dns_records";

                /// Create a record from validated input. Never retried, even with the opt-in
                /// 429 policy. On [`Error::CreateUncertain`], list records to reconcile.
                POST create(body record: NewDnsRecord) -> Single<DnsRecord>;

                /// List DNS Records of the zone; walk every page with `.pages()`.
                GET list(
                    /// `type`: only records of this type.
                    query("type") record_type?: RecordType,
                    /// `proxied`.
                    query proxied?: bool,
                    /// `order`: the field to order records by.
                    query order?: RecordOrder,
                    /// `direction` of `order`.
                    query direction?: Direction,
                    ..Paging,
                    /// `per_page`: 1 to 5,000,000.
                    query per_page?: RecordsPerPage,
                ) -> Paged<DnsRecord>;
            }

            /// One DNS record of the zone.
            scope dns_record(record_id: RecordId) {
                path "dns_records/{record_id}";

                /// DNS Record Details.
                GET get() -> Single<DnsRecord>;
            }
        }
    }
}

impl Cloudflare {
    /// Sends `authorization: Bearer <api_token>`, built by `concord_auth::bearer`.
    ///
    /// A client for `https://api.cloudflare.com/client/v4` with an API token (sent as
    /// `Authorization: Bearer {api_token}`) and the default [`SendPolicy`]. Creating, rolling
    /// and scoping tokens is the application's.
    ///
    /// # Errors
    ///
    /// [`Error::Credential`] if the token is empty or not visible ASCII without spaces.
    pub fn connect(http: reqwest::Client, api_token: &str) -> Result<Self, Error> {
        let authorization = concord_auth::bearer(api_token).map_err(Error::Credential)?;
        Ok(Self::new(
            http,
            CloudflareConfig {
                authorization,
                policy: SendPolicy::default(),
            },
        ))
    }
}

/// A transport for this SDK: no transport-level retries (repetition belongs to
/// [`SendPolicy`]), no redirects, a 30-second timeout per request. Any `reqwest::Client` works;
/// this one keeps the documented semantics exact.
///
/// # Errors
///
/// reqwest's, if the client cannot be built.
pub fn http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
}

/// Compiles the README examples without making requests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeExamples;
