//! The configured SDK: one [`GitHubContext`] (transport, base URL, token, API version,
//! `User-Agent`) and per-repository [`RepoContext`]s.
//!
//! A context yields the **generated** handles for the declared operations
//! ([`RepoContext::handle`], [`RepoContext::actions`]) and, from the same values, native requests
//! for operations this SDK does not declare ([`RepoContext::request`]). Both carry the same
//! service headers, so another crate can add an operation without copying the credentials or
//! the version pin, and without editing this SDK. Like Concord handles, contexts are immutable
//! snapshots: a setter returns a new context, and cloning is a reference-count increment.
//!
//! ```
//! use concord_example_github_sdk::{GitHubConfig, GitHubContext, RepoRef};
//! use concord_example_github_sdk::params::{Labels, StateFilter};
//! use concord_example_github_sdk::filters::IssueFilter;
//! use concord::reqwest::{self, Method};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let github = GitHubContext::new(reqwest::Client::new(), GitHubConfig { origin: concord::BaseUrl::from_static("https://api.github.com"), token: "t-1".into() });
//! let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
//!
//! // A declared operation, through the generated handle (setters, prepare, send stay available):
//! let filter = IssueFilter {
//!     state: Some(StateFilter::All),
//!     labels: Some(Labels::new(["bug", "ui"])?),
//!     ..IssueFilter::default()
//! };
//! let issues = repo.issues(&filter).prepare()?;
//! assert_eq!(
//!     issues.request().url().as_str(),
//!     "https://api.github.com/repos/octo-org/hello-world/issues?state=all&labels=bug%2Cui",
//! );
//! // An operation the SDK does not declare, from the same values:
//! let events = repo.request(Method::GET, &["issues", "events"])?;
//! assert_eq!(events.url().as_str(), "https://api.github.com/repos/octo-org/hello-world/issues/events");
//! assert_eq!(events.headers()["x-github-api-version"], "2026-03-10");
//! assert!(events.headers()["authorization"].is_sensitive());
//! # Ok(()) }
//! ```
//!
//! # Redirects and the token
//!
//! GitHub asks clients to follow redirects. reqwest follows them by default and strips
//! `Authorization` when the host changes, but forwards every other header, and the redirect
//! policy is a property of the whole `reqwest::Client` (there is no per-request policy). The
//! artifact download must see GitHub's `302` itself (to report it and to fetch the signed URL
//! without any GitHub header), so build the client with [`redirect_policy`], which follows
//! same-origin redirects (a renamed repository's `301`) and stops at a cross-origin one. A
//! client with `redirect::Policy::none()` works too. With reqwest's default policy, the download
//! refuses the already-followed response ([`crate::download::LocateError::RedirectFollowed`]).

use std::fmt;

use concord::reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use concord::reqwest::{self, Method, Url, redirect};

use crate::api::{
    GitHub, GitHubConfig, GitHubRepo, GitHubRepoActions, GitHubRepoActionsRuns, GitHubRepoIssues,
    X_GITHUB_API_VERSION,
};
use crate::filters::{IssueFilter, RunFilter};
use crate::ids::RepoRef;
use crate::params::ApiVersion;

/// A redirect policy for the `reqwest::Client` given to a [`GitHubContext`]: follow at most 10
/// redirects that stay on the origin (scheme, host, port) of the original request, and stop at
/// the first one that leaves it, returning that `3xx` response to the caller.
pub fn redirect_policy() -> redirect::Policy {
    redirect::Policy::custom(|attempt| {
        let leaves = attempt
            .previous()
            .first()
            .is_some_and(|origin| !same_origin(origin, attempt.url()));
        if leaves {
            attempt.stop()
        } else if attempt.previous().len() > 10 {
            attempt.error("too many redirects")
        } else {
            attempt.follow()
        }
    })
}

/// Same scheme, host and port (default ports made explicit).
pub(crate) fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

/// Generated and native requests use `authorization: Bearer <token>`,
/// `accept: application/vnd.github+json`, `x-github-api-version` from `ApiVersion::as_str()`,
/// and `user-agent` from the context (initially `DEFAULT_USER_AGENT`).
///
/// The GitHub configuration shared by the generated handles and native requests. See the
/// [module documentation](self).
#[derive(Clone)]
pub struct GitHubContext {
    client: GitHub,
}

impl GitHubContext {
    /// The context of `http`, the checked destination and token in `config`, with the pinned API
    /// version and the default `User-Agent` — the same arguments as [`GitHub::new`].
    pub fn new(http: reqwest::Client, config: GitHubConfig) -> Self {
        Self {
            client: GitHub::new(http, config),
        }
    }

