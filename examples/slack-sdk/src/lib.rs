//! A read-only Slack Web API slice: users pagination and conversation information.
//!
//! Construct [`Client`] with the caller's configured HTTP transport and [`ClientConfig`].
//! Every domain operation applies HTTP status and Slack's `ok` semantics, retaining metadata
//! on errors. There are no retries, sleeps, credential refreshes, or background tasks.

mod error;
mod models;
mod wire;

pub use concord_pagination as pagination;
pub use error::{
    EnvelopeProblem, Error, ErrorBody, ErrorCode, ProviderError, ResponseMeta, RetryAfter,
};
pub use models::{Conversation, Cursor, ResponseMetadata, User, UsersPage};
pub use wire::SlackApiConfig as ClientConfig;

use concord::{Endpoint, reqwest};
use pagination::{Page, Pages, Resume};
use wire::{Envelope, SlackApi};

/// Maximum bytes of a 2xx body decoded per request (16 MiB). An SDK safety bound against an
/// endpoint that never stops sending, not a Slack-documented size: a larger body is
/// [`Error::Concord`] with `ErrorKind::Limit` and the response metadata, and nothing beyond the
/// limit is read. A full `users.list` page at `limit` 1000 is expected to be far smaller; lower
/// [`UsersOptions::limit`] if a page reaches it.
pub const SUCCESS_BODY_LIMIT: usize = 16 * 1024 * 1024;

/// A completely decoded, successful response. Warnings are preserved even when `ok` is true.
#[derive(Debug)]
pub struct Response<T> {
    /// Domain value.
    pub value: T,
    /// HTTP status, headers, and declared operation identity.
    pub meta: ResponseMeta,
    /// Top-level warning code(s), when supplied.
    pub warning: Option<String>,
    /// Body-level messages, warnings, and pagination cursor.
    pub response_metadata: Option<ResponseMetadata>,
}

/// Page size constrained to 1–1000; the SDK defaults to Slack's recommended 200.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSize(u16);

impl PageSize {
    /// A supported size, or `None` for zero or a value above 1000.
    pub fn new(value: u16) -> Option<Self> {
        (1..=1000).contains(&value).then_some(Self(value))
    }

    /// Numeric limit sent on every users request.
    pub fn get(self) -> u16 {
        self.0
    }
}

impl Default for PageSize {
    fn default() -> Self {
        Self(200)
    }
}

/// Stable options for a users traversal, shared unchanged across its requests.
#[derive(Clone, Debug, Default)]
pub struct UsersOptions {
    /// Maximum users per page. An explicit limit is always sent.
    pub limit: PageSize,
    /// Request user locales; absent uses Slack's default false.
    pub include_locale: Option<bool>,
    /// Workspace to select with an organization-level token.
    pub team_id: Option<String>,
}

/// Optional fields for `conversations.info`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ConversationOptions {
    /// Request the conversation locale.
    pub include_locale: Option<bool>,
    /// Request the member count.
    pub include_num_members: Option<bool>,
}

/// Metadata retained per page by [`Users`]. Item traversal drops page metadata.
#[derive(Debug)]
pub struct UsersPageMeta {
    /// HTTP metadata.
    pub http: ResponseMeta,
    /// Top-level warning, if supplied.
    pub warning: Option<String>,
    /// Full response metadata, including continuation.
    pub response_metadata: Option<ResponseMetadata>,
    /// Cache timestamp, if supplied.
    pub cache_ts: Option<u64>,
}

/// A lazy users traversal. `Ok(None)` means provider exhaustion; caps and failures are typed
/// errors. Use `items().into_stream()` for a standard Rust stream, or `collect_complete()` to
/// keep the collected prefix if traversal fails. Cursors are only for prompt in-memory use.
pub type Users = Pages<User, Cursor, UsersPageMeta, Error>;

/// Configured Slack client. The token is a raw bearer token, without `Bearer `.
///
/// ```
/// use concord::reqwest;
/// use concord_sdk_slack::{Client, ClientConfig};
/// let http = reqwest::Client::builder()
///     .timeout(std::time::Duration::from_secs(15))
///     .redirect(reqwest::redirect::Policy::none())
///     .build()?;
/// let slack = Client::new(http, ClientConfig { token: "synthetic-slack-token".into() });
/// # Ok::<(), reqwest::Error>(())
/// ```
///
/// The caller controls transport policies. This SDK acquires no token or credentials.
#[derive(Clone, Debug)]
pub struct Client {
    api: SlackApi,
}

impl Client {
    /// Use `https://slack.com/api` with the caller's transport and bearer token.
    pub fn new(http: reqwest::Client, config: ClientConfig) -> Self {
        Self {
            api: SlackApi::new(http, config),
        }
    }

    /// Replace the API root, including `/api`, for a loopback fixture server.
    ///
    /// # Errors
    /// Returns Concord's URL validation error before any I/O.
    pub fn with_base_url(self, base: impl AsRef<str>) -> Result<Self, concord::Error> {
        Ok(Self {
            api: self.api.with_base_url(base)?,
        })
    }

