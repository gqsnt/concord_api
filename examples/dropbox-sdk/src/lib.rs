#![doc = include_str!("../README.md")]

mod context;
mod error;
mod models;
mod wire;

pub use concord_pagination as pagination;
pub use context::{InputError, NamespaceId, PathRoot, RequestContext};
pub use error::{
    Error, ErrorBody, HttpError, LookupError, ProviderError, ProviderErrorKind, RateLimitError,
    RateLimitReason, ResponseMeta, RetryAfter, RootInfo, TaggedError, UserMessage,
};
pub use models::{
    Cursor, Entry, EntryPath, FileMetadata, FolderMetadata, FolderPage, ListFolderOptions, PageSize,
};
pub use wire::DropboxApiConfig as ClientConfig;

use concord::reqwest;
use pagination::{Page, Pages, Resume};
use std::num::NonZeroUsize;
use wire::{ContinueBody, DropboxApi, WirePage};

/// SDK byte limit for a single successful JSON response (16 MiB), not a Dropbox guarantee.
pub const SUCCESS_BODY_LIMIT: usize = 16 * 1024 * 1024;
/// SDK byte limit for an HTTP error document (64 KiB).
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// A successful domain value with its response metadata.
#[derive(Debug)]
pub struct Response<T> {
    /// Fully decoded value.
    pub value: T,
    /// HTTP response metadata, including request ID and declared operation identity.
    pub meta: ResponseMeta,
}

/// Per-page metadata, including the last cursor even on provider exhaustion.
#[derive(Debug)]
pub struct FolderPageMeta {
    /// Status, headers, request ID and operation.
    pub http: ResponseMeta,
    /// Context-bound provider cursor for this page.
    pub cursor: Cursor,
    /// Provider's completion indicator.
    pub has_more: bool,
}

/// Lazy page traversal. `Ok(None)` means provider exhaustion; a bound or failure is an error.
/// Item traversal/streaming is available through `items()`; it discards per-page metadata.
pub type FolderPages = Pages<Entry, Cursor, FolderPageMeta, Error>;

/// Sends `authorization: Bearer <token>` from `ClientConfig::token`. Listing wrappers send
/// `dropbox-api-select-user` from the operation's member ID and `dropbox-api-path-root`
/// from `serde_json::to_string` of the selected `PathRoot`.
///
/// Shared transport and bearer credentials. Member/root selections are owned per operation.
#[derive(Clone, Debug)]
pub struct Client {
    api: DropboxApi,
}

impl Client {
    /// Construct with a caller-configured transport and raw bearer token.
    /// Use timeouts and `reqwest::redirect::Policy::none()` as shown in the crate example.
    pub fn new(http: reqwest::Client, config: ClientConfig) -> Self {
        Self {
            api: DropboxApi::new(http, config),
        }
    }

    /// Replace the trusted RPC base (including `/2`), normally for loopback fixtures.
    /// This destination receives the bearer token. Invalid URLs fail before any I/O.
    pub fn with_base_url(self, base: impl AsRef<str>) -> Result<Self, concord::Error> {
        Ok(Self {
            api: self.api.with_base_url(base)?,
        })
    }

    /// Fetch the first page in an explicitly selected member/root context.
    /// The owned context is captured by this operation and by its returned cursor.
    pub async fn list_folder(
        &self,
        context: RequestContext,
        options: ListFolderOptions,
    ) -> Result<Response<FolderPage>, Error> {
        let response = self
            .api
            .selected(context.member_id(), context.path_root().header())
            .list_folder(options)
            .send()
            .await?;
        bind_page(response, context)
    }

    /// Fetch the next page using the cursor's original member/root. Only a cursor is sent
    /// in the JSON body; starting options are already represented by Dropbox's cursor.
    /// Use the same token/app that started the listing. HTTP 409/reset requires an explicit restart.
    pub async fn list_folder_continue(
        &self,
        cursor: &Cursor,
    ) -> Result<Response<FolderPage>, Error> {
        let context = cursor.context.clone();
        let response = self
            .api
            .selected(context.member_id(), context.path_root().header())
            .list_folder_continue(ContinueBody {
                cursor: cursor.value.clone(),
            })
            .send()
            .await?;
        bind_page(response, context)
    }

    /// Lazily list all available pages with a default safety cap of 1000 requests.
    /// Override with `max_pages(NonZeroUsize)` on the returned traversal. A cap is an explicit
    /// incomplete outcome, never successful exhaustion. No request occurs until polled.
    pub fn list_folder_pages(
        &self,
        context: RequestContext,
        options: ListFolderOptions,
    ) -> FolderPages {
        let client = self.clone();
        Pages::new(Resume::Start, move |cursor: Option<Cursor>| {
            let client = client.clone();
            let context = context.clone();
            let options = options.clone();
            async move {
                let response = match cursor {
                    Some(cursor) => client.list_folder_continue(&cursor).await?,
                    None => client.list_folder(context, options).await?,
                };
                let page = response.value;
                Ok(Page {
                    items: page.entries,
                    next: page.has_more.then(|| page.cursor.clone()),
                    meta: FolderPageMeta {
                        http: response.meta,
                        cursor: page.cursor,
                        has_more: page.has_more,
                    },
                })
            }
        })
        .max_pages(NonZeroUsize::new(1000).expect("nonzero constant"))
    }
}

fn bind_page(
    response: Response<WirePage>,
    context: RequestContext,
) -> Result<Response<FolderPage>, Error> {
    if response.value.cursor.is_empty() {
        return Err(Error::EmptyCursor {
            meta: Box::new(response.meta),
        });
    }
    Ok(Response {
        value: FolderPage {
            entries: response.value.entries,
            cursor: Cursor {
                value: response.value.cursor,
                context,
            },
            has_more: response.value.has_more,
        },
        meta: response.meta,
    })
}
