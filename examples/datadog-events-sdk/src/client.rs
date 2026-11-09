//! The SDK's client: site and keys checked once, reads and posting named.

use std::collections::HashSet;

use concord::reqwest;
use concord_pagination::{Page, Pages, Resume};

use crate::api::{
    DatadogApi, DatadogApiConfig, DatadogApiEvents, DatadogApiEventsList, DatadogApiEventsPost,
    DatadogApiEventsSearch,
};
use crate::credentials::Credentials;
use crate::error::{Error, ProtocolError};
use crate::model::{Event, EventsPage, Warning};
use crate::post::NewEvent;
use crate::query::{Cursor, SearchRequest, TimeRange};
use crate::site::Site;

/// A lazy search traversal: one `POST /api/v2/events/search` per page, each page's metadata
/// kept beside its events.
///
/// `next_page()` returns `Ok(None)` only after a page without `meta.page.after`. A cap
/// (`max_pages`) reached while more pages exist is `PagesError::Bounded` with the cursor to
/// resume from; a failed page is `PagesError::Provider` with this crate's [`Error`] and the
/// cursor of the failed page. `items()` flattens the events and drops page metadata
/// (warnings included); use `next_page()` or `item_events()` to keep it.
pub type SearchPages = Pages<Event, Cursor, PageInfo, Error>;

/// What a search page says about itself, besides its events.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct PageInfo {
    /// `meta.request_id`.
    pub request_id: Option<String>,
    /// `meta.status` (`done` in the documentation).
    pub status: Option<String>,
    /// `meta.warnings`: when present, the page may hold partial results.
    pub warnings: Vec<Warning>,
}

/// A Datadog Events client for one site and one pair of keys.
///
/// Cheap to clone; clones share the `reqwest::Client`'s connection pool. Transport settings
/// (timeouts, proxies, TLS) belong to that client. The keys travel in custom headers, which
/// reqwest forwards on redirects to another host: use [`http_client`](Self::http_client) or
/// configure your client without redirects.
#[derive(Clone, Debug)]
pub struct DatadogEvents {
    api: DatadogApi,
    site: Site,
}

impl DatadogEvents {
    /// A client for `site`, sending both keys with every request.
    ///
    /// ```
    /// use concord_sdk_datadog_events::{Credentials, DatadogEvents, Site};
    ///
    /// let credentials = Credentials::new("synthetic-api-key", "synthetic-app-key").unwrap();
    /// let http = DatadogEvents::http_client().unwrap();
    /// let client = DatadogEvents::new(http, Site::Eu, credentials);
    /// let prepared = client.events().list().prepare().unwrap();
    /// assert_eq!(prepared.request().url().as_str(), "https://api.datadoghq.eu/api/v2/events");
    /// assert!(!format!("{client:?}").contains("synthetic"));
    /// ```
    pub fn new(http: reqwest::Client, site: Site, credentials: Credentials) -> Self {
        let Credentials {
            api_key,
            application_key,
        } = credentials;
        let config = DatadogApiConfig {
            api_key,
            application_key,
        };
        let api = DatadogApi::new(http, config)
            .with_base_url(site.api_base_url())
            .expect("every listed site's API base is an https URL with a domain host");
        Self { api, site }
    }

    /// A `reqwest::Client` suited to this SDK: no redirects (the keys are custom headers that
    /// reqwest would forward to another host) and no transport-level retries.
    ///
    /// # Errors
    ///
    /// reqwest's, when its TLS backend cannot be initialised.
    pub fn http_client() -> reqwest::Result<reqwest::Client> {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
    }

    /// The same client sending to `base` instead of the site's API host: for a proxy or a local
    /// stand-in. `/api/v2/...` (reads) or `/api/v1/events` (post) is appended to `base`'s path.
    /// Authentication errors name the host that answered; [`site`](Self::site) keeps the
    /// configured site.
    ///
    /// # Errors
    ///
    /// [`concord::Error`] when `base` is not an `http`/`https` URL with a host (and without
    /// user information, query or fragment).
    pub fn with_base_url(mut self, base: impl AsRef<str>) -> Result<Self, concord::Error> {
        self.api = self.api.with_base_url(base)?;
        Ok(self)
    }

