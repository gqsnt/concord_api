#![doc = include_str!("../README.md")]

use concord::{
    Json,
    reqwest::{
        self,
        header::{AUTHORIZATION, HeaderValue},
    },
};
use std::time::Duration;

/// Concord types used by the generated public API.
pub use concord;
/// Pagination reports, checkpoints and incomplete results.
pub use concord_pagination as pagination;
/// Strict, value-free Retry-After parse errors.
pub use concord_retry::http::RetryAfterError;

mod error;
mod events;
mod message;
mod policy;

pub use error::{Error, HttpClass, InputError, LinkError, Problem, ProviderError};
pub use events::{
    DeliveryStatus, Domain, Event, EventCursor, EventFlags, EventMessage, EventPage, EventPages,
    EventRange, MessageHeaders, PageSize, Paging, delivery_status,
};
pub use message::{
    Accepted, Attachment, MAX_ATTACHMENT_BYTES, MAX_TEXT_BYTES, MailgunMultipart, MessageId,
    TextEmail,
};
pub use policy::{HeaderNumber, Metadata, Response, SendPolicy};

/// Maximum bytes collected for a successful event page.
pub const EVENT_BODY_LIMIT: usize = 1024 * 1024;
/// Maximum bytes collected for a send acknowledgement or provider error.
pub const SMALL_BODY_LIMIT: usize = 16 * 1024;

/// Region in which the sending domain was provisioned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Region {
    /// United States.
    Us,
    /// European Union.
    Eu,
}

impl Region {
    /// Regional API origin; endpoint versions live in the declaration.
    pub const fn base_url(self) -> &'static str {
        match self {
            Self::Us => "https://api.mailgun.net",
            Self::Eu => "https://api.eu.mailgun.net",
        }
    }
}

concord::api! {
    /// Mailgun text messages and domain event searches.
    ///
    /// Prefer `connect` or `local` for a transport that never retries or follows redirects.
    /// Advanced `new` callers must supply an equivalent transport.
    pub client Mailgun(#[sensitive] header(AUTHORIZATION) authorization: HeaderValue) {
        base "https://api.mailgun.net";
        path "v3";
        response Json;
        limit EVENT_BODY_LIMIT;
        send SendPolicy;

        /// One sending domain, in the client's region.
        scope domain(domain: Domain) {
            path "{domain}";

            /// Submit one text email with an attachment, once. Success means queued only.
            POST send_message(body email: TextEmail) -> Accepted {
                path "messages";
                request MailgunMultipart;
                limit SMALL_BODY_LIMIT;
            }

            /// Search a finite time range. Use `pages` to follow provider links.
            GET events(
                query(..) range: EventRange,
                query limit: PageSize = PageSize::default(),
                #[sensitive] query("message-id") message_id?: MessageId,
                #[sensitive] query recipient?: String,
                query event?: String,
            ) -> EventPage {
                path "events";
            }
        }
    }
}

impl Mailgun {
    /// Build a regional client with a finite whole-request timeout, no redirects and no retries.
    /// A key with event-read permission is required for the connected workflow.
    pub fn connect(region: Region, key: &str, timeout: Duration) -> Result<Self, Error> {
        Self::configured(region.base_url(), key, timeout, false)
    }

    /// Build an HTTP loopback-only fixture client. No proxy is used for local fixtures.
    pub fn local(base: &str, key: &str, timeout: Duration) -> Result<Self, Error> {
        let url = reqwest::Url::parse(base).map_err(|_| Error::Input(InputError::BaseUrl))?;
        let loopback = url
            .host_str()
            .and_then(|s| s.trim_matches(['[', ']']).parse::<std::net::IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback());
        if !loopback || url.scheme() != "http" {
            return Err(Error::Input(InputError::BaseUrl));
        }
        Self::configured(base, key, timeout, true)
    }

    fn configured(base: &str, key: &str, timeout: Duration, local: bool) -> Result<Self, Error> {
        if key.is_empty() || key.trim() != key {
            return Err(Error::Input(InputError::Credential));
        }
        if timeout.is_zero() {
            return Err(Error::Input(InputError::Timeout));
        }
        let authorization =
            concord_auth::basic("api", key).map_err(|_| Error::Input(InputError::Credential))?;
        let mut builder = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .referer(false);
        if local {
            builder = builder.no_proxy();
        }
        let http = builder
            .build()
            .map_err(|_| Error::Input(InputError::TransportConfig))?;
        Ok(Self::new(http, MailgunConfig { authorization }).with_base_url(base)?)
    }
}
