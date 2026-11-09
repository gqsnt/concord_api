#![doc = include_str!("../README.md")]

mod download;
mod error;
mod model;
mod storage;
mod upload;
mod wire;

pub use concord;
pub use concord_retry as retry;
pub use download::{Download, DownloadedFile};
pub use error::{
    ChecksumError, ChecksumFailure, ContractError, Error, ErrorCode, ErrorDocument, InputError,
    ProviderError, Violation,
};
pub use model::{
    AccountInfo, Allowed, AllowedBucket, ApplicationKey, BucketId, ContentType, FileId, FileName,
    FileVersion, Sha1Hex, UploadTarget,
};
pub use storage::{
    DeleteError, DeletedFileVersion, FileNamePage, FileNameQuery, FileNameTraversal, ListedFile,
    Storage,
};
pub use upload::{
    AttemptRecord, Evidence, UploadError, UploadFile, UploadPolicy, UploadReport, Uploaded,
};

use concord::reqwest::{self, Url};

use model::{Token, UploadUrl, base_url, issued_url};
use wire::{B2Wire, B2WireDownload, B2WireStorage};

/// SDK bound for one successful JSON response (1 MiB); not a B2 limit.
pub const JSON_BODY_LIMIT: usize = 1024 * 1024;
/// SDK bound for one B2 error document (64 KiB); not a B2 limit.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// The documented account host, where `b2_authorize_account` goes.
pub const AUTHORIZE_BASE: &str = "https://api.backblazeb2.com";

/// A `reqwest::Client` as this SDK assumes one: no redirects (a status then answers the request
/// that was sent, and a write's delivery is judged on the attempt that made it) and no reqwest
/// retries (every repetition is the upload policy's, counted in its report). Add timeouts and
/// other transport settings to the builder before `build`.
pub fn http_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
}

/// The entry point: an HTTP client and the account host.
#[derive(Clone, Debug)]
pub struct Client {
    wire: B2Wire,
    authorize_base: Url,
}

impl Client {
    /// Uses `http` for every request (see [`http_client_builder`]).
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            wire: B2Wire::new(http),
            authorize_base: Url::parse(AUTHORIZE_BASE).expect("the documented host parses"),
        }
    }

    /// Replaces the account host (scheme, host, port and an optional path prefix), normally for
    /// a local fixture. The application key is sent there.
    ///
    /// # Errors
    ///
    /// [`InputError::BaseUrl`] for anything but an `http`/`https` URL without credentials,
    /// query or fragment.
    pub fn with_base_url(self, base: &str) -> Result<Self, Error> {
        let authorize_base = base_url(base).ok_or(InputError::BaseUrl)?;
        let wire = self
            .wire
            .with_base_url(base)
            .map_err(|_| InputError::BaseUrl)?;
        Ok(Self {
            wire,
            authorize_base,
        })
    }

    /// `b2_authorize_account` with `key`'s Basic credential. The issued `apiUrl` and
    /// `downloadUrl` must be `http`/`https` bases without credentials, query or fragment, and
    /// must not downgrade an `https` account host to `http`; the token must be header text.
    ///
    /// The account token is valid for at most 24 hours; this SDK does not re-authorize on its
    /// own (an `expired_auth_token` answer is returned as [`Error::Provider`]).
    /// Prepares `authorization: Basic <base64(key-id:application-key)>` and, from the
    /// declared `Json` output, `accept: application/json`. This GET has no body or
    /// format-added `content-type`; header names are lowercase.
    ///
    /// # Errors
    ///
    /// [`Error::Provider`] for `401 unauthorized`/`unsupported` and the other documented
    /// answers, [`Error::Concord`] for transport and decoding failures, [`Error::Contract`] for
    /// unusable issued URLs or token.
    pub async fn authorize_account(&self, key: &ApplicationKey) -> Result<Session, Error> {
        let (wire, meta) = self
            .wire
            .account(key.credentials())
            .authorize_account()
            .send()
            .await?;
        let storage = wire.api_info.storage_api;
        let contract = |violation| Error::contract(violation, Some(meta.clone()));
        let api_url = issued_url(&self.authorize_base, &storage.api_url, true).map_err(contract)?;
        let download_url =
            issued_url(&self.authorize_base, &storage.download_url, true).map_err(contract)?;
        let token = Token::issued(wire.authorization_token).map_err(contract)?;
        let mut buckets = Vec::with_capacity(storage.allowed.buckets.len());
        for bucket in storage.allowed.buckets {
            let id = BucketId::new(bucket.id).map_err(|_| contract(Violation::AllowedBucket))?;
            buckets.push(AllowedBucket {
                id,
                name: bucket.name,
            });
        }
        let account = AccountInfo {
            account_id: wire.account_id,
            api_url,
            download_url,
            allowed: Allowed {
                buckets,
                capabilities: storage.allowed.capabilities,
                name_prefix: storage.allowed.name_prefix,
            },
            key_expiration: wire.application_key_expiration_timestamp,
        };
        let bind = |url: &Url| self.wire.clone().with_base_url(url.as_str());
        Ok(Session {
            storage: bind(&account.api_url)?.storage(token.expose()),
            download: bind(&account.download_url)?.download(token.expose()),
            wire: self.wire.clone(),
            account,
        })
    }
}

/// An authorized account: the issued URLs and the account token, bound to their operations.
/// `Debug` withholds the token and the account ID.
#[derive(Clone, concord::Debug)]
pub struct Session {
    account: AccountInfo,
    #[sensitive]
    storage: B2WireStorage,
    #[sensitive]
    download: B2WireDownload,
    /// The unbound client, for binding an upload scope to each issued upload URL.
    #[sensitive]
    wire: B2Wire,
}

impl Session {
    /// Account-authorized operations at the issued API URL.
    pub fn storage(&self) -> Storage<'_> {
        Storage::new(self)
    }

    /// What `b2_authorize_account` returned (without the token).
    pub fn account(&self) -> &AccountInfo {
        &self.account
    }

    /// `b2_get_upload_url` for `bucket`: an upload URL and its own upload token, at the issued
    /// `apiUrl`. The URL must not downgrade the `apiUrl`'s `https` and must carry no
    /// credentials or fragment (its path and query are kept as issued).
    /// Sends the account token verbatim as `authorization`, with `accept: application/json`
    /// from the declared `Json` output. This GET adds no `content-type`.
    ///
    /// # Errors
    ///
    /// [`Error::Provider`] (`401 expired_auth_token` means the *account* token expired:
    /// authorize again), [`Error::Concord`], [`Error::Contract`].
    pub async fn get_upload_url(&self, bucket: &BucketId) -> Result<UploadTarget, Error> {
        let current = self.issue_upload_url(bucket).await?;
        Ok(UploadTarget {
            bucket: bucket.clone(),
            current: Some(current),
        })
    }

    pub(crate) async fn issue_upload_url(&self, bucket: &BucketId) -> Result<UploadUrl, Error> {
        let (wire, meta) = self.storage.get_upload_url(bucket.as_str()).send().await?;
        let contract = |violation| Error::contract(violation, Some(meta.clone()));
        if wire.bucket_id != bucket.as_str() {
            return Err(contract(Violation::UploadUrlBucket));
        }
        let url = issued_url(&self.account.api_url, &wire.upload_url, false).map_err(contract)?;
        let token = Token::issued(wire.authorization_token).map_err(contract)?;
        Ok(UploadUrl { url, token })
    }
}
