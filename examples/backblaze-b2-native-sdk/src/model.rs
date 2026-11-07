//! Validated inputs and the domain values B2 returns.

use std::collections::BTreeMap;
use std::fmt;

use concord::reqwest::{Url, header::HeaderValue};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use crate::error::{InputError, Violation};

/// An application key: the key ID and the key, held only as the sensitive Basic credential
/// `b2_authorize_account` takes. `Debug` prints neither.
#[derive(Clone)]
pub struct ApplicationKey {
    credentials: HeaderValue,
}

impl ApplicationKey {
    /// A key ID (or, for the master key, the account ID) and its application key.
    ///
    /// # Errors
    ///
    /// [`InputError::ApplicationKey`] for an empty ID or key, a colon in the ID, or a control
    /// character in either.
    pub fn new(key_id: &str, application_key: &str) -> Result<Self, InputError> {
        if key_id.is_empty() || application_key.is_empty() {
            return Err(InputError::ApplicationKey);
        }
        let credentials =
            concord_auth::basic(key_id, application_key).map_err(|_| InputError::ApplicationKey)?;
        Ok(Self { credentials })
    }

    pub(crate) fn credentials(&self) -> &HeaderValue {
        &self.credentials
    }
}

impl fmt::Debug for ApplicationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApplicationKey(<redacted>)")
    }
}

fn identifier(value: String, error: InputError) -> Result<String, InputError> {
    if value.is_empty() || value.chars().any(char::is_control) {
        Err(error)
    } else {
        Ok(value)
    }
}

/// A bucket ID (`bucketId`), kept as given.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BucketId(String);

impl BucketId {
    /// # Errors
    ///
    /// [`InputError::BucketId`] for an empty value or one with a control character.
    pub fn new(value: impl Into<String>) -> Result<Self, InputError> {
        identifier(value.into(), InputError::BucketId).map(Self)
    }

    /// The ID as given.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A file version's ID (`fileId`), kept as given.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileId(String);

impl FileId {
    /// # Errors
    ///
    /// [`InputError::FileId`] for an empty value or one with a control character.
    pub fn new(value: impl Into<String>) -> Result<Self, InputError> {
        identifier(value.into(), InputError::FileId).map(Self)
    }

    /// The ID as given.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Bytes sent as themselves in `X-Bz-File-Name`; every other byte of the UTF-8 name is
/// percent-encoded (`b2_upload_file`: "percent-encoded UTF-8 ... spaces should be replaced with
/// %20"). Encoding more than strictly needed is decoded the same way.
const NAME: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'/');

/// A file name. Other name rules (length, forbidden characters) are B2's: a name it refuses is
/// a `400 bad_request`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileName(String);

impl FileName {
    /// # Errors
    ///
    /// [`InputError::FileName`] for an empty name.
    pub fn new(value: impl Into<String>) -> Result<Self, InputError> {
        let value = value.into();
        if value.is_empty() {
            return Err(InputError::FileName);
        }
        Ok(Self(value))
    }

    /// The name as given (not encoded).
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The `X-Bz-File-Name` spelling: percent-encoded UTF-8.
    pub fn header_value(&self) -> String {
        utf8_percent_encode(&self.0, NAME).to_string()
    }
}

/// The `Content-Type` of an upload: a media type, or [`ContentType::auto`] (`b2/x-auto`).
#[derive(Clone, PartialEq, Eq)]
pub struct ContentType(HeaderValue);

impl ContentType {
    /// # Errors
    ///
    /// [`InputError::ContentType`] unless the value parses as `type/subtype[; params]` and is
    /// header-safe text.
    pub fn new(value: &str) -> Result<Self, InputError> {
        let parsed: mime::Mime = value.parse().map_err(|_| InputError::ContentType)?;
        if parsed.type_().as_str().is_empty() || parsed.subtype().as_str().is_empty() {
            return Err(InputError::ContentType);
        }
        HeaderValue::from_str(value)
            .map(Self)
            .map_err(|_| InputError::ContentType)
    }

    /// `b2/x-auto`: B2 sets the stored type from the file name's extension.
    pub fn auto() -> Self {
        Self(HeaderValue::from_static("b2/x-auto"))
    }

    /// The header value.
    pub fn as_str(&self) -> &str {
        self.0.to_str().expect("validated as visible ASCII")
    }

    pub(crate) fn header(&self) -> HeaderValue {
        self.0.clone()
    }
}

impl fmt::Debug for ContentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ContentType").field(&self.as_str()).finish()
    }
}

