//! The Concord declaration of the two Events v2 read operations, and the send policy every
//! generated `send()` goes through.

use concord::reqwest::header::HeaderName;
use concord::{Endpoint, Json, SendWith};

use crate::error::{AuthError, Error, ErrorBodyError, ErrorDocument, ProviderError, Rejected};
use crate::model::EventsPage;
use crate::query::{Cursor, PageLimit, SearchRequest, Sort, TimeRange};
use crate::site::Site;
use crate::{ERROR_BODY_LIMIT, RESPONSE_LIMIT};

/// `DD-API-KEY`: the header that carries the API key.
pub const DD_API_KEY: HeaderName = HeaderName::from_static("dd-api-key");
/// `DD-APPLICATION-KEY`: the header that carries the application key.
pub const DD_APPLICATION_KEY: HeaderName = HeaderName::from_static("dd-application-key");

concord::api! {
    /// The Datadog Events v2 read API of one site.
    ///
    /// Build it through [`DatadogEvents::new`](crate::DatadogEvents::new), which checks both
    /// keys and installs the site's API host; constructing it directly skips those checks.
    /// The declared base is the US1 host; a site's own host is installed with
    /// `with_base_url`. Every generated `send()` returns `Result<_, `[`Error`](crate::Error)`>`
    /// through [`SendPolicy`]; `prepare()` and `with_base_url` return [`concord::Error`].
    pub client DatadogApi(
        /// The API key, sent as `DD-API-KEY`.
        #[sensitive] header(DD_API_KEY) api_key: String,
        /// The application key, sent as `DD-APPLICATION-KEY`: read requests require it.
        #[sensitive] header(DD_APPLICATION_KEY) application_key: String,
    ) {
        base "https://api.datadoghq.com";
        path "api/v2";
        format Json;
        limit RESPONSE_LIMIT;
        send SendPolicy -> Result<_, Error>;

        /// Events.
        scope events {
            path "events";

            /// `GET /api/v2/events`: the events matching a query, newest or oldest first, one
            /// page per call (`page[cursor]` continues).
            GET list(
                /// `filter[query]`: a query in the event search syntax.
                query("filter[query]") query?: String,
                /// `filter[from]` and `filter[to]`, in milliseconds.
                query(..) range?: TimeRange,
                /// `sort`.
                query sort?: Sort,
                /// `page[cursor]`: a previous page's `meta.page.after`.
                query("page[cursor]") cursor?: Cursor,
                /// `page[limit]`: the maximum number of events (provider default 10).
                query("page[limit]") limit?: PageLimit,
            ) -> EventsPage;

            /// `POST /api/v2/events/search`: the events matching a filter; the next page is the
            /// same body with `page.cursor` set to the previous `meta.page.after`.
            POST search(body request: SearchRequest) -> EventsPage { path "search"; }
        }
    }
}

/// The send policy of [`DatadogApi`]: what every generated `send()` does.
///
/// One request, no retry. A 2xx body is decoded as declared (JSON, at most
/// [`RESPONSE_LIMIT`] bytes). Any other status reads the error document (at most
/// [`ERROR_BODY_LIMIT`] bytes, strict JSON) and is classified by status: 401 and 403 are
/// [`Error::Authentication`] naming the API host that answered (and its [`Site`]), 400
/// [`Error::BadRequest`], 429 [`Error::RateLimited`], anything else [`Error::Provider`]. The
/// status decides even when the document is absent, malformed, oversized or not JSON.
#[derive(Clone, Copy, Debug, Default)]
pub struct SendPolicy;

impl<T: Send> SendWith<T> for SendPolicy {
    type Output = Result<T, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        let status = received.status();
        // The host this answer came from: the site an authentication error names is the one
        // actually called, whatever base URL the client was given.
        let host = received.url().host_str().unwrap_or_default().to_owned();
        if status.is_success() {
            return Ok(received.decode().await?);
        }
        let (meta, document) = received
            .document::<Json, ErrorDocument>(ERROR_BODY_LIMIT)
            .await;
        let provider = ProviderError {
            meta,
            document: document.map_err(ErrorBodyError::from),
        };
        Err(match status.as_u16() {
            401 | 403 => Error::Authentication(AuthError::Rejected(Box::new(Rejected {
                site: Site::from_api_host(&host),
                host,
                provider,
            }))),
            400 => Error::BadRequest(Box::new(provider)),
            429 => Error::RateLimited(Box::new(provider)),
            _ => Error::Provider(Box::new(provider)),
        })
    }
}
