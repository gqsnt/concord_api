//! Mastodon's documents, as far as this slice reads them, and the SDK's validated values.

use std::fmt;

use concord::reqwest::Url;
use concord::reqwest::header::HeaderValue;
use serde::Deserialize;

/// A Mastodon instance: the origin its REST API is served from (`https://mastodon.social`).
///
/// [`parse`](Self::parse) accepts an absolute `http` or `https` URL with a host and nothing after
/// the origin but an optional `/`: Mastodon's documented paths (`/api/v1/...`) are absolute, so
/// a path prefix, a query, a fragment or user information is refused rather than silently
/// dropped. The text is never echoed in an error (it could carry user information).
#[derive(Clone, PartialEq, Eq)]
pub struct InstanceUrl {
    url: Url,
}

impl InstanceUrl {
    /// Validates an instance URL (see the type documentation).
    ///
    /// # Errors
    ///
    /// [`InvalidInstanceUrl`], naming the rule, never the input.
    pub fn parse(input: &str) -> Result<Self, InvalidInstanceUrl> {
        let url = Url::parse(input).map_err(|_| InvalidInstanceUrl("is not an absolute URL"))?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(InvalidInstanceUrl("must use http or https"));
        }
        check_origin(&url).map_err(InvalidInstanceUrl)?;
        Ok(Self { url })
    }

    /// The normalized URL (`https://mastodon.social/`).
    pub fn as_str(&self) -> &str {
        self.url.as_str()
    }

    pub(crate) fn url(&self) -> &Url {
        &self.url
    }
}

/// The origin rules shared by an instance URL and a streaming host: a host, no port 0, no user
/// information, no query or fragment, no path beyond `/`.
pub(crate) fn check_origin(url: &Url) -> Result<(), &'static str> {
    if url.host().is_none() {
        return Err("must have a host");
    }
    if url.port() == Some(0) {
        return Err("must not use port 0");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("must not contain user information");
    }
    if url.query().is_some() {
        return Err("must not contain a query");
    }
    if url.fragment().is_some() {
        return Err("must not contain a fragment");
    }
    if url.path() != "/" {
        return Err("must not contain a path");
    }
    Ok(())
}

impl fmt::Debug for InstanceUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("InstanceUrl")
            .field(&self.url.as_str())
            .finish()
    }
}

impl fmt::Display for InstanceUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.url.as_str())
    }
}

/// [`InstanceUrl::parse`] refused its input: the rule it broke (the input is not shown).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidInstanceUrl(&'static str);

impl InvalidInstanceUrl {
    /// The rule the input broke (`"must not contain a path"`, ...).
    pub fn reason(&self) -> &'static str {
        self.0
    }
}

impl fmt::Display for InvalidInstanceUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a Mastodon instance URL: it {}", self.0)
    }
}

impl std::error::Error for InvalidInstanceUrl {}

/// A user access token, sent as `Authorization: Bearer <token>`.
///
/// Built with `concord_auth::bearer`: the token must be non-empty visible ASCII without spaces;
/// the header value is flagged sensitive. `Debug` never shows it, and the SDK sends it only to
/// the instance it was given to and to the streaming host that instance advertises.
#[derive(Clone)]
pub struct AccessToken(HeaderValue);

impl AccessToken {
    /// Validates a token.
    ///
    /// # Errors
    ///
    /// [`InvalidToken`] for an empty token, whitespace, a control character or non-ASCII text.
    pub fn new(token: &str) -> Result<Self, InvalidToken> {
        concord_auth::bearer(token)
            .map(Self)
            .map_err(|_| InvalidToken)
    }

    pub(crate) fn header(&self) -> &HeaderValue {
        &self.0
    }
}

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccessToken(<redacted>)")
    }
}

/// [`AccessToken::new`] refused its input (which is not shown).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidToken;

impl fmt::Display for InvalidToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an access token must be non-empty visible ASCII without spaces")
    }
}

impl std::error::Error for InvalidToken {}

/// A status id: an opaque string (`"103206804533200177"`). The SDK requires it to be non-empty
/// and free of whitespace and control characters; it never compares ids as numbers.
#[derive(Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct StatusId(String);

impl StatusId {
    /// Validates an id.
    ///
    /// # Errors
    ///
    /// [`InvalidStatusId`] for an empty id or one with whitespace or a control character.
    pub fn new(id: impl Into<String>) -> Result<Self, InvalidStatusId> {
        let id = id.into();
        if id.is_empty() || id.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(InvalidStatusId);
        }
        Ok(Self(id))
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for StatusId {
    type Error = InvalidStatusId;

    fn try_from(id: String) -> Result<Self, InvalidStatusId> {
        Self::new(id)
    }
}

impl fmt::Display for StatusId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for StatusId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("StatusId").field(&self.0).finish()
    }
}

/// [`StatusId::new`] refused its input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidStatusId;