/// A SHA-1 digest as 40 lowercase hexadecimal digits.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Sha1Hex(String);

impl Sha1Hex {
    /// Accepts 40 hexadecimal digits in either case and keeps them lowercase.
    ///
    /// # Errors
    ///
    /// [`InputError::Sha1`] otherwise.
    pub fn new(value: &str) -> Result<Self, InputError> {
        if value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
            Ok(Self(value.to_ascii_lowercase()))
        } else {
            Err(InputError::Sha1)
        }
    }

    /// The SHA-1 of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self(sha1_smol::Sha1::from(bytes).digest().to_string())
    }

    /// The digits.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha1Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A bucket an application key is restricted to (`allowed.buckets[]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllowedBucket {
    /// `id`.
    pub id: BucketId,
    /// `name`; `null` when the bucket name is not available to the key.
    pub name: Option<String>,
}

/// The key's restrictions (`allowed`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Allowed {
    /// Buckets the key is restricted to (empty: all buckets).
    pub buckets: Vec<AllowedBucket>,
    /// Capability names, as sent (`writeFiles`, `readFiles`, ...).
    pub capabilities: Vec<String>,
    /// File-name prefix restriction, when present.
    pub name_prefix: Option<String>,
}

impl Allowed {
    /// Whether the key has the capability named exactly `name`.
    pub fn has_capability(&self, name: &str) -> bool {
        self.capabilities.iter().any(|c| c == name)
    }
}

/// What `b2_authorize_account` returned, without the token (it stays inside the
/// [`Session`](crate::Session)).
#[derive(Clone)]
pub struct AccountInfo {
    pub(crate) account_id: String,
    pub(crate) api_url: Url,
    pub(crate) download_url: Url,
    pub(crate) allowed: Allowed,
    pub(crate) key_expiration: Option<u64>,
}

impl AccountInfo {
    /// `accountId`. For the master key it doubles as the key ID, so `Debug` withholds it.
    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    /// `apiInfo.storageApi.apiUrl`: where upload-URL, file-name list and version-delete calls go.
    pub fn api_url(&self) -> &Url {
        &self.api_url
    }

    /// `apiInfo.storageApi.downloadUrl`: where `b2_download_file_by_id` goes.
    pub fn download_url(&self) -> &Url {
        &self.download_url
    }

    /// `apiInfo.storageApi.allowed`.
    pub fn allowed(&self) -> &Allowed {
        &self.allowed
    }

    /// `applicationKeyExpirationTimestamp` (milliseconds since the epoch), when the key expires.
    pub fn key_expiration_millis(&self) -> Option<u64> {
        self.key_expiration
    }
}

impl fmt::Debug for AccountInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccountInfo")
            .field("account_id", &"<redacted>")
            .field("api_url", &self.api_url.as_str())
            .field("download_url", &self.download_url.as_str())
            .field("allowed", &self.allowed)
            .field("key_expiration", &self.key_expiration)
            .finish()
    }
}

/// A stored file version, as `b2_upload_file` returned it (the fields this slice reads).
/// `Debug` withholds the account ID.
#[derive(Clone, PartialEq, Eq)]
pub struct FileVersion {
    /// `fileId`.
    pub file_id: FileId,
    /// `fileName`.
    pub file_name: String,
    /// `bucketId`.
    pub bucket_id: BucketId,
    /// `accountId`.
    pub account_id: String,
    /// `action` (`upload` for a stored upload).
    pub action: String,
    /// `contentLength`.
    pub content_length: u64,
    /// `contentSha1`.
    pub content_sha1: Sha1Hex,
    /// `contentMd5`, when B2 sent one.
    pub content_md5: Option<String>,
    /// `contentType`, as stored.
    pub content_type: Option<String>,
    /// `fileInfo`.
    pub file_info: BTreeMap<String, String>,
    /// `uploadTimestamp` (milliseconds since the epoch).
    pub upload_timestamp: u64,
}

impl fmt::Debug for FileVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileVersion")
            .field("file_id", &self.file_id)
            .field("file_name", &self.file_name)
            .field("bucket_id", &self.bucket_id)
            .field("account_id", &"<redacted>")
            .field("action", &self.action)
            .field("content_length", &self.content_length)
            .field("content_sha1", &self.content_sha1)
            .field("content_md5", &self.content_md5)
            .field("content_type", &self.content_type)
            .field("file_info", &self.file_info)
            .field("upload_timestamp", &self.upload_timestamp)
            .finish()
    }
}

