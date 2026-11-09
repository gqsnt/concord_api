use std::{fmt, num::NonZeroUsize, time::Duration};

use concord::reqwest::{self, Url, header::HeaderValue};
use concord::{Prepared, Received};
use concord_pagination::{Page, Pages, Resume};

use crate::api::{DriveApi, DriveApiConfig};
use crate::{
    ChangesOptions, ChangesPage, ChangesPageMeta, Error, File, FileFields, FilesPage, ListOptions,
    NewFile, PageToken, Response, ResponseMeta, StartPageToken,
};

/// Transport/context settings. Origins must be HTTPS, or HTTP on a literal loopback IP.
#[derive(Clone, Debug)]
pub struct ClientConfig {
    /// API origin only; no path, query, user info, or fragment. Defaults to Google's origin.
    pub origin: String,
    /// Timeout for a single request, including its response body.
    pub timeout: Duration,
    /// Maximum bytes for any finite metadata response (default 2 MiB).
    pub max_response_bytes: usize,
    /// Maximum bytes read from an error response (default 64 KiB).
    pub max_error_bytes: usize,
    /// Maximum bytes accepted by a single resumable chunk (default 8 MiB).
    pub max_chunk_bytes: usize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            origin: "https://www.googleapis.com".into(),
            timeout: Duration::from_secs(30),
            max_response_bytes: 2 * 1024 * 1024,
            max_error_bytes: 64 * 1024,
            max_chunk_bytes: 8 * 1024 * 1024,
        }
    }
}

/// Sends `authorization: Bearer <token>` from the constructor's raw bearer token.
/// Upload-specific header sources are documented on the upload wrappers.
///
/// Public developer entry point. OAuth tokens are injected; no token minting or refreshing.
///
/// The client builds a transport with redirects and automatic retries disabled. Credentials
/// are bound to the configured origin. Clone to share the connection pool.
#[derive(Clone)]
pub struct DriveClient {
    pub(crate) api: DriveApi,
    pub(crate) origin: Url,
    pub(crate) config: ClientConfig,
}

/// File-page traversal. Bounds and failures retain resumable positions and accepted prefixes.
pub type FilePages = Pages<File, PageToken, ResponseMeta, Error>;
/// Change-page traversal; final-page metadata retains the next polling interval's token.
pub type ChangesPages = Pages<crate::Change, PageToken, ChangesPageMeta, Error>;

impl fmt::Debug for DriveClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DriveClient")
            .field("api", &self.api)
            .finish_non_exhaustive()
    }
}

impl DriveClient {
    /// Construct for Google with bounded default transport settings.
    pub fn new(bearer_token: impl Into<String>) -> Result<Self, Error> {
        Self::with_config(bearer_token, ClientConfig::default())
    }

    /// Construct with an explicit origin (including offline loopback) and resource limits.
    /// The token contains only the token value, without the `Bearer ` prefix.
    pub fn with_config(
        bearer_token: impl Into<String>,
        config: ClientConfig,
    ) -> Result<Self, Error> {
        let token = bearer_token.into();
        if token.is_empty()
            || token.bytes().any(|b| !b.is_ascii_graphic())
            || HeaderValue::try_from(format!("Bearer {token}")).is_err()
        {
            return Err(Error::invalid(
                "bearer token must be nonempty visible ASCII without whitespace",
            ));
        }
        let origin =
            Url::parse(&config.origin).map_err(|_| Error::invalid("invalid API origin"))?;
        let loopback = origin
            .host_str()
            .and_then(|h| h.trim_matches(['[', ']']).parse::<std::net::IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback());
        if origin.host_str().is_none()
            || !(origin.scheme() == "https" || (origin.scheme() == "http" && loopback))
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
        {
            return Err(Error::invalid(
                "API origin must be HTTPS or literal loopback HTTP, without credentials, path, query or fragment",
            ));
        }
        if config.timeout.is_zero()
            || config.max_response_bytes == 0
            || config.max_error_bytes == 0
            || config.max_chunk_bytes < crate::CHUNK_ALIGNMENT
        {
            return Err(Error::invalid(
                "timeouts and response limits must be positive; chunk bound must be at least 256 KiB",
            ));
        }
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(config.timeout)
            .build()
            .map_err(|_| Error::invalid("could not construct HTTP transport"))?;
        let api = DriveApi::new(http, DriveApiConfig { token }).with_base_url(origin.as_str())?;
        Ok(Self {
            api,
            origin,
            config,
        })
    }

    pub(crate) async fn receive<T>(&self, prepared: Prepared<T>) -> Result<Received<T>, Error> {
        Ok(prepared.receive().await?)
    }

    pub(crate) async fn finish<T>(&self, received: Received<T>) -> Result<Response<T>, Error> {
        if !received.status().is_success() {
            return Err(
                Error::provider_response(received, self.config.max_error_bytes, false).await,
            );
        }
        let (value, meta) = received
            .decode_limited_with_meta(self.config.max_response_bytes)
            .await?;
        Ok(Response { value, meta })
    }

