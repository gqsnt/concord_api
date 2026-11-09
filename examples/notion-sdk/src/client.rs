use crate::api::{Notion, NotionConfig};
use crate::models::{Block, List, Page, QueryResult};
use crate::{ERROR_LIMIT, EnvelopeProblem, Error, PageSize, ProviderError, Query};
use concord::{Prepared, ResponseMeta, reqwest};
use concord_pagination::{Page as TraversalPage, Pages, Resume};
use std::collections::HashSet;
use std::fmt;

/// A decoded value and the actual response metadata. Debug omits the value.
pub struct Response<T> {
    /// Decoded provider payload.
    pub value: T,
    /// HTTP status, version, headers and generated operation identity.
    pub meta: ResponseMeta,
}
impl<T> fmt::Debug for Response<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("meta", &self.meta)
            .finish_non_exhaustive()
    }
}

/// Lazy query pages; `.items()` and `.into_stream()` retain typed incomplete outcomes.
pub type QueryPages = Pages<QueryResult, String, ResponseMeta, Error>;
/// Lazy block-child pages. Each traversal covers one parent, never its descendants.
pub type BlockPages = Pages<Block, String, ResponseMeta, Error>;

/// Sends `authorization: Bearer <token>` from the raw constructor token and `notion-version`
/// from `NOTION_VERSION`; `with_version` replaces that header with its argument verbatim.
///
/// Read-only Notion client. The caller supplies a transport and bearer token.
///
/// Transport timeout/proxy/TLS settings belong to the supplied reqwest client. Disable
/// redirects on credential-bearing transports. Clones have independent version/base context
/// and share only reqwest's connection pool. No retries or background work are performed.
#[derive(Clone, Debug)]
pub struct NotionClient {
    api: Notion,
}

impl NotionClient {
    /// Constructs a client pinned to [`crate::NOTION_VERSION`].
    ///
    /// ```
    /// use concord_sdk_notion::NotionClient;
    /// use concord::reqwest;
    /// let http = reqwest::Client::builder()
    ///     .redirect(reqwest::redirect::Policy::none())
    ///     .timeout(std::time::Duration::from_secs(20)).build().unwrap();
    /// let client = NotionClient::new(http, "synthetic-notion-token");
    /// assert!(!format!("{client:?}").contains("synthetic-notion-token"));
    /// ```
    pub fn new(http: reqwest::Client, token: impl Into<String>) -> Self {
        Self {
            api: Notion::new(
                http,
                NotionConfig {
                    token: token.into(),
                },
            ),
        }
    }

    /// Overrides the API base including `/v1`, normally only for a loopback fixture server.
    pub fn with_base_url(mut self, base: &str) -> Result<Self, concord::Error> {
        self.api = self.api.with_base_url(base)?;
        Ok(self)
    }