/// A token B2 issued; never printed.
#[derive(Clone)]
pub(crate) struct Token(String);

impl Token {
    pub(crate) fn issued(value: String) -> Result<Self, Violation> {
        if value.is_empty() || HeaderValue::from_str(&value).is_err() {
            return Err(Violation::IssuedToken);
        }
        Ok(Self(value))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

/// An upload URL and its own authorization token.
#[derive(Clone)]
pub(crate) struct UploadUrl {
    pub(crate) url: Url,
    pub(crate) token: Token,
}

impl UploadUrl {
    /// `scheme://host[:port]`: the base the upload scope is bound to before `follow`.
    pub(crate) fn origin(&self) -> String {
        self.url.origin().ascii_serialization()
    }
}

/// Where uploads to one bucket go: the bucket and, while it is usable, the upload URL and upload
/// token `b2_get_upload_url` issued.
///
/// B2: the pair is "valid for 24 hours or until the endpoint rejects an upload". After any
/// upload attempt that was not stored, [`upload_file`](crate::Session::upload_file) retires it,
/// and the next upload first obtains a new one. A new URL says nothing about earlier attempts.
/// One target serves one upload at a time (`&mut`): B2 refuses a token already in use
/// (`auth_token_limit`). `Debug` withholds the URL and the token.
pub struct UploadTarget {
    pub(crate) bucket: BucketId,
    pub(crate) current: Option<UploadUrl>,
}

impl UploadTarget {
    /// The bucket uploads go to.
    pub fn bucket_id(&self) -> &BucketId {
        &self.bucket
    }

    /// Whether an issued upload URL is held (not retired).
    pub fn is_usable(&self) -> bool {
        self.current.is_some()
    }

    /// The issued upload URL, **unredacted**, while usable.
    pub fn upload_url(&self) -> Option<&Url> {
        self.current.as_ref().map(|current| &current.url)
    }
}

impl fmt::Debug for UploadTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadTarget")
            .field("bucket", &self.bucket)
            .field("upload_url", &self.current.as_ref().map(|_| "<withheld>"))
            .field("token", &self.current.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// An `http`/`https` URL B2 issued, checked against the URL of the request that issued it: no
/// credentials or fragment, no query where a base is expected, and no `https` → `http` downgrade.
pub(crate) fn issued_url(issuer: &Url, issued: &str, base: bool) -> Result<Url, Violation> {
    let url = Url::parse(issued).map_err(|_| Violation::IssuedUrl)?;
    let scheme_ok = matches!(url.scheme(), "http" | "https");
    let downgrade = issuer.scheme() == "https" && url.scheme() != "https";
    if !scheme_ok
        || downgrade
        || url.host_str().is_none_or(str::is_empty)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || (base && url.query().is_some())
    {
        return Err(Violation::IssuedUrl);
    }
    Ok(url)
}

/// A base URL of the caller's (no issuer to compare with): the same rules as an issued base.
pub(crate) fn base_url(value: &str) -> Option<Url> {
    let any = Url::parse("http://caller.invalid").expect("parses");
    issued_url(&any, value, true).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_https_issuer_cannot_issue_an_http_url() {
        let issuer = Url::parse("https://api.backblazeb2.com").unwrap();
        assert_eq!(
            issued_url(&issuer, "http://api001.backblazeb2.com", true),
            Err(Violation::IssuedUrl)
        );
        assert!(issued_url(&issuer, "https://api001.backblazeb2.com", true).is_ok());
        let http = Url::parse("http://127.0.0.1:1").unwrap();
        assert!(issued_url(&http, "http://127.0.0.1:2", true).is_ok());
    }

    #[test]
    fn issued_urls_carry_no_credentials_fragment_or_base_query() {
        let issuer = Url::parse("https://api001.backblazeb2.com").unwrap();
        for bad in [
            "https://user:pass@pod.backblaze.com/x",
            "https://pod.backblaze.com/x#frag",
            "ftp://pod.backblaze.com/x",
            "not a url",
        ] {
            assert_eq!(
                issued_url(&issuer, bad, false),
                Err(Violation::IssuedUrl),
                "{bad}"
            );
        }
        let query = "https://pod-000-1005-03.backblaze.com/b2api/v4/b2_upload_file?cvt=c&bucket=b";
        assert!(issued_url(&issuer, query, false).is_ok());
        assert_eq!(issued_url(&issuer, query, true), Err(Violation::IssuedUrl));
    }
}