    /// Fetch one files page, preserving a partial search indication for inspection.
    pub async fn list_files_page(
        &self,
        options: &ListOptions,
        token: Option<&PageToken>,
    ) -> Result<Response<FilesPage>, Error> {
        validate_page_size(options.page_size)?;
        let fields = format!(
            "nextPageToken,incompleteSearch,files({})",
            options.fields.wire()
        );
        let prepared = self
            .api
            .files()
            .list(options.page_size, fields)
            .maybe_page_token(token.map(|t| t.as_str().to_owned()))
            .maybe_q(options.query.clone())
            .maybe_order_by(options.order_by.clone())
            .prepare()?;
        let response = self.finish(self.receive(prepared).await?).await?;
        validate_next(
            token,
            response.value.next_page_token.as_ref(),
            &response.meta,
        )?;
        Ok(response)
    }

    /// Traverse files within a required page budget. An incomplete search is a typed error,
    /// rather than successful exhaustion. Options and origin must match a restored cursor.
    pub fn files(
        &self,
        options: ListOptions,
        from: Resume<PageToken>,
        max_pages: NonZeroUsize,
    ) -> FilePages {
        let client = self.clone();
        Pages::new(from, move |token: Option<PageToken>| {
            let client = client.clone();
            let options = options.clone();
            async move {
                let response = client.list_files_page(&options, token.as_ref()).await?;
                if response.value.incomplete_search {
                    return Err(Error::protocol(
                        "Drive reported an incomplete search",
                        Some(response.meta),
                    ));
                }
                Ok(Page {
                    items: response.value.files,
                    next: response.value.next_page_token,
                    meta: response.meta,
                })
            }
        })
        .max_pages(max_pages)
    }

    /// Retrieve selected metadata; this operation never requests media/download content.
    pub async fn get_file(&self, id: &str, fields: &FileFields) -> Result<Response<File>, Error> {
        if id.is_empty() || id == "." || id == ".." {
            return Err(Error::invalid("file id must be a nonempty non-dot segment"));
        }
        let prepared = self
            .api
            .files()
            .get(id.to_owned(), fields.wire())
            .prepare()?;
        self.finish(self.receive(prepared).await?).await
    }

    /// Create a metadata-only file, such as a folder.
    pub async fn create_file(
        &self,
        metadata: NewFile,
        fields: &FileFields,
    ) -> Result<Response<File>, Error> {
        let prepared = self.api.files().create(metadata, fields.wire()).prepare()?;
        self.finish(self.receive(prepared).await?).await
    }

    /// Obtain the initial change-feed boundary. Tokens are opaque and retained without parsing.
    pub async fn start_page_token(&self) -> Result<Response<StartPageToken>, Error> {
        self.finish(self.receive(self.api.changes().start().prepare()?).await?)
            .await
    }

    /// Fetch one changes page. Exactly one of next-page/new-start-page must be present.
    pub async fn list_changes_page(
        &self,
        token: &PageToken,
        options: &ChangesOptions,
    ) -> Result<Response<ChangesPage>, Error> {
        validate_page_size(options.page_size)?;
        let fields = format!(
            "nextPageToken,newStartPageToken,changes(fileId,removed,time,file({}))",
            options.fields.wire()
        );
        let prepared = self
            .api
            .changes()
            .list(token.as_str().to_owned(), options.page_size, fields)
            .prepare()?;
        let response = self.finish(self.receive(prepared).await?).await?;
        if response.value.next_page_token.is_some() == response.value.new_start_page_token.is_some()
        {
            return Err(Error::protocol(
                "changes page must have exactly one boundary token",
                Some(response.meta),
            ));
        }
        validate_next(
            Some(token),
            response.value.next_page_token.as_ref(),
            &response.meta,
        )?;
        Ok(response)
    }

    /// Traverse a finite changes batch. Process pages and save final-page `new_start_page_token`
    /// after committing those changes. `item_events()` yields final metadata on completion;
    /// `items()` intentionally drops page metadata.
    pub fn changes(
        &self,
        start: PageToken,
        options: ChangesOptions,
        max_pages: NonZeroUsize,
    ) -> ChangesPages {
        let client = self.clone();
        Pages::new(Resume::At(start), move |token: Option<PageToken>| {
            let client = client.clone();
            let options = options.clone();
            async move {
                let token =
                    token.ok_or_else(|| Error::invalid("changes traversal needs a start token"))?;
                let response = client.list_changes_page(&token, &options).await?;
                Ok(Page {
                    items: response.value.changes,
                    next: response.value.next_page_token,
                    meta: ChangesPageMeta {
                        response: response.meta,
                        new_start_page_token: response.value.new_start_page_token,
                    },
                })
            }
        })
        .max_pages(max_pages)
    }
}

fn validate_page_size(size: u16) -> Result<(), Error> {
    if !(1..=1000).contains(&size) {
        Err(Error::invalid("page size must be 1–1000"))
    } else {
        Ok(())
    }
}

fn validate_next(
    current: Option<&PageToken>,
    next: Option<&PageToken>,
    meta: &ResponseMeta,
) -> Result<(), Error> {
    if next.is_some() && current == next {
        Err(Error::protocol(
            "page token did not advance",
            Some(meta.clone()),
        ))
    } else {
        Ok(())
    }
}
