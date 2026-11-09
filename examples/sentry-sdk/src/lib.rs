#![doc = include_str!("../README.md")]

mod deployment;
mod error;
mod model;
mod pagination;
mod policy;
mod rate;
mod update;

pub use concord;
pub use concord_pagination::{PagesError, Resume};
pub use deployment::Deployment;
pub use error::{Error, HttpClass, Problem, ProviderError};
pub use model::{Event, Issue, Response};
pub use pagination::Listing;
pub use policy::{SendPolicy, Settings};
pub use rate::RateLimits;
pub use update::{Ignore, MAX_IGNORE_WINDOW_MINUTES, StatusUpdate};

use concord::{
    Json,
    reqwest::{
        self,
        header::{AUTHORIZATION, HeaderValue},
    },
};

/// Maximum decoded success document size, including a whole page (1 MiB).
pub const BODY_LIMIT: usize = 1024 * 1024;
/// Maximum provider error document size (16 KiB).
pub const ERROR_BODY_LIMIT: usize = 16 * 1024;

concord::api! {
    /// Sentry issue and event reads and issue status updates. Prefer [`Sentry::with_token`]
    /// for validated bearer auth.
    pub client Sentry(
        /// The deployment owning this caller and its admission state.
        deployment: Deployment,
        /// Sensitive, complete Authorization header; `with_token` constructs this safely.
        #[sensitive] header(AUTHORIZATION) authorization: HeaderValue,
        /// Shared admission and bounded read retries, scoped to one caller identity.
        policy: SendPolicy,
    ) {
        base deployment;
        error crate::Problem in concord::Json;
        path "api/0";
        format Json;
        limit BODY_LIMIT;
        send policy -> Result<Response<_>, Error>;

        /// Organization id or slug, encoded as one segment.
        scope organization(organization: String) {
            path "organizations/{organization}";

            /// Organization issues; omission of `query` keeps Sentry's unresolved default.
            GET issues(
                query limit: u32 = 100,
                #[sensitive] query cursor?: String,
                #[sensitive] query query?: String,
            ) -> Vec<Issue> { path "issues/"; page next link; }

            /// One issue within this organization.
            scope issue(issue: String) {
                path "issues";
                /// Retrieve issue details.
                GET get() -> Issue { path "{issue.issue}/"; }
                /// Page event summaries. `full` remains the provider default (false).
                GET events(
                    query per_page: u32 = 100,
                    #[sensitive] query cursor?: String,
                    #[sensitive] query query?: String,
                ) -> Vec<Event> { path "{issue.issue}/events/"; page next link; }
                /// Retrieve an event by eventID (also accepts latest/oldest/recommended).
                GET event(event: String) -> Event { path "{issue.issue}/events/{event}/"; }
                /// Change this issue's status; returns the updated issue. Only 429 is retried:
                /// an update without a response head is [`Error::Uncertain`] (re-read the issue).
                PUT update_status(body change: StatusUpdate) -> Issue { path "{issue.issue}/"; }
            }
        }
    }
}

impl Sentry {
    /// Sends `authorization: Bearer <token>`, built by `concord_auth::bearer`.
    ///
    /// Construct with a validated token. Clones of the policy share caller/endpoint quotas.
    pub fn with_token(
        http: reqwest::Client,
        token: &str,
        policy: SendPolicy,
    ) -> Result<Self, Error> {
        Self::at(http, Deployment::hosted(), token, policy)
    }

    /// Construct for one checked hosted or self-hosted deployment and caller identity.
    pub fn at(
        http: reqwest::Client,
        deployment: Deployment,
        token: &str,
        policy: SendPolicy,
    ) -> Result<Self, Error> {
        let authorization = concord_auth::bearer(token).map_err(|_| Error::Credential)?;
        Ok(Self::new(
            http,
            SentryConfig {
                deployment,
                authorization,
                policy,
            },
        ))
    }
}

/// Transport with a 10 second request timeout, no redirects, no internal retries or proxy.
/// SDK retry counts therefore describe application sends without hidden transport replay.
pub fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .build()
}

pub(crate) use __SentryOperation as SentryOperation;
