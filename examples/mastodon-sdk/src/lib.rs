//! A bounded [Mastodon](https://docs.joinmastodon.org) SDK slice declared with `concord::api!`:
//! read public and hashtag timelines, retrieve one status, discover the instance's streaming
//! host and read public and hashtag streams as server-sent events.
//!
//! Checked against the Mastodon documentation pages listed in `PROVENANCE.md` (read
//! 2026-11-07). Not covered: posting or any other write, WebSocket streaming, other timelines and
//! streams, federation semantics, OAuth flows (obtaining a token), rate-limit headers, and any
//! exactly-once or replay guarantee for streamed events.
//!
//! # Construction
//!
//! The instance is the caller's, chosen at run time: [`Mastodon::new`] takes your
//! `reqwest::Client` (timeouts, proxies, TLS) and an [`InstanceUrl`]. [`Mastodon::with_token`]
//! adds a user [`AccessToken`] (`Authorization: Bearer …`, flagged sensitive, withheld from every
//! `Debug`). A client is bound to its instance: there is no way to point it, and its token, at
//! another host, except the streaming host that instance advertises. The generated client,
//! [`Api`], is reached with [`Mastodon::api`]; it is bound the same way (its constructor and
//! `with_base_url` are this crate's).
//!
//! ```
//! use concord::reqwest;
//! use concord_sdk_mastodon::{AccessToken, InstanceUrl, Mastodon, TimelineLimit};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let instance = InstanceUrl::parse("https://mastodon.example")?;
//! let mastodon = Mastodon::new(reqwest::Client::new(), instance)
//!     .with_token(&AccessToken::new("synthetic-token-not-real")?);
//!
//! let page = mastodon
//!     .public_timeline()
//!     .set_local(true)
//!     .set_limit(TimelineLimit::new(40)?)
//!     .prepare()?;
//! assert_eq!(
//!     page.request().url().as_str(),
//!     "https://mastodon.example/api/v1/timelines/public?local=true&limit=40"
//! );
//! assert!(!format!("{mastodon:?} {page:?}").contains("synthetic-token"));
//! # Ok(()) }
//! ```
//!
//! # Operations
//!
//! | Method | Request | `send()` returns |
//! |---|---|---|
//! | [`public_timeline`](Mastodon::public_timeline) + `set_local`, `set_remote`, `set_only_media`, `set_max_id`, `set_since_id`, `set_min_id`, `set_limit` | `GET /api/v1/timelines/public` | `Vec<`[`Status`]`>` |
//! | [`hashtag_timeline`](Mastodon::hashtag_timeline) + `set_any`, `set_all`, `set_none` and the public timeline setters | `GET /api/v1/timelines/tag/{hashtag}` | `Vec<`[`Status`]`>` |
//! | [`status`](Mastodon::status) | `GET /api/v1/statuses/{id}` | [`Status`] |
//! | [`instance`](Mastodon::instance) | `GET /api/v2/instance` | [`InstanceInfo`] |
//! | [`discover_streaming`](Mastodon::discover_streaming) | `GET /api/v2/instance` | [`StreamingHost`] |
//! | [`public_stream`](Mastodon::public_stream) + `set_only_media` | `GET /api/v1/streaming/public` on the streaming host | an open [`PublicStream`] |
//! | [`hashtag_stream`](Mastodon::hashtag_stream) / [`local_hashtag_stream`](Mastodon::local_hashtag_stream) | `GET /api/v1/streaming/hashtag` / `hashtag/local` with `tag` on the streaming host | an open [`PublicStream`] |
//!
//! Every `send()` returns `Result<_, Error>`: a non-2xx answer is [`Error::Api`] with
//! Mastodon's error document; every other failure is Concord's error, kept whole
//! ([`Error::Request`]). `prepare()` builds the native request without sending it (it carries
//! the token: a native request you redirect elsewhere is your decision).
//!
//! # Example: the two workflows
//!
//! ```no_run
//! use concord_sdk_mastodon::{Mastodon, StreamEvent};
//!
//! # async fn run(mastodon: Mastodon) -> Result<(), Box<dyn std::error::Error>> {
//! // 1. A page of the public timeline, then one of its statuses.
//! let page = mastodon.public_timeline().send().await?;
//! if let Some(first) = page.first() {
//!     let status = mastodon.status(first.id.clone()).send().await?;
//!     println!("{}: {}", status.account.acct, status.content);
//! }
//!
//! // 2. The streaming host, then the public timeline as it happens.
//! let host = mastodon.discover_streaming().await?;
//! let mut stream = mastodon.public_stream(&host)?.send().await?;
//! while let Some(event) = stream.next().await {
//!     match event {
//!         Ok(StreamEvent::Update(status)) => println!("new: {}", status.id),
//!         Ok(StreamEvent::Delete(id)) => println!("deleted: {id}"),
//!         Ok(_) => {}
//!         Err(error) if !error.is_terminal() => eprintln!("skipped: {error}"),
//!         Err(error) => {
//!             // Disconnected, closed by the server, truncated...: nothing is replayed.
//!             eprintln!("stream stopped: {error}");
//!         }
//!     }
//! }
//! # Ok(()) }
//! ```