impl fmt::Display for InvalidStatusId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a status id must be non-empty, without whitespace or control characters")
    }
}

impl std::error::Error for InvalidStatusId {}

/// The `limit` of a timeline page: 1 to [`MAX`](Self::MAX) statuses (the documented maximum;
/// the server's default is 20).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimelineLimit(u8);

impl TimelineLimit {
    /// The documented maximum: 40 statuses.
    pub const MAX: u8 = 40;

    /// Validates a limit.
    ///
    /// # Errors
    ///
    /// [`InvalidLimit`] for 0 or more than [`MAX`](Self::MAX): nothing is sent.
    pub fn new(limit: u8) -> Result<Self, InvalidLimit> {
        if limit == 0 || limit > Self::MAX {
            return Err(InvalidLimit);
        }
        Ok(Self(limit))
    }

    /// The number of statuses.
    pub fn get(self) -> u8 {
        self.0
    }
}

impl fmt::Display for TimelineLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// [`TimelineLimit::new`] refused its input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidLimit;

impl fmt::Display for InvalidLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a timeline limit is 1 to 40 statuses")
    }
}

impl std::error::Error for InvalidLimit {}

/// A status (a post), as far as this slice decodes it. Other members are ignored.
///
/// Required: `id`, `uri`, `created_at`, `account`, `content` and `visibility`; a document
/// without one of them does not decode. `Debug` shows the content: a status is the caller's data.
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct Status {
    /// The status id on this instance.
    pub id: StatusId,
    /// The ActivityPub URI.
    pub uri: String,
    /// The HTML page of the status, when there is one.
    #[serde(default)]
    pub url: Option<String>,
    /// When it was created (ISO 8601 text, as sent).
    pub created_at: String,
    /// When it was last edited, if ever (ISO 8601 text, as sent).
    #[serde(default)]
    pub edited_at: Option<String>,
    /// The author.
    pub account: Account,
    /// The content, as HTML.
    pub content: String,
    /// Who may see it.
    pub visibility: Visibility,
    /// Whether the content is marked sensitive.
    #[serde(default)]
    pub sensitive: bool,
    /// The content warning, empty if none.
    #[serde(default)]
    pub spoiler_text: String,
    /// The language (ISO 639), when known.
    #[serde(default)]
    pub language: Option<String>,
    /// The status this one replies to.
    #[serde(default)]
    pub in_reply_to_id: Option<StatusId>,
    /// The account this one replies to.
    #[serde(default)]
    pub in_reply_to_account_id: Option<String>,
    /// The status this one boosts.
    #[serde(default)]
    pub reblog: Option<Box<Status>>,
}

/// The author of a [`Status`], as far as this slice decodes it.
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct Account {
    /// The account id on this instance.
    pub id: String,
    /// The user name (`aiueohisama`).
    pub username: String,
    /// The user name for this instance: `username` for a local account, `username@domain` for a
    /// remote one.
    pub acct: String,
    /// The display name.
    #[serde(default)]
    pub display_name: String,
    /// The profile page.
    #[serde(default)]
    pub url: Option<String>,
}

/// The visibility of a [`Status`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(from = "String")]
pub enum Visibility {
    /// `public`.
    Public,
    /// `unlisted`.
    Unlisted,
    /// `private`.
    Private,
    /// `direct`.
    Direct,
    /// A value this SDK does not know, kept as sent.
    Other(String),
}

impl From<String> for Visibility {
    fn from(value: String) -> Self {
        match value.as_str() {
            "public" => Self::Public,
            "unlisted" => Self::Unlisted,
            "private" => Self::Private,
            "direct" => Self::Direct,
            _ => Self::Other(value),
        }
    }
}

/// The instance document (`GET /api/v2/instance`), as far as this slice reads it: the streaming
/// URL. `configuration` is required (a document without it does not decode); `urls` and
/// `streaming` are optional; the rest of the document is ignored.
#[derive(Clone, Debug, Default, Deserialize)]
#[non_exhaustive]
pub struct InstanceInfo {
    configuration: Configuration,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Configuration {
    #[serde(default)]
    urls: Option<Urls>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Urls {
    #[serde(default)]
    streaming: Option<String>,
}

impl InstanceInfo {
    /// `configuration.urls.streaming`: where the instance serves its streaming API, as
    /// advertised (`None` when absent, `null` or empty). Apply
    /// [`StreamingHost::advertised_by`](crate::StreamingHost::advertised_by) before use.
    pub fn streaming_url(&self) -> Option<&str> {
        self.configuration
            .urls
            .as_ref()?
            .streaming
            .as_deref()
            .filter(|url| !url.is_empty())
    }
}

/// Mastodon's error document: `{"error": "Record not found"}`.
#[derive(Clone, Debug, Deserialize)]
pub struct ErrorDocument {
    error: String,
}

impl ErrorDocument {
    /// The `error` text, as the server wrote it.
    pub fn error(&self) -> &str {
        &self.error
    }
}
