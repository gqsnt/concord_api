//! The Concord declaration of the slice, and the client's construction.
//!
//! Every operation is an ordinary generated endpoint: its setters, `prepare()` and `send()` (which
//! returns `concord::Error`) stay available, and [`Call::call`](crate::Call::call) sends it with
//! this SDK's error and metadata. Shared configuration lives where GitLab applies it: the token on
//! the client, `projects/:id` on the project scope.
//!
//! What the declaration deliberately does not have:
//!
//! - **No `page` parameter.** The issue listing is paginated by GitLab's `Link` header; later
//!   pages are requested at the URLs GitLab sends ([`GitLabProjectIssues::pages`]), never built
//!   from a page number.
//! - **No `pagination=keyset`, `cursor` or keyset-only `order_by` values.** The SDK pins
//!   offset-based pagination for this endpoint (see the crate documentation).

use concord::Json;
use concord::reqwest::{
    self,
    header::{HeaderName, HeaderValue},
};

use crate::models::{Issue, Project};
use crate::params::{
    IssueIid, IssueOrder, IssueState, Labels, PerPage, PrivateToken, ProjectRef, SortOrder,
};

/// `PRIVATE-TOKEN` (HTTP/1.1 sends header names in lowercase; GitLab reads them
/// case-insensitively).
pub const PRIVATE_TOKEN: HeaderName = HeaderName::from_static("private-token");

/// GitLab.com's REST API v4 base. A self-managed instance is `https://<host>/api/v4`, set with
/// [`GitLab::with_base_url`].
pub const GITLAB_COM: &str = "https://gitlab.com/api/v4";

concord::api! {
    /// A slice of the GitLab REST API v4: project issues and the project itself.
    ///
    /// Construct it with [`GitLab::with_token`]; a self-managed instance with
    /// `.with_base_url("https://gitlab.example.com/api/v4")?`.
    pub client GitLab(
        /// A personal, project or group access token, sent as `PRIVATE-TOKEN`. Never printed by
        /// Concord. Build it with [`PrivateToken`].
        #[sensitive] header(PRIVATE_TOKEN) token: HeaderValue,
    ) {
        base "https://gitlab.com/api/v4";
        format Json;

        /// One project: `projects/:id`, the ID or the full path sent as one URL-encoded segment.
        scope project(id: ProjectRef) {
            path "projects/{id}";

            /// `GET /projects/:id` (Projects API, "Retrieve a project").
            GET get() -> Project;

            /// `GET /projects/:id/issues` (Issues API, "List project issues"): one page. GitLab's
            /// defaults: every state, `order_by=created_at`, `sort=desc`, 20 per page. For every
            /// page, use [`GitLabProjectIssues::pages`].
            GET issues(
                /// `opened`, `closed` or `all`.
                query state?: IssueState,
                /// Issues with all of these labels, or `Any` / `None`.
                query labels?: Labels,
                /// A milestone title (also `None`, `Any`, `Upcoming`, `Started`).
                query milestone?: String,
                /// Searches title and description.
                query search?: String,
                /// The author's username.
                query author_username?: String,
                /// Sort key (offset-pagination values).
                query order_by?: IssueOrder,
                /// `asc` or `desc`.
                query sort?: SortOrder,
                /// The first page's size, 1–100.
                query per_page?: PerPage,
            ) -> Vec<Issue> {
                path "issues";
            }

            /// `GET /projects/:id/issues/:issue_iid` (Issues API, "Retrieve a project issue"): by
            /// the project-scoped number, not the global ID.
            GET issue(iid: IssueIid) -> Issue {
                path "issues/{iid}";
            }
        }
    }
}

impl GitLab {
    /// A client for GitLab.com with `token`, over your `reqwest::Client` (timeouts, proxies,
    /// TLS; see [`redirect_policy`](crate::redirect_policy) for redirects).
    ///
    /// ```
    /// use concord_sdk_gitlab::{GitLab, PrivateToken};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let http = concord::reqwest::Client::builder()
    ///     .redirect(concord_sdk_gitlab::redirect_policy())
    ///     .build()?;
    /// let token = PrivateToken::new("synthetic-token-for-docs")?;
    /// let gitlab = GitLab::with_token(http, token).with_base_url("https://gitlab.example.com/api/v4")?;
    /// assert!(!format!("{gitlab:?}").contains("synthetic-token-for-docs"));
    /// # Ok(()) }
    /// ```
    pub fn with_token(http: reqwest::Client, token: PrivateToken) -> Self {
        Self::new(
            http,
            GitLabConfig {
                token: token.into_header_value(),
            },
        )
    }
}