mod api;
mod error;
mod model;
pub mod streaming;

use concord::reqwest;

pub use concord;

pub use api::Api;
pub use api::{
    ApiHashtagStream as HashtagStreamRequest, ApiHashtagTimeline as HashtagTimelineRequest,
    ApiInstance as InstanceRequest, ApiLocalHashtagStream as LocalHashtagStreamRequest,
    ApiPublicStream as PublicStreamRequest, ApiPublicTimeline as PublicTimelineRequest,
    ApiStatus as StatusRequest,
};
pub use error::{ApiError, ERROR_BODY_LIMIT, Error, MastodonPolicy};
pub use model::{
    AccessToken, Account, ErrorDocument, Hashtag, InstanceInfo, InstanceUrl, InvalidHashtag,
    InvalidInstanceUrl, InvalidLimit, InvalidStatusId, InvalidToken, Status, StatusId,
    TimelineLimit, Visibility,
};
pub use streaming::{
    DiscoveryError, MAX_EVENT_BYTES, PublicStream, PublicStreamFormat, StreamError,
    StreamErrorKind, StreamEvent, StreamingHost,
};

/// The README's code blocks, compiled as doctests so they cannot drift from the API.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

/// A client for one Mastodon instance, with or without a user token.
///
/// Its methods return the generated request values: chain their setters, then `send()` (or
/// `prepare()` for the native request). Handles are cheap to clone and share.
#[derive(Clone)]
pub struct Mastodon {
    api: api::Api,
    instance: InstanceUrl,
}

impl Mastodon {
    /// A client for `instance`, without a token. `http` is used for every request: configure
    /// timeouts, proxies and TLS on it. reqwest's default redirect policy drops `Authorization`
    /// when a redirect leaves the origin; a policy of yours must keep that rule.
    pub fn new(http: reqwest::Client, instance: InstanceUrl) -> Self {
        let api = match api::Api::new(http).with_base_url(instance.as_str()) {
            Ok(api) => api,
            // `InstanceUrl::parse` applies every rule of `with_base_url` and more.
            Err(_) => unreachable!("an InstanceUrl is a valid base URL"),
        };
        Self { api, instance }
    }

    /// The same client sending `token` on every request, to this instance and to the streaming
    /// host it advertises. Request values created earlier keep their credentials.
    #[must_use]
    pub fn with_token(mut self, token: &AccessToken) -> Self {
        self.api = self.api.set_authorization(token.header());
        self
    }

    /// The instance.
    pub fn instance_url(&self) -> &InstanceUrl {
        &self.instance
    }

    /// The generated client, bound to this instance with this client's token: the endpoint
    /// methods below are its own (`mastodon.api().status(id)` is `mastodon.status(id)`). It
    /// cannot be pointed at another host.
    pub fn api(&self) -> &Api {
        &self.api
    }