    /// The configured site, retained when a proxy or fixture overrides the base.
    pub fn site(&self) -> Site {
        self.site
    }

    /// The generated client, for anything the named workflows do not cover.
    pub fn api(&self) -> &DatadogApi {
        &self.api
    }

    /// The generated `events` scope: `list()`, `search(..)` and `post(..)` with every setter.
    pub fn events(&self) -> DatadogApiEvents {
        self.api.events()
    }

    /// Workflow 1: the events of a bounded time range (`GET /api/v2/events` with
    /// `filter[from]` and `filter[to]`). Returns the generated request: chain
    /// `set_query`, `set_sort`, `set_limit`, `set_cursor`, then `send()`; inspect an event of
    /// the page with [`EventsPage::event`].
    pub fn list(&self, range: TimeRange) -> DatadogApiEventsList {
        self.events().list().set_range(range)
    }

    /// One search page (`POST /api/v2/events/search`).
    pub fn search(&self, request: SearchRequest) -> DatadogApiEventsSearch {
        self.events().search(request)
    }

    /// Post an event to this site's v1 API with both keys. Available only to organizations
    /// that previously used v1. Returns HTTP 202 acceptance, never search visibility.
    /// The validated payload cannot exceed documented limits. A timeout is
    /// [`Error::PostUncertain`]: the event may exist. Never automatically replayed, even with
    /// a retrying HTTP client. An aggregation key groups events; it is not an idempotency key.
    pub fn post(&self, event: NewEvent) -> DatadogApiEventsPost {
        self.events().post(event)
    }

    /// Workflow 2: every page of a search, following `meta.page.after` with the same body.
    /// Nothing is sent until the traversal is read. A cursor set on `request` is replaced.
    ///
    /// Besides the provider's own errors, a page whose cursor is empty or was already
    /// requested ends the traversal with [`Error::Protocol`] (following it would restart or
    /// loop). There is no page cap unless you set one (`max_pages`).
    pub fn search_pages(&self, request: SearchRequest) -> SearchPages {
        self.search_pages_from(request, Resume::Start)
    }

    /// [`search_pages`](Self::search_pages) from a checkpoint (`PagesError::resume()` or
    /// `checkpoint()` of an earlier traversal of the same `request`, site and keys). A cursor
    /// is not a snapshot: events added meanwhile may shift pages. Use `concord_sdk_datadog_events::pagination::Resume` for the position. `Resume::Start` repeats the same search body without a cursor; `Resume::At(key)` takes the `Cursor` projected from `meta.page.after` into a returned page's `next`.
    pub fn search_pages_from(&self, request: SearchRequest, from: Resume<Cursor>) -> SearchPages {
        let client = self.clone();
        let mut requested = HashSet::new();
        Pages::new(from, move |cursor: Option<Cursor>| {
            let repeated = cursor
                .as_ref()
                .is_some_and(|cursor| !requested.insert(cursor.clone()));
            let operation = client.search(request.clone().maybe_cursor(cursor));
            async move {
                if repeated {
                    return Err(Error::Protocol(ProtocolError::RepeatedCursor));
                }
                project(operation.send().await?)
            }
        })
    }
}

fn project(page: EventsPage) -> Result<Page<Event, Cursor, PageInfo>, Error> {
    let next = page.next_cursor();
    if next
        .as_ref()
        .is_some_and(|cursor| cursor.as_str().is_empty())
    {
        return Err(Error::Protocol(ProtocolError::EmptyCursor));
    }
    let EventsPage { data, meta, .. } = page;
    let meta = meta.unwrap_or_default();
    Ok(Page {
        items: data,
        next,
        meta: PageInfo {
            request_id: meta.request_id,
            status: meta.status,
            warnings: meta.warnings,
        },
    })
}
