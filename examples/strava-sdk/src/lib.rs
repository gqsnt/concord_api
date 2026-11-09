#![doc = include_str!("../README.md")]
pub use concord;
pub use concord_pagination as pagination;
pub use concord_retry as retry;

mod auth;
mod error;
mod rate;
mod update;
mod upload;

pub use auth::{Credentials, Session, TokenRequest, Tokens};
pub use error::{Error, ErrorDocument, HttpFailure, Problem, ProblemField};
pub use rate::{Quota, RateLimits, Window};
pub use update::ActivityUpdate;
pub use upload::{DataType, PollPolicy, StreamMultipart, UploadFile, UploadStatus};

use concord::{Endpoint, Form, Json, Received, SendWith, reqwest};
use serde::Deserialize;
use std::{num::NonZeroUsize, time::Duration};

/// Maximum finite success body (1 MiB).
pub const BODY_LIMIT: usize = 1024 * 1024;
/// Maximum provider error document (16 KiB).
pub const ERROR_LIMIT: usize = 16 * 1024;

/// A deliberately small projection shared by summary, detail and update responses.
/// Optional fields are `None` when the provider omits them or sends `null`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Activity {
    /// Strava's 64-bit activity identifier.
    pub id: u64,
    /// Athlete-supplied title.
    pub name: String,
    /// Distance in metres.
    pub distance: f64,
    /// Moving duration in seconds.
    pub moving_time: u64,
    /// Case-sensitive sport name; unknown future sports remain readable.
    pub sport_type: String,
    /// Athlete-supplied description. Detail and update responses only: summaries omit it, so a
    /// listed activity and its detail differ here when a description exists.
    pub description: Option<String>,
    /// Identifier of the associated gear; `None` when there is none.
    pub gear_id: Option<String>,
    /// Whether the activity is a commute.
    pub commute: Option<bool>,
    /// Whether the activity was recorded on a training machine.
    pub trainer: Option<bool>,
}

/// A typed value and safe response metadata. Debug withholds the value.
#[derive(concord::Debug)]
#[sensitive]
pub struct Response<T> {
    /// Decoded document, available deliberately.
    pub value: T,
    /// Original HTTP head; Debug prints header names only.
    #[public]
    pub meta: concord::ResponseMeta,
    /// Independently parsed quota observations.
    #[public]
    pub rates: RateLimits,
    /// Application executions (OAuth calls counted separately).
    #[public]
    pub attempts: usize,
}

concord::api! {
    /// Strava client. Construct with [`Strava::connect`] to enforce transport policy.
    pub client Strava(policy: Session) {
        base "https://www.strava.com";
        path "api/v3";
        response Json;
        limit BODY_LIMIT;
        send policy -> Result<Response<_>, Error>;
        /// Every Strava operation, as the send policy classifies it (scope, quota windows,
        /// retries, uncertainty).
        operations StravaOperation;

        /// The authorized athlete (no arbitrary athlete identifier).
        scope athlete {
            path "athlete";
            /// One page. Prefer [`StravaAthlete::pages`] for a bounded traversal.
            GET activities(query page: u32 = 1, query per_page: u16 = 30,
                query before?: u64, query after?: u64) -> Vec<Activity> {
                path "activities";
            }
        }
        /// Activity detail operations.
        scope activities {
            path "activities";
            /// Retrieve one visible activity.
            GET get(id: u64, query include_all_efforts: bool = false) -> Activity {
                path "{id}";
            }
            /// Change only the fields set on `changes` of an activity the athlete owns; returns
            /// the updated activity. Requires `activity:write`. A timeout is uncertain: re-read
            /// the activity. Never resent automatically.
            PUT update(id: u64, body changes: ActivityUpdate) -> Activity {
                path "{id}";
                request Json;
            }
        }
        /// Asynchronously processed activity files.
        scope uploads {
            path "uploads";
            /// Submit once. A timeout is ambiguous; never automatically replayed.
            POST create(body file: UploadFile) -> UploadStatus { request StreamMultipart; }
            /// Retrieve processing state for the returned upload identifier.
            GET status(id: u64) -> UploadStatus { path "{id}"; }
        }
    }
}