    /// A page of the public timeline (`GET /api/v1/timelines/public`).
    pub fn public_timeline(&self) -> PublicTimelineRequest {
        self.api.public_timeline()
    }

    /// A page of public statuses containing `hashtag` (`GET /api/v1/timelines/tag/{hashtag}`).
    /// The validated name is encoded as one path segment. The instance controls access.
    pub fn hashtag_timeline(&self, hashtag: Hashtag) -> HashtagTimelineRequest {
        self.api.hashtag_timeline(hashtag)
    }

    /// One status (`GET /api/v1/statuses/{id}`).
    pub fn status(&self, id: StatusId) -> StatusRequest {
        self.api.status(id)
    }

    /// The instance document (`GET /api/v2/instance`).
    pub fn instance(&self) -> InstanceRequest {
        self.api.instance()
    }

    /// Reads the instance document and returns the streaming host it advertises
    /// (`configuration.urls.streaming`, read by [`StreamingHost::advertised_by`]).
    ///
    /// # Errors
    ///
    /// The instance document's [`Error`], or [`Error::Discovery`] when it advertises no usable
    /// streaming host. Nothing is guessed: an instance known to stream on its own host is
    /// [`StreamingHost::instance_host`].
    pub async fn discover_streaming(&self) -> Result<StreamingHost, Error> {
        let info = self.instance().send().await?;
        let advertised = info.streaming_url().ok_or(DiscoveryError::NotAdvertised)?;
        Ok(StreamingHost::advertised_by(&self.instance, advertised)?)
    }

    /// The public timeline stream on `host` (`GET /api/v1/streaming/public`), with this client's
    /// token. `send()` returns the open [`PublicStream`] once the head is accepted.
    ///
    /// # Errors
    ///
    /// [`Error::Discovery`] ([`DiscoveryError::OtherInstance`]) when `host` was advertised by
    /// another instance: nothing is sent.
    pub fn public_stream(&self, host: &StreamingHost) -> Result<PublicStreamRequest, Error> {
        Ok(self.streaming_api(host)?.public_stream())
    }

    /// The `hashtag` stream on the advertised host (`GET /api/v1/streaming/hashtag?tag=…`).
    /// `send()` returns [`PublicStream`], sharing its framing and terminal-disconnect rules.
    /// Requires a user token with `read:statuses`; nothing is reconnected or replayed.
    ///
    /// # Errors
    ///
    /// [`Error::Discovery`] ([`DiscoveryError::OtherInstance`]) if the host belongs to another
    /// instance, before any request is sent.
    pub fn hashtag_stream(
        &self,
        host: &StreamingHost,
        tag: Hashtag,
    ) -> Result<HashtagStreamRequest, Error> {
        Ok(self.streaming_api(host)?.hashtag_stream(tag))
    }

    /// The `hashtag:local` stream (`GET /api/v1/streaming/hashtag/local?tag=…`).
    /// Uses the same token, instance binding and [`PublicStream`] reader as [`Self::hashtag_stream`].
    ///
    /// # Errors
    ///
    /// [`Error::Discovery`] ([`DiscoveryError::OtherInstance`]) if the host belongs to another
    /// instance, before any request is sent.
    pub fn local_hashtag_stream(
        &self,
        host: &StreamingHost,
        tag: Hashtag,
    ) -> Result<LocalHashtagStreamRequest, Error> {
        Ok(self.streaming_api(host)?.local_hashtag_stream(tag))
    }

    fn streaming_api(&self, host: &StreamingHost) -> Result<api::Api, Error> {
        if host.instance() != &self.instance {
            return Err(DiscoveryError::OtherInstance.into());
        }
        Ok(self.api.clone().with_base_url(host.url())?)
    }
}

impl std::fmt::Debug for Mastodon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The generated client's `Debug` shows the base and withholds the token.
        f.debug_struct("Mastodon").field("api", &self.api).finish()
    }
}