    /// Fetch one page, using the exact cursor from the previous response or `None` to start.
    /// Do not pre-encode the cursor or change options while continuing a listing.
    /// Concord query-encodes the raw value once: `=` becomes `%3D`, while literal `%3D`
    /// becomes `%253D`. Nonempty cursors made only of padding or punctuation still continue.
    ///
    /// # Errors
    /// Returns [`Error`] for HTTP, application, metadata, envelope, transport, or decoding errors.
    pub async fn users_page(
        &self,
        options: &UsersOptions,
        cursor: Option<&Cursor>,
    ) -> Result<Response<UsersPage>, Error> {
        let response = execute(
            self.api
                .users_list(options.limit.get())
                // Keep the provider string raw; the declared query placement owns encoding.
                .maybe_cursor(cursor.map(|c| c.as_str().to_owned()))
                .maybe_include_locale(options.include_locale)
                .maybe_team_id(options.team_id.clone()),
        )
        .await?;
        let members = response.value.members.ok_or_else(|| Error::Envelope {
            problem: EnvelopeProblem::MissingMembers,
            meta: Box::new(response.meta.clone()),
        })?;
        Ok(Response {
            value: UsersPage {
                members,
                cache_ts: response.value.cache_ts,
            },
            meta: response.meta,
            warning: response.warning,
            response_metadata: response.response_metadata,
        })
    }

    /// Start a lazy listing at the first page. Empty pages continue when they carry a cursor.
    /// No cursor is persisted; discard a stale traversal and start again after hours or days.
    /// Configure an optional `max_pages` cap on the returned traversal.
    pub fn users(&self, options: UsersOptions) -> Users {
        let client = self.clone();
        Pages::new(Resume::Start, move |cursor: Option<Cursor>| {
            let client = client.clone();
            let options = options.clone();
            async move {
                let response = client.users_page(&options, cursor.as_ref()).await?;
                let next = response
                    .response_metadata
                    .as_ref()
                    .and_then(|m| m.next_cursor.clone());
                Ok(Page {
                    items: response.value.members,
                    next,
                    meta: UsersPageMeta {
                        http: response.meta,
                        warning: response.warning,
                        response_metadata: response.response_metadata,
                        cache_ts: response.value.cache_ts,
                    },
                })
            }
        })
    }

    /// Retrieve a conversation, including DM shapes that omit `name`.
    ///
    /// # Errors
    /// A 200/`ok: false` is [`Error::Api`], including `channel_not_found`. All other failure
    /// classes also retain metadata once response headers arrive.
    pub async fn conversation_info(
        &self,
        channel: impl Into<String>,
        options: ConversationOptions,
    ) -> Result<Response<Conversation>, Error> {
        let response = execute(
            self.api
                .conversations_info(channel.into())
                .maybe_include_locale(options.include_locale)
                .maybe_include_num_members(options.include_num_members),
        )
        .await?;
        let channel = response.value.channel.ok_or_else(|| Error::Envelope {
            problem: EnvelopeProblem::MissingChannel,
            meta: Box::new(response.meta.clone()),
        })?;
        Ok(Response {
            value: channel,
            meta: response.meta,
            warning: response.warning,
            response_metadata: response.response_metadata,
        })
    }
}

async fn execute<T>(endpoint: impl Endpoint<Envelope<T>>) -> Result<Response<T>, Error> {
    let received = endpoint.prepare()?.receive().await?;
    let meta = ResponseMeta {
        status: received.status(),
        headers: received.head().headers().clone(),
        operation: received.operation(),
    };
    if !meta.status.is_success() {
        let body = Box::new(ErrorBody::new(received.bytes_limited(64 * 1024).await));
        return Err(if meta.status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            Error::RateLimited {
                retry_after: meta.retry_after(),
                meta: Box::new(meta),
                body,
            }
        } else {
            Error::Http {
                meta: Box::new(meta),
                body,
            }
        });
    }
    let envelope = received
        .decode_limited(SUCCESS_BODY_LIMIT)
        .await
        .map_err(|error| Error::Concord {
            error,
            meta: Some(Box::new(meta.clone())),
        })?;
    if !envelope.ok {
        return Err(match envelope.into_provider() {
            Ok(error) => Error::Api {
                error: Box::new(error),
                meta: Box::new(meta),
            },
            Err(problem) => Error::Envelope {
                problem,
                meta: Box::new(meta),
            },
        });
    }
    if envelope.error.is_some() {
        return Err(Error::Envelope {
            problem: EnvelopeProblem::ConflictingError,
            meta: Box::new(meta),
        });
    }
    Ok(Response {
        value: envelope.payload,
        meta,
        warning: envelope.warning,
        response_metadata: envelope.response_metadata,
    })
}

/// Compile the README's complete developer workflows.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