    /// Replaces the base URL (GitHub Enterprise Server's `https://HOST/api/v3`, a test server).
    /// Validated by Concord's own [`concord::BaseUrl::parse`]; its path is kept as a prefix.
    ///
    /// # Errors
    ///
    /// The Concord `Build` error of [`concord::BaseUrl::parse`].
    pub fn with_base_url(mut self, url: impl AsRef<str>) -> Result<Self, concord::Error> {
        let url = url.as_ref();
        self.client = GitHub::new(
            self.client.http().clone(),
            GitHubConfig {
                origin: concord::BaseUrl::parse(url)?,
                token: self.client.token().clone(),
            },
        )
        .set_api_version(self.client.api_version().clone())
        .set_user_agent(self.client.user_agent().clone());
        Ok(self)
    }

    /// Sends another `X-GitHub-Api-Version`. The models were checked against
    /// [`ApiVersion::pinned`] only.
    pub fn set_api_version(mut self, version: ApiVersion) -> Self {
        self.client = self.client.clone().set_api_version(version.clone());
        self
    }

    /// Sends another `User-Agent` (GitHub asks for the application's name or the user's login).
    pub fn set_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        let user_agent = user_agent.into();
        self.client = self.client.clone().set_user_agent(user_agent.as_str());
        self
    }

    /// The generated client handle.
    pub fn client(&self) -> &GitHub {
        &self.client
    }

    /// The context of one repository.
    pub fn repo(&self, repo: RepoRef) -> RepoContext {
        let handle = self.client.repo(repo.owner, repo.name);
        let actions = handle.actions();
        RepoContext {
            context: self.clone(),
            handle,
            actions,
        }
    }

    /// The `reqwest::Client`.
    pub fn http(&self) -> &reqwest::Client {
        self.client.http()
    }

    /// The base URL.
    pub fn base_url(&self) -> &Url {
        self.client.base_url()
    }

    /// The API version sent.
    pub fn api_version(&self) -> &ApiVersion {
        self.client.api_version()
    }

    /// Whether `url` belongs to this API: the base's origin, and a path under the base path. Only
    /// such URLs receive the token (pagination links are checked with it).
    pub fn is_api_url(&self, url: &Url) -> bool {
        let base = self.client.base_url();
        if !same_origin(base, url) {
            return false;
        }
        let prefix = base.path().trim_end_matches('/');
        let path = url.path();
        prefix.is_empty() || path == prefix || path.starts_with(&format!("{prefix}/"))
    }

    /// A native request for `method` and base-relative `segments`, with the service headers.
    ///
    /// # Errors
    ///
    /// [`ContextError`]; nothing is sent.
    pub fn request(
        &self,
        method: Method,
        segments: &[&str],
    ) -> Result<reqwest::Request, ContextError> {
        self.build(method, None, segments)
    }

    fn build(
        &self,
        method: Method,
        repo: Option<(&str, &str)>,
        segments: &[&str],
    ) -> Result<reqwest::Request, ContextError> {
        let mut url = self.client.base_url().clone();
        let base_path = self.client.base_url().path();
        let mut path = base_path.strip_suffix('/').unwrap_or(base_path).to_owned();
        if let Some((owner, name)) = repo {
            if is_dot_or_empty(owner) || is_dot_or_empty(name) {
                return Err(ContextError::Repository);
            }
            push_segment(&mut path, "repos");
            push_segment(&mut path, owner);
            push_segment(&mut path, name);
        }
        for (index, segment) in segments.iter().enumerate() {
            if is_dot_or_empty(segment) {
                return Err(ContextError::Segment { index });
            }
            push_segment(&mut path, segment);
        }
        if repo.is_some() || !segments.is_empty() {
            url.set_path(&path);
        }
        let mut headers = HeaderMap::new();
        let mut authorization = HeaderValue::from_str(&format!("Bearer {}", self.client.token()))
            .map_err(|_| ContextError::HeaderValue {
            name: "authorization",
        })?;
        authorization.set_sensitive(true);
        headers.insert(AUTHORIZATION, authorization);
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            X_GITHUB_API_VERSION,
            HeaderValue::from_str(self.client.api_version().as_str()).map_err(|_| {
                ContextError::HeaderValue {
                    name: "x-github-api-version",
                }
            })?,
        );
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(self.client.user_agent())
                .map_err(|_| ContextError::HeaderValue { name: "user-agent" })?,
        );
        let mut request = reqwest::Request::new(method, url);
        *request.headers_mut() = headers;
        Ok(request)
    }
}

impl fmt::Debug for GitHubContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitHubContext")
            .field("base", &self.client.base_url().as_str())
            .field("token", &format_args!("<redacted>"))
            .field("api_version", &self.client.api_version().as_str())
            .field("user_agent", self.client.user_agent())
            .finish()
    }
}

