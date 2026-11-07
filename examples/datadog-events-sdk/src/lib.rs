#![doc = include_str!("../README.md")]

mod api;
mod client;
mod credentials;
mod error;
mod model;
mod query;
mod site;

pub use api::{
    DD_API_KEY, DD_APPLICATION_KEY, DatadogApi, DatadogApiConfig, DatadogApiEvents,
    DatadogApiEventsList, DatadogApiEventsSearch, SendPolicy,
};
pub use client::{DatadogEvents, PageInfo, SearchPages};
pub use credentials::Credentials;
pub use error::{
    AuthError, Error, ErrorBodyError, ErrorDocument, Key, ProtocolError, ProviderError, Rejected,
};
pub use model::{Event, EventAttributes, EventsMeta, EventsPage, Links, PageMeta, Warning};
pub use query::{Cursor, InputError, PageLimit, SearchRequest, Sort, TimeRange};
pub use site::{Site, UnknownSite};

/// The Concord crate this SDK's public types come from (`concord::Error`, `ResponseMeta`).
pub use concord;
/// The pagination crate of [`SearchPages`] (`Resume`, `PagesError`, `Incomplete`).
pub use concord_pagination as pagination;

/// The most bytes read from a successful response (8 MiB); a longer body is a Concord
/// `Limit` error and is not decoded.
pub const RESPONSE_LIMIT: usize = 8 * 1024 * 1024;

/// The most bytes read from a non-2xx error document (64 KiB).
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;