concord::api! {
    /// OAuth token service, separate from authenticated API traffic.
    pub client OAuth {
        base "https://www.strava.com";
        response Json;
        limit ERROR_LIMIT;
        send TokenPolicy -> Result<Response<_>, Error>;
        /// Token grants.
        scope oauth {
            path "oauth";
            /// Exchange an authorization code or the latest refresh token, once.
            POST token(body grant: TokenRequest) -> Tokens { path "token"; request Form; }
        }
    }
}

/// Stateless OAuth response/error policy. No token grant is retried.
#[derive(Default)]
pub struct TokenPolicy;
impl<T: Send> SendWith<T> for TokenPolicy {
    type Output = Result<Response<T>, Error>;
    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        finish(endpoint.prepare()?.receive().await?, false, 1).await
    }
}

pub(crate) async fn finish<T>(
    received: Received<T>,
    uncertain: bool,
    attempts: usize,
) -> Result<Response<T>, Error> {
    let rates = RateLimits::parse(received.head().headers());
    if !received.status().is_success() {
        let ambiguous = uncertain && received.status().is_server_error();
        let (meta, document) = received.document::<Json, ErrorDocument>(ERROR_LIMIT).await;
        return Err(Error::Http(Box::new(HttpFailure {
            meta,
            document,
            rates,
            ambiguous,
        })));
    }
    let (value, meta) = received
        .decode_with_meta()
        .await
        .map_err(|e| Error::from_call(e, uncertain))?;
    Ok(Response {
        value,
        meta,
        rates,
        attempts,
    })
}

/// Build a transport with finite timeouts, redirects and native retry disabled.
pub fn transport(timeout: Duration) -> Result<reqwest::Client, Error> {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .referer(false)
        .retry(reqwest::retry::never())
        .build()
        .map_err(|_| Error::Invalid("transport configuration"))
}

impl Strava {
    /// Configure OAuth and API destinations. Use the documented HTTPS origin in production;
    /// loopback HTTP is accepted for fixtures. Other origins must use HTTPS.
    pub fn connect(
        api_origin: &str,
        oauth_origin: &str,
        credentials: Credentials,
        tokens: Tokens,
        timeout: Duration,
    ) -> Result<Self, Error> {
        Ok(Session::connect(api_origin, oauth_origin, credentials, tokens, timeout)?.client())
    }
}

/// Metadata retained once per activity page; items are owned by the page itself.
#[derive(Debug)]
pub struct PageMeta {
    /// Original HTTP head.
    pub meta: concord::ResponseMeta,
    /// Parsed provider windows.
    pub rates: RateLimits,
    /// Application executions for this logical page.
    pub attempts: usize,
}
/// Bounded activity pages with explicit completion, errors and resume positions.
pub type ActivityPages = pagination::Pages<Activity, u32, PageMeta, Error>;

impl StravaAthlete {
    /// Page until an empty page, with an explicit logical-page cap and resume position.
    /// Keep the same filters and page size on resume; changing activities can cause gaps. Use `concord_sdk_strava::pagination::Resume` for the position. `Resume::Start` repeats page 1 of the athlete's activities with `per_page`; `Resume::At(key)` takes the `u32` page number in a returned page's `next`.
    pub fn pages(
        &self,
        per_page: u16,
        from: pagination::Resume<u32>,
        max_pages: NonZeroUsize,
    ) -> Result<ActivityPages, Error> {
        if !(1..=200).contains(&per_page) || matches!(from, pagination::Resume::At(0)) {
            return Err(Error::Invalid("page size or starting page"));
        }
        let scope = self.clone();
        Ok(pagination::Pages::new(from, move |page: Option<u32>| {
            let scope = scope.clone();
            async move {
                let page = page.unwrap_or(1);
                let response = scope
                    .activities()
                    .set_page(page)
                    .set_per_page(per_page)
                    .send()
                    .await?;
                if response.value.len() > usize::from(per_page) {
                    return Err(Error::Invalid("page exceeds requested size"));
                }
                let next = if response.value.is_empty() {
                    None
                } else {
                    Some(
                        page.checked_add(1)
                            .ok_or(Error::Invalid("page number overflow"))?,
                    )
                };
                let Response {
                    value: items,
                    meta,
                    rates,
                    attempts,
                } = response;
                Ok(pagination::Page {
                    items,
                    next,
                    meta: PageMeta {
                        meta,
                        rates,
                        attempts,
                    },
                })
            }
        })
        .max_pages(max_pages))
    }
}