/// One repository of a [`GitHubContext`].
#[derive(Clone)]
pub struct RepoContext {
    context: GitHubContext,
    handle: GitHubRepo,
    actions: GitHubRepoActions,
}

impl RepoContext {
    /// The repository identity, assembled from the scope's borrowed facts.
    pub fn repo(&self) -> RepoRef {
        RepoRef {
            owner: self.handle.owner().clone(),
            name: self.handle.name().clone(),
        }
    }

    /// The context it was created from.
    pub fn context(&self) -> &GitHubContext {
        &self.context
    }

    /// The generated repository handle (repository, issues, pull requests).
    pub fn handle(&self) -> &GitHubRepo {
        &self.handle
    }

    /// The generated Actions handle (workflows, runs, jobs, artifacts).
    pub fn actions(&self) -> &GitHubRepoActions {
        &self.actions
    }

    /// `GET .../issues` with the filter's values set (each `None` stays absent). Returns the
    /// generated operation: further setters, `prepare()` and `send()` remain available.
    pub fn issues(&self, filter: &IssueFilter) -> GitHubRepoIssues {
        let IssueFilter {
            state,
            labels,
            since,
            sort,
            direction,
            assignee,
            issue_type,
            creator,
            mentioned,
            milestone,
            per_page,
        } = filter.clone();
        self.handle
            .issues()
            .maybe_state(state)
            .maybe_labels(labels)
            .maybe_since(since)
            .maybe_sort(sort)
            .maybe_direction(direction)
            .maybe_assignee(assignee)
            .maybe_issue_type(issue_type)
            .maybe_creator(creator)
            .maybe_mentioned(mentioned)
            .maybe_milestone(milestone)
            .maybe_per_page(per_page)
    }

    /// `GET .../actions/runs` with the filter's values set.
    pub fn runs(&self, filter: &RunFilter) -> GitHubRepoActionsRuns {
        let RunFilter {
            actor,
            branch,
            event,
            status,
            created,
            head_sha,
            exclude_pull_requests,
            per_page,
        } = filter.clone();
        self.actions
            .runs()
            .maybe_actor(actor)
            .maybe_branch(branch)
            .maybe_event(event)
            .maybe_status(status)
            .maybe_created(created)
            .maybe_head_sha(head_sha)
            .maybe_exclude_pull_requests(exclude_pull_requests)
            .maybe_per_page(per_page)
    }

    /// A native request for `method` and the path `repos/{owner}/{repo}/` followed by
    /// `segments` (each one percent-encoded segment), with the service headers. For an operation
    /// this SDK does not declare: execute it once with
    /// `concord::Decoder::<T>::new::<concord::Json>().execute(github.http(), request)`, which
    /// returns the response for any status with its body unread, then `decode()` it.
    ///
    /// # Errors
    ///
    /// [`ContextError`]; nothing is sent.
    pub fn request(
        &self,
        method: Method,
        segments: &[&str],
    ) -> Result<reqwest::Request, ContextError> {
        self.context.build(
            method,
            Some((self.handle.owner().as_str(), self.handle.name().as_str())),
            segments,
        )
    }
}

impl fmt::Debug for RepoContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RepoContext")
            .field(
                "repo",
                &format_args!("{}/{}", self.handle.owner(), self.handle.name()),
            )
            .field("context", &self.context)
            .finish()
    }
}

/// A native request a context could not build. Never contains the token or path data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContextError {
    /// The owner or repository name is empty, `.` or `..`.
    Repository,
    /// The segment at `index` is empty, `.` or `..`.
    Segment {
        /// Its position.
        index: usize,
    },
    /// A configured value is not a valid header value.
    HeaderValue {
        /// The header.
        name: &'static str,
    },
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GitHub context: request not built: ")?;
        match self {
            Self::Repository => f.write_str("owner or repository is empty, `.` or `..`"),
            Self::Segment { index } => write!(f, "path segment {index} is empty, `.` or `..`"),
            Self::HeaderValue { name } => write!(f, "{name} value is not a valid header value"),
        }
    }
}

impl std::error::Error for ContextError {}

fn is_dot_or_empty(segment: &str) -> bool {
    segment.is_empty() || segment == "." || segment == ".."
}

/// Appends `/` and `value` as one segment, encoded as Concord encodes a path value (every byte
/// outside RFC 3986 unreserved as `%XX`).
fn push_segment(path: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    path.push('/');
    for &byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            path.push(char::from(byte));
        } else {
            path.push('%');
            path.push(char::from(HEX[usize::from(byte >> 4)]));
            path.push(char::from(HEX[usize::from(byte & 0x0F)]));
        }
    }
}