    /// Explicitly changes this owned client's version header. No automatic schema migration
    /// is attempted; only the pinned version has been fixture-tested.
    /// Existing clones and lazy traversals keep the context captured when they were created,
    /// including for later pages. Response version headers do not change that context.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.api = self.api.set_version(version.into());
        self
    }

    /// Access to the public generated operations and Concord's preparation/receive boundary.
    /// Direct `.send()` returns Concord errors rather than SDK provider errors.
    pub fn api(&self) -> &Notion {
        &self.api
    }

    /// Retrieves one page's properties. Content and complete paginated properties are separate.
    pub async fn retrieve_page(&self, page_id: impl Into<String>) -> Result<Response<Page>, Error> {
        receive(self.api.page(page_id.into()).prepare()?).await
    }

    /// Executes one query page, preserving the filter and placing the cursor in the POST body.
    /// Capped or unfamiliar request_status responses return [`Error::IncompleteQuery`].
    pub async fn query_data_source(
        &self,
        data_source_id: impl Into<String>,
        query: &Query,
        cursor: Option<String>,
    ) -> Result<Response<List<QueryResult>>, Error> {
        let response = receive(
            self.api
                .query(data_source_id.into(), query.body(cursor.clone()))
                .prepare()?,
        )
        .await?;
        if response
            .value
            .request_status
            .as_ref()
            .is_some_and(|status| status.kind != "complete")
        {
            return Err(Error::IncompleteQuery {
                response: Box::new(response),
            });
        }
        validate(&response, cursor.as_deref())?;
        Ok(response)
    }

    /// Executes one children page for a page or block ID. Descendants require separate calls.
    pub async fn list_block_children(
        &self,
        block_id: impl Into<String>,
        size: PageSize,
        cursor: Option<String>,
    ) -> Result<Response<List<Block>>, Error> {
        let mut op = self.api.children(block_id.into(), size.get());
        if let Some(cursor) = &cursor {
            op = op.set_start_cursor(cursor.clone());
        }
        let response = receive(op.prepare()?).await?;
        validate(&response, cursor.as_deref())?;
        Ok(response)
    }

    /// Starts a lazy query traversal. The query is owned so each page reuses the same filter.
    /// Set `.max_pages(NonZeroUsize)` to bound it; a cap is an error, not exhaustion.
    pub fn query_pages(&self, data_source_id: impl Into<String>, query: Query) -> QueryPages {
        self.query_pages_from(data_source_id, query, Resume::Start)
    }

    /// Resumes a query at a page checkpoint. The caller must reuse the same data source,
    /// version, authorization, filter and page size. A checkpoint does not imply a snapshot. Use `concord_sdk_notion::pagination::Resume` for the position. `Resume::Start` repeats the data-source query without `start_cursor`; `Resume::At(key)` takes the `String` from `next_cursor` in a returned page's `next`.
    pub fn query_pages_from(
        &self,
        data_source_id: impl Into<String>,
        query: Query,
        from: Resume<String>,
    ) -> QueryPages {
        let client = self.clone();
        let id = data_source_id.into();
        let mut seen = HashSet::new();
        Pages::new(from, move |cursor: Option<String>| {
            let repeated = cursor.as_ref().is_some_and(|key| !seen.insert(key.clone()));
            let (client, id, query) = (client.clone(), id.clone(), query.clone());
            async move {
                if repeated {
                    return Err(Error::CursorCycle);
                }
                let response = client.query_data_source(id, &query, cursor).await?;
                Ok(project(response))
            }
        })
    }

    /// Starts a lazy traversal of all children at one tree level. Empty pages with cursors
    /// continue. Unknown block types remain items and can themselves have children.
    pub fn children(&self, block_id: impl Into<String>, size: PageSize) -> BlockPages {
        self.children_from(block_id, size, Resume::Start)
    }

    /// Resumes children at a page checkpoint with the same parent, version, authorization
    /// and page size. Item checkpoints also contain an offset; use a page checkpoint here. Use `concord_sdk_notion::pagination::Resume` for the position. `Resume::Start` repeats the parent block's first children page without `start_cursor`; `Resume::At(key)` takes the `String` from `next_cursor` in a returned page's `next`.
    pub fn children_from(
        &self,
        block_id: impl Into<String>,
        size: PageSize,
        from: Resume<String>,
    ) -> BlockPages {
        let client = self.clone();
        let id = block_id.into();
        let mut seen = HashSet::new();
        Pages::new(from, move |cursor: Option<String>| {
            let repeated = cursor.as_ref().is_some_and(|key| !seen.insert(key.clone()));
            let (client, id) = (client.clone(), id.clone());
            async move {
                if repeated {
                    return Err(Error::CursorCycle);
                }
                let response = client.list_block_children(id, size, cursor).await?;
                Ok(project(response))
            }
        })
    }
}

fn project<T>(response: Response<List<T>>) -> TraversalPage<T, String, ResponseMeta> {
    TraversalPage {
        items: response.value.results,
        next: response.value.next_cursor,
        meta: response.meta,
    }
}

fn validate<T>(response: &Response<List<T>>, cursor: Option<&str>) -> Result<(), Error> {
    let list = &response.value;
    let problem = if list.has_more != list.next_cursor.is_some() {
        Some(EnvelopeProblem::CursorMismatch)
    } else if list.next_cursor.as_deref().is_some_and(str::is_empty) {
        Some(EnvelopeProblem::EmptyCursor)
    } else if cursor.is_some() && list.next_cursor.as_deref() == cursor {
        Some(EnvelopeProblem::CursorDidNotAdvance)
    } else {
        None
    };
    match problem {
        Some(problem) => Err(Error::Envelope {
            problem,
            meta: Box::new(response.meta.clone()),
        }),
        None => Ok(()),
    }
}

async fn receive<T>(prepared: Prepared<T>) -> Result<Response<T>, Error> {
    let received = prepared.receive().await?;
    if received.status().is_success() {
        let (value, meta) = received.decode_with_meta().await?;
        return Ok(Response { value, meta });
    }
    // Keep the raw bytes even when JSON decoding succeeds but the provider envelope is
    // inconsistent with the head; `document` retains bytes only after a decode failure.
    let meta = received.meta();
    let body = received.bytes_limited(ERROR_LIMIT).await?;
    match serde_json::from_slice::<ProviderError>(&body) {
        Ok(error) if error.object == "error" && error.status == meta.status().as_u16() => {
            Err(Error::Provider {
                error: Box::new(error),
                meta: Box::new(meta),
            })
        }
        _ => Err(Error::InvalidErrorBody {
            meta: Box::new(meta),
            body,
        }),
    }
}
