# the GitHub REST API in Concord

Written by Claude Opus 5.5 (`claude-opus-5-5`, as the harness names it), from the guide (the sheet of phase A, `tour.md`, chapters 3 to 6, `grammar.md`, `rules.md`, the tables of chapter 8) and from memory of GitHub's public REST documentation, on 2026-11-12. Nothing was called, nothing was compiled. A detail of GitHub that I am not sure of is marked (unsure).

## The description

```rust
// src/lib.rs of the crate `concord_github`
use base64::{engine::general_purpose::STANDARD, Engine as _};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use concord::{json, link, Binary, Json, Secret};
use serde_json::json;

mod quota;
mod types;
pub use quota::*;
pub use types::*;

concord::api! {
    /// The GitHub REST API on github.com, version `2022-11-28`: repositories, issues,
    /// pull requests, contents, releases, search, the rate limit.
    ///
    /// GitHub refuses a request with no `User-Agent`, so `new` asks for your program's name.
    /// With no token GitHub allows 60 requests an hour, with one 5 000.
    /// For a program that runs for long, build the client with [`Github::watched`] (it keeps
    /// the quotas) and `.with_retry(retry())` (GitHub's own "later").
    pub client Github(
        /// Sent as `User-Agent` on every request: `"my-tool/1.0"`.
        user_agent: String,
        /// A personal access token, or an app's installation token. Sent as a bearer.
        token: Option<Secret>,
    ) {
        base "https://api.github.com";
        headers {
            "accept" = "application/vnd.github+json";
            "x-github-api-version" = "2022-11-28";
            "user-agent" = user_agent;
            "authorization" = "Bearer {token}";
        }
        read Json;
        problem GitHubProblem;

        /// The quotas of the token (or of your address, with no token). It spends none.
        GET rate_limit() -> RateLimits { path "rate_limit"; }

        /// A repository by its id. This is where the `301` of a renamed repository points
        /// (see [`Github::find_repo`]). (unsure: GitHub's reference does not list it)
        GET repository(id: u64) -> Repository { path "repositories/{id}"; }

        /// The user that the token belongs to.
        scope user {
            path "user";

            GET get() -> User;

            /// Repositories that the user can reach. GitHub refuses `kind` together with
            /// `visibility` (`422`).
            GET repos(
                visibility: Option<Visibility>,
                /// Sent as `type`. `all`, `owner`, `public`, `private`, `member`.
                kind: Option<RepoType>,
                sort: Option<RepoSort>,
                direction: Option<Direction>,
                /// At most 100.
                per_page: Option<u8>,
                page: Option<u32>,
            ) -> Vec<Repository> {
                path "repos";
                query { visibility; "type" = kind; sort; direction; per_page; page; }
                paginate link();
            }

            /// A new repository, owned by the user.
            POST create_repo(repo: NewRepo) -> Repository {
                path "repos";
                body json(repo);
            }
        }

        /// Another user, by login.
        scope users(username: String) {
            path "users/{username}";

            GET get() -> User;

            /// The user's public repositories. `kind` is sent as `type`: `all`, `owner`, `member`.
            GET repos(kind: Option<RepoType>, sort: Option<RepoSort>, direction: Option<Direction>,
                      per_page: Option<u8>, page: Option<u32>) -> Vec<Repository> {
                path "repos";
                query { "type" = kind; sort; direction; per_page; page; }
                paginate link();
            }
        }

        /// An organisation, by login.
        scope orgs(org: String) {
            path "orgs/{org}";

            /// The organisation's repositories that the token may see. `kind` is sent as
            /// `type`: `all`, `public`, `private`, `forks`, `sources`, `member`.
            GET repos(kind: Option<RepoType>, sort: Option<RepoSort>, direction: Option<Direction>,
                      per_page: Option<u8>, page: Option<u32>) -> Vec<Repository> {
                path "repos";
                query { "type" = kind; sort; direction; per_page; page; }
                paginate link();
            }

            /// A new repository in the organisation.
            POST create_repo(repo: NewRepo) -> Repository {
                path "repos";
                body json(repo);
            }
        }

        /// One repository. A `404` here means "no such repository" or "not one the token may
        /// see": GitHub does not say which. Under an old name, every endpoint below answers
        /// `301`: the error's `problem().moved_to()` has the new id (see [`Github::find_repo`]).
        scope repo(owner: String, name: String) {
            path "repos/{owner}/{name}";

            GET get() -> Repository;

            /// The repository, or `None` when it has not changed since `etag` (the `etag` of an
            /// earlier answer). A `304` spends no quota (unsure: only with a token).
            GET get_if_changed(etag: Option<String>) -> Option<Tagged<Repository>> {
                headers { "if-none-match" = etag; }
                status {
                    304 => None,
                    200 { body, headers } => Some(Tagged::new(headers.get("etag"), body)),
                }
            }

            /// Commits, additions and deletions of each contributor, week by week. GitHub
            /// computes them on the first ask and answers `202` meanwhile: `Stats::Computing`.
            /// [`github::Repo::contributor_stats_ready`] asks again for you.
            GET contributor_stats() -> Stats<Vec<ContributorStats>> {
                path "stats/contributors";
                status {
                    200 { body } => Stats::Ready(body),
                    202 => Stats::Computing,
                    204 => Stats::Ready(Vec::new()),      // an empty repository (unsure)
                }
            }

            /// The entries at the root of the repository. (`contents` takes a path, and an
            /// empty path cannot be sent.)
            GET contents_root(
                /// A branch, a tag or a commit. The default branch when not given.
                reference: Option<String>,
            ) -> Vec<Entry> {
                path "contents";
                query { "ref" = reference; }
            }

            /// The issues of the repository, and its issue comments.
            scope issues {
                path "issues";

                /// Issues, newest first. GitHub counts a pull request as an issue: those come
                /// too, with `pull_request` set.
                GET list(
                    /// A milestone number, `Filter::Any` or `Filter::Without`.
                    milestone: Option<Filter<u64>>,
                    /// GitHub gives `open` when this is not sent.
                    state: Option<StateFilter>,
                    /// A login, `Filter::Any` or `Filter::Without`.
                    assignee: Option<Filter<String>>,
                    /// The login of who opened it.
                    creator: Option<String>,
                    /// A login that is mentioned in it.
                    mentioned: Option<String>,
                    /// Issues that have every one of these labels.
                    labels: Option<Vec<String>>,
                    /// Issues updated at or after this time.
                    since: Option<DateTime<Utc>>,
                    sort: Option<IssueSort>,
                    direction: Option<Direction>,
                    per_page: Option<u8>,
                    page: Option<u32>,
                ) -> Vec<Issue> {
                    query {
                        milestone; state; assignee; creator; mentioned;
                        "labels" = labels.as_ref().map(|l| l.join(","));
                        since; sort; direction; per_page; page;
                    }
                    paginate link();
                }

                /// A new issue. Labels, assignees and a milestone need push access; without it
                /// GitHub leaves them out and still creates the issue (unsure).
                POST create(title: String, body: Option<String>, labels: Option<Vec<String>>,
                            assignees: Option<Vec<String>>, milestone: Option<u64>) -> Issue {
                    body compact(json!({
                        "title": title, "body": body, "labels": labels,
                        "assignees": assignees, "milestone": milestone
                    }));
                }

                /// One comment, by its id (the ids run across the whole repository).
                scope comment(id: u64) {
                    path "comments/{id}";

                    GET get() -> Comment;

                    PATCH update(body: String) -> Comment {
                        body json(json!({ "body": body }));
                        idempotent;                    // the text is set, not added to
                    }

                    DELETE delete();
                }
            }

            /// One issue (or pull request), by its number.
            scope issue(number: u64) {
                path "issues/{number}";

                GET get() -> Issue;

                /// Sets the fields that `changes` holds; the others stay as they are.
                PATCH update(changes: IssueUpdate) -> Issue {
                    body json(changes);
                    idempotent;                        // every field is set, none is added to
                }

                GET comments(
                    /// Comments updated at or after this time.
                    since: Option<DateTime<Utc>>,
                    per_page: Option<u8>,
                    page: Option<u32>,
                ) -> Vec<Comment> {
                    path "comments";
                    query { since; per_page; page; }
                    paginate link();
                }

                POST create_comment(body: String) -> Comment {
                    path "comments";
                    body json(json!({ "body": body }));
                }
            }

            /// The pull requests of the repository.
            scope pulls {
                path "pulls";

                /// `head` is `user:branch`; `base` is the branch that they want to merge into.
                GET list(state: Option<StateFilter>, head: Option<String>, base: Option<String>,
                         sort: Option<PullSort>, direction: Option<Direction>,
                         per_page: Option<u8>, page: Option<u32>) -> Vec<PullRequest> {
                    query { state; head; base; sort; direction; per_page; page; }
                    paginate link();
                }

                /// A new pull request: `head` into `base`.
                POST create(title: String, head: String, base: String,
                            body: Option<String>, draft: Option<bool>) -> PullRequest {
                    body compact(json!({
                        "title": title, "head": head, "base": base, "body": body, "draft": draft
                    }));
                }
            }

            /// One pull request, by its number.
            scope pull(number: u64) {
                path "pulls/{number}";

                GET get() -> PullRequest;

                /// The diff as GitHub writes it. Bytes, not text: a changed file that is not
                /// UTF-8 is in the diff as it is.
                GET diff() -> Bytes {
                    headers { override "accept" = "application/vnd.github.diff"; }
                    read Binary;
                }

                /// The changed files, at most 3 000.
                GET files(per_page: Option<u8>, page: Option<u32>) -> Vec<DiffEntry> {
                    path "files";
                    query { per_page; page; }
                    paginate link();
                }

                /// `false` also when there is no such pull request: GitHub answers `404` to both.
                GET is_merged() -> bool {
                    path "merge";
                    status { 2xx => true, 404 => false }
                }

                /// Merge it. With `sha`, only if the head is still that commit.
                PUT merge(commit_title: Option<String>, commit_message: Option<String>,
                          sha: Option<String>, merge_method: Option<MergeMethod>)
                    -> Result<Merged, MergeRefused> {
                    path "merge";
                    body compact(json!({
                        "commit_title": commit_title, "commit_message": commit_message,
                        "sha": sha, "merge_method": merge_method
                    }));
                    idempotent false;      // sent again after a merge that happened, it answers 405
                    status {
                        200 { body } => Ok(body),
                        405 { body } => Err(MergeRefused::NotMergeable(body)),
                        409 { body } => Err(MergeRefused::HeadMoved(body)),
                    }
                }
            }

            /// A file or a directory, by its path in the repository (`src/lib.rs`).
            scope contents(path: String) {
                path "contents/{*path}";

                /// The file (its bytes in base64) or the entries of a directory.
                /// `None`: no such path on that reference.
                GET get(
                    /// A branch, a tag or a commit. The default branch when not given.
                    reference: Option<String>,
                ) -> Option<Contents> {
                    query { "ref" = reference; }
                    status { 404 => None }
                }

                /// The bytes of the file, as they are. Up to 100 MB, GitHub's limit for this
                /// form (unsure); past 1 MB the JSON form above has no content.
                GET raw(reference: Option<String>) -> Bytes {
                    query { "ref" = reference; }
                    headers { override "accept" = "application/vnd.github.raw+json"; }
                    read Binary.limit(100 * concord::MIB);
                }

                /// Create the file, or replace it. Replacing needs `sha`, the blob that it
                /// replaces: when someone wrote first, GitHub answers `409`.
                PUT create_or_update(
                    message: String,
                    /// The new bytes. Sent in base64, as GitHub wants.
                    content: Vec<u8>,
                    sha: Option<String>,
                    /// The default branch when not given.
                    branch: Option<String>,
                ) -> FileCommit {
                    body compact(json!({
                        "message": message, "content": STANDARD.encode(content),
                        "sha": sha, "branch": branch
                    }));
                    idempotent false;      // sent again after a write that happened: 409 or 422
                }
            }

            /// The releases of the repository.
            scope releases {
                path "releases";

                /// Published releases, and drafts for a token with push access.
                GET list(per_page: Option<u8>, page: Option<u32>) -> Vec<Release> {
                    query { per_page; page; }
                    paginate link();
                }

                POST create(release: NewRelease) -> Release { body json(release); }

                /// The latest published release that is not a pre-release.
                GET latest() -> Release { path "latest"; }

                /// A tag may hold `/` (`release/1.0`). (unsure: that GitHub reads it unencoded)
                GET by_tag(tag: String) -> Release { path "tags/{*tag}"; }
            }

            /// One release, by its id.
            scope release(id: u64) {
                path "releases/{id}";

                GET get() -> Release;

                DELETE delete();

                GET assets(per_page: Option<u8>, page: Option<u32>) -> Vec<Asset> {
                    path "assets";
                    query { per_page; page; }
                    paginate link();
                }

                /// GitHub's upload host.
                scope upload {
                    base "https://uploads.github.com";

                    /// Attach a file to the release. `file` carries its media type
                    /// (`Body::new(bytes, "application/zip")`) and needs a known length (unsure).
                    /// The time to send it counts as silence: give a large file
                    /// `.with_idle_timeout(…)`. A name that is taken: `422`.
                    POST asset(name: String, label: Option<String>, file: concord::Body) -> Asset {
                        path "assets";
                        query { name; label; }
                        body file;
                    }
                }
            }
        }

        /// Search. It has its own quota: 30 requests a minute with a token.
        scope search {
            path "search";

            /// Repositories that match `q`, in GitHub's search syntax
            /// (`"language:rust stars:>1000"`). At most 1 000 results in all.
            GET repositories(q: String, sort: Option<RepoSearchSort>, order: Option<Direction>,
                             per_page: Option<u8>, page: Option<u32>) -> SearchResults<Repository> {
                path "repositories";
                query { q; sort; order; per_page; page; }
                paginate link();
            }

            /// Issues and pull requests that match `q` (`"repo:rust-lang/rust is:open label:C-bug"`).
            GET issues(q: String, sort: Option<IssueSearchSort>, order: Option<Direction>,
                       per_page: Option<u8>, page: Option<u32>) -> SearchResults<Issue> {
                path "issues";
                query { q; sort; order; per_page; page; }
                paginate link();
            }
        }
    }
}
```

42 endpoints in 13 scopes; with the blocks hidden, the tree is GitHub's URL.

## Beside the macro

### The data types

Two in full. The problem type, because the `301` of a renamed repository depends on it:

```rust
// src/types.rs
use serde::{Deserialize, Serialize, Serializer};

/// How GitHub says no: `{ "message": "Not Found", "documentation_url": "…" }`, with
/// `errors` on a `422`, and `url` on the `301` of a renamed repository.
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubProblem {
    pub message: String,
    #[serde(default)]
    pub documentation_url: Option<String>,
    /// What did not validate (`422`): objects with `resource`, `field` and `code`, or plain
    /// text. GitHub sends both (unsure), so they stay JSON.
    #[serde(default)]
    pub errors: Vec<serde_json::Value>,
    /// On a `301`: the new address of the repository, `https://api.github.com/repositories/{id}`.
    #[serde(default)]
    pub url: Option<String>,
    /// The status again, as text (`"404"`), on newer answers (unsure).
    #[serde(default)]
    pub status: Option<String>,
}

impl GitHubProblem {
    /// The id of a renamed repository, from the body of its `301`.
    pub fn moved_to(&self) -> Option<u64> {
        self.url.as_deref()?.rsplit('/').next()?.parse().ok()
    }
}
```

And the wrapped list of search, because `.items()` needs to know where its items are:

```rust
/// One page of a search.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchResults<T> {
    /// Every match, on every page (GitHub gives at most 1 000 of them).
    pub total_count: u64,
    /// The search ran out of time: these are some of the matches, not all.
    pub incomplete_results: bool,
    pub items: Vec<T>,
}

impl<T> IntoIterator for SearchResults<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter { self.items.into_iter() }
}
```

The rest, named. All `Deserialize` unless marked; each enum that GitHub sends has a last case `#[serde(other)] Unknown`.

| Type | What |
|---|---|
| `User`, `Repository`, `Issue`, `Comment`, `PullRequest`, `DiffEntry`, `Release`, `Asset`, `Entry`, `FileCommit`, `Merged`, `RateLimits`, `ContributorStats` | GitHub's objects. `Entry.kind` is `#[serde(rename = "type")]`. |
| `NewRepo`, `NewRelease` (`Serialize`, `Default`) | Bodies with many optional fields, each `#[serde(skip_serializing_if = "Option::is_none")]`. Shared by two endpoints (`NewRepo`), or too long for inputs (`NewRelease`). |
| `IssueUpdate` (`Serialize`, `Default`) | `milestone: Option<Option<u64>>` with `serde_with::rust::double_option`: `None` leaves it, `Some(None)` sends `null` and removes it. The same for `body`. |
| `Contents` | `#[serde(untagged)] enum { Dir(Vec<Entry>), File(Entry) }`: GitHub answers an array for a directory, an object for the rest. |
| `MergeRefused` | `enum { NotMergeable(GitHubProblem), HeadMoved(GitHubProblem) }`: the `405` and the `409` of a merge. |
| `Tagged<T>` | `{ pub etag: Option<String>, pub value: T }`, with `fn new(etag: Option<&str>, value: T)`. |
| `Stats<T>` | `enum { Ready(T), Computing }`. |
| `Visibility`, `RepoType`, `RepoSort`, `Direction`, `StateFilter`, `IssueState`, `StateReason`, `IssueSort`, `PullSort`, `MergeMethod`, `RepoSearchSort`, `IssueSearchSort` (`Serialize`) | `#[serde(rename_all = "snake_case")]`, with a `rename` where GitHub writes otherwise: `PullSort::LongRunning` is `long-running`, `IssueSearchSort::ReactionsPlusOne` is `reactions-+1`. |

One more, in full: its wire text is not what `serde` writes by default.

```rust
/// A filter that takes a value, "any" or "none": `milestone` and `assignee`.
#[derive(Debug, Clone)]
pub enum Filter<T> {
    /// This milestone number, this login.
    Is(T),
    /// Any, but there is one: sent as `*`.
    Any,
    /// There is none: sent as `none`.
    Without,
}

// `serde`'s own derive would write `Is(5)` as `{"Is":5}`, a map, which Concord refuses in a query.
impl<T: Serialize> Serialize for Filter<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Filter::Is(value) => value.serialize(serializer),
            Filter::Any => serializer.serialize_str("*"),
            Filter::Without => serializer.serialize_str("none"),
        }
    }
}
```

### A body without its `null`s

```rust
// src/lib.rs, beside the macro

/// A JSON body without its `null` fields. An option that the caller did not give is not
/// sent at all: GitHub may read a `null` as "set to nothing" (unsure), and `json!` writes
/// `None` as `null`.
pub fn compact(mut value: serde_json::Value) -> Result<concord::Body, serde_json::Error> {
    if let Some(fields) = value.as_object_mut() {
        fields.retain(|_, field| !field.is_null());
    }
    Ok(concord::Body::new(serde_json::to_vec(&value)?, "application/json"))
}
```

### GitHub's own "later"

```rust
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// For `with_retry`: GitHub's rate limits. A secondary limit is a `403` or a `429`, with
/// `retry-after` or without it; a spent primary limit is a `403` or a `429` with
/// `x-ratelimit-remaining: 0`, until `x-ratelimit-reset`. More than a minute away, the
/// call ends with the answer, and its headers say when.
pub fn retry() -> concord::Retry<GitHubProblem> {
    concord::Retry::standard()
        // GitHub does nothing with a request that a limit turned away (unsure), so a `POST`
        // may go again after one. Any other 403 is still not tried again: `when` says nothing.
        .not_applied_on(403)
        .not_applied_on(429)
        .when(|tried| {
            let status = tried.status()?;
            if status != 403 && status != 429 {
                return None;
            }
            let headers = tried.headers();
            if let Some(wait) = headers.retry_after() {
                return Some(wait);
            }
            if headers.get("x-ratelimit-remaining") == Some("0") {
                let reset: u64 = headers.get("x-ratelimit-reset")?.parse().ok()?;
                return Some(Duration::from_secs(reset.saturating_sub(unix_now())));
            }
            // a secondary limit with no header: GitHub says to wait at least a minute
            tried.problem()?.message.contains("secondary rate limit").then(|| Duration::from_secs(60))
        })
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}
```

### The quotas: a transport

The rate-limit headers come on every answer, and the description has no place that sees every answer. A transport does.

```rust
// src/quota.rs
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use concord::transport::{Transport, TransportError, TryInfo};

/// What GitHub said of one quota (`core`, `search`, …) on its last answer.
#[derive(Debug, Clone)]
pub struct Quota { pub limit: u64, pub remaining: u64, pub used: u64, /** Unix seconds. */ pub reset: u64 }

/// The quotas seen so far. A clone reads the same ones.
#[derive(Clone, Default)]
pub struct Quotas(Arc<Mutex<HashMap<String, Quota>>>);

impl Quotas {
    pub fn get(&self, resource: &str) -> Option<Quota> {
        self.0.lock().unwrap().get(resource).cloned()
    }
}

/// The cause of a request that was not sent: the quota is spent until `reset`.
#[derive(Debug, thiserror::Error)]
#[error("the {resource} quota of GitHub is spent until {reset}")]
pub struct Spent { pub resource: String, pub reset: u64 }

/// Reads the `x-ratelimit-*` headers of every answer (every try, every page), and does not
/// send while the quota of that kind of request is spent.
pub struct RateWatch<T> { quotas: Quotas, below: T }

impl<T: Transport> Transport for RateWatch<T> {
    async fn send(&self, request: reqwest::Request, info: TryInfo) -> Result<reqwest::Response, TransportError> {
        // Which quota a request spends, its answer says (`x-ratelimit-resource`). Before the
        // answer, the name of the endpoint is the best guess. `rate_limit` spends nothing.
        let endpoint = info.endpoint();
        let resource = if endpoint.starts_with("github.search.") { "search" } else { "core" };
        if endpoint != "github.rate_limit" {
            if let Some(quota) = self.quotas.get(resource) {
                if quota.remaining == 0 && quota.reset > crate::unix_now() {
                    let spent = Spent { resource: resource.to_owned(), reset: quota.reset };
                    return Err(TransportError::blocked().with_source(spent));
                }
            }
        }
        let response = self.below.send(request, info).await?;
        if let Some((resource, quota)) = quota_of(response.headers()) {
            self.quotas.0.lock().unwrap().insert(resource, quota);
        }
        Ok(response)
    }
}

fn quota_of(headers: &reqwest::header::HeaderMap) -> Option<(String, Quota)> {
    let text = |name: &str| headers.get(name)?.to_str().ok();
    let number = |name: &str| text(name)?.parse::<u64>().ok();
    let quota = Quota {
        limit: number("x-ratelimit-limit")?,
        remaining: number("x-ratelimit-remaining")?,
        used: number("x-ratelimit-used")?,
        reset: number("x-ratelimit-reset")?,
    };
    Some((text("x-ratelimit-resource")?.to_owned(), quota))
}

impl Github {
    /// The way in for a program that runs for long: a client that keeps GitHub's quotas and
    /// does not send while one is spent. (`Github::new` keeps none.)
    pub fn watched(user_agent: &str, token: Secret) -> reqwest::Result<(Github, Quotas)> {
        Ok(Self::watched_with(user_agent, token, concord::Http::with(|builder| builder)?))
    }

    /// The same, over what you send with: an HTTP client of yours, a fake in a test.
    /// (`with_transport` on a watched client would replace the watch with the rest.)
    pub fn watched_with(user_agent: &str, token: Secret, below: impl Transport) -> (Github, Quotas) {
        let quotas = Quotas::default();
        let client = Github::new(user_agent)
            .token(token)
            .with_transport(RateWatch { quotas: quotas.clone(), below });
        (client, quotas)
    }
}
```

### Two methods beside the generated ones

```rust
impl Github {
    /// The repository, also after a rename or a transfer. Under its old name GitHub answers
    /// `301` and writes the new place in the body; this asks there once.
    pub async fn find_repo(&self, owner: &str, name: &str) -> github::Result<Repository> {
        match self.repo(owner, name).get().await {
            Err(e) if e.is_status(301) => match e.problem().and_then(GitHubProblem::moved_to) {
                Some(id) => self.repository(id).await,
                None => Err(e),
            },
            other => other,
        }
    }
}

impl github::Repo {
    /// The contributor statistics, asked again every two seconds while GitHub computes them,
    /// `tries` times at most. `None`: still not ready.
    pub async fn contributor_stats_ready(&self, tries: u32) -> github::Result<Option<Vec<ContributorStats>>> {
        for _ in 0..tries {
            if let Stats::Ready(stats) = self.contributor_stats().await? {
                return Ok(Some(stats));
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        Ok(None)
    }
}
```

## Calling it

**1. A client for a bot, and a repository that may have moved.**

```rust
use concord_github::*;                  // Github, the module `github`, the types, retry, Quotas
use futures::TryStreamExt;

let token = concord::Secret::new(std::env::var("GITHUB_TOKEN")?);   // the constructor's name: not found
let (gh, quotas) = Github::watched("release-bot/2.1", token)?;
let gh = gh.with_retry(retry());

let found = gh.find_repo("old-org", "old-name").await?;            // follows one 301
println!("{} has {} stars", found.full_name, found.stargazers_count);
if let Some(core) = quotas.get("core") {
    println!("{} of {} requests left", core.remaining, core.limit);
}
let repo = gh.repo(&found.owner.login, &found.name);
```

**2. Every open bug that nobody took, updated this week.**

```rust
let mut issues = repo.issues().list()
    .state(StateFilter::Open)
    .labels(vec!["C-bug".to_owned()])
    .assignee(Filter::Without)
    .since(Utc::now() - chrono::Duration::days(7))
    .per_page(100)
    .items();                                        // every page, by `Link`

while let Some(issue) = issues.try_next().await? {
    if issue.pull_request.is_none() {                // GitHub lists pull requests too
        println!("#{} {}", issue.number, issue.title);
    }
}
```

**3. Read a file, change it, write it back, and lose a race cleanly.**

```rust
let file = repo.contents("Cargo.toml");

let Some(Contents::File(current)) = file.get().reference("main").await? else {
    anyhow::bail!("Cargo.toml is not a file on main");
};
let text = String::from_utf8(file.raw().reference("main").await?.to_vec())?;   // the bytes, by `Accept`
let bumped = text.replace("version = \"1.4.0\"", "version = \"1.5.0\"");

match file.create_or_update("Release 1.5.0", bumped.into_bytes())
    .sha(&current.sha)
    .branch("main")
    .await
{
    Ok(written) => println!("commit {}", written.commit.sha),
    Err(e) if e.is_status(409) => println!("someone pushed first: read it again"),
    Err(e) => return Err(e.into()),
}
```

**4. A release, and its binary on the upload host.**

```rust
let release = repo.releases()
    .create(NewRelease { tag_name: "v1.5.0".into(), draft: Some(true), ..Default::default() })
    .await?;

let bytes = tokio::fs::read("dist/tool-x86_64-linux.tar.gz").await?;
let asset = repo.release(release.id).upload()
    .asset("tool-x86_64-linux.tar.gz", concord::Body::new(bytes, "application/gzip"))
    .label("Linux, x86-64")
    .with_idle_timeout(Duration::from_secs(300))     // sending it counts as silence
    .await?;
println!("{}", asset.browser_download_url);
```

**5. Polling without spending the quota.**

```rust
// The repository has an endpoint for it.
let mut etag: Option<String> = None;
loop {
    if let Some(fresh) = repo.get_if_changed().etag(etag.clone()).await? {
        println!("{} open issues", fresh.value.open_issues_count);
        etag = fresh.etag;
    }
    tokio::time::sleep(Duration::from_secs(60)).await;
}

// The issue list has none: the door, and the `304` comes as an error.
// (`list_etag: Option<String>` is kept from the last poll.)
let sent = list_etag.clone();
match repo.issues().list().state(StateFilter::Open).per_page(100)
    .edit_request(move |request| {
        if let Some(tag) = &sent {
            request.headers_mut().insert("if-none-match", tag.parse().unwrap());   // an ETag is ASCII
        }
    })
    .send_full()
    .await
{
    Ok(answer) => {
        list_etag = answer.headers().get("etag").map(String::from);
        show(answer.into_value());
    }
    Err(e) if e.is_status(304) => {}                 // nothing changed
    Err(e) => return Err(e.into()),
}
```

**6. Merge what was reviewed; search; statistics.**

```rust
match repo.pull(4521).merge().merge_method(MergeMethod::Squash).sha(&reviewed_head).await? {
    Ok(merged) => println!("merged as {}", merged.sha),
    Err(MergeRefused::HeadMoved(_)) => println!("new commits since the review: look again"),
    Err(MergeRefused::NotMergeable(why)) => println!("not mergeable: {}", why.message),
}

let found = gh.search().repositories("language:rust topic:http-client")
    .sort(RepoSearchSort::Stars)
    .per_page(10)
    .await?;
println!("{} in all{}", found.total_count, if found.incomplete_results { ", maybe more" } else { "" });

match repo.contributor_stats_ready(5).await? {
    Some(stats) => println!("{} contributors", stats.len()),
    None => println!("GitHub is still counting: ask later"),
}
```

## Findings

| # | Where | What got in the way | What I wrote instead | Kind |
|---|---|---|---|---|
| 1 | The client | GitHub refuses a request with no `User-Agent`, and Concord adds none. | A required client input, `user_agent`, and `"user-agent" = user_agent;`. Every caller names their program. | small |
| 2 | The client's name | `GitHub` is the brand. The module it gives (`github` or `git_hub`) is in the contracts, which are not in my pages. | `Github`. | small |
| 3 | An optional token | `token: Option<Secret>` with `"Bearer {token}"` works well (no token, no header). But the table of 4.10 has no "text for an `Option<Secret>`", and no page says how a `Secret` is made from text. | `concord::Secret::new(…)`, a guess. | small |
| 4 | `type` (repository lists), `ref` (contents) | Rust keywords cannot be inputs. | `kind` with `"type" = kind;`, `reference` with `"ref" = reference;`. The caller reads `.kind(…)` where GitHub's page says `type`. | small |
| 5 | `labels` of the issue list | A comma list. A `Vec` input is required (only `Option` makes an option), and `labels.join(",")` of an empty list sends `labels=`: a list's "empty sends nothing" is lost in the join. | `labels: Option<Vec<String>>` and `"labels" = labels.as_ref().map(\|l\| l.join(","));`. The caller cannot write `.labels(["bug"])`: the impls give `Option<Vec<String>>` only from a `Vec<String>`. | said with pain |
| 6 | `milestone`, `assignee` | A number or a login, or `*`, or `none`. The derive writes a map, which is refused when the request is built. | `Filter<T>` with its own `Serialize`, 10 lines. The guide says this is the way. | small |
| 7 | Every list (11 endpoints) | `per_page: Option<u8>, page: Option<u32>`, `per_page; page;` and `paginate link();` written again on each list: some 30 lines that say one fact of GitHub. Parts are parked. | The repetition. `paginate link();` itself is one word and right. | said with pain |
| 8 | Conditional requests | An arm for the `304` changes the arrow, so a `GET` that may be conditional is a second endpoint. Doing it on every endpoint would give `Option<Tagged<…>>` to every caller. | `get_if_changed` on the repository. For the issue list, the door: `edit_request` for `If-None-Match`, `send_full` for the `ETag`, and the `304` arrives as a Refused error. | said with pain |
| 9 | The rate-limit headers | On every answer; the description has no place that sees every answer, and pages give values, not heads. Which quota a request spends is known only from its answer. | A transport, `RateWatch` (45 lines), and two constructors, `watched` and `watched_with`. `Github::new` gives a client that does not watch, and `with_transport` on a watched client drops the watch. | said with pain |
| 10 | The secondary limit | A `403` (not only `429`) with `Retry-After`, or with nothing and a message. | `retry()` beside the macro, with `when` and `not_applied_on(403)`. The application has to name it. The guide's own example in 6.3 is this case. | small |
| 11 | A renamed repository | GitHub answers `301` for every path under the old name. Concord follows no redirect, and an arm cannot make a second request. The description cannot say "ask again at the new place". | Nothing in the description: the `301` is a Refused error whose problem has the new address. `find_repo` beside the macro follows it for `get` only; every other endpoint under an old name fails. | cannot be said |
| 12 | `404` | It means "missing" or "you may not see it". | `404 => None` only where the caller already sees the repository (`contents.get`, `is_merged`). Elsewhere an error, and a doc comment says why. | small |
| 13 | The upload of an asset | It goes to another host. `base` cannot stand on an endpoint. | A scope `upload` with one endpoint and only a `base`: against advice 3, and the call has a level for nothing (`release(id).upload().asset(…)`). It relies on my reading of rule 5: the paths above a lower `base` are kept. | small |
| 14 | GitHub Enterprise Server | Its two hosts are `https://host/api/v3` and `https://host/api/uploads`. `with_base` sends every request to one place, uploads included. | Nothing: this client is github.com only. Serving both would take two `Base` inputs on the client, which every github.com caller would then give. | said with pain |
| 15 | The release's `upload_url` | GitHub gives a URI template (`…/assets{?name,label}`). | Ignored; the URL is built from parts, as the language wants. | small |
| 16 | A large upload | Sending the body counts as silence (30 s). | A doc comment on `asset`, and `.with_idle_timeout(…)` in the calling code. | small |
| 17 | The statistics (`202`) | "Ask again later" with a `2xx`: retry does not see it, since a `202` is a success. | Arms and `Stats<T>`, which read well; the waiting is a loop of mine beside (`contributor_stats_ready`). | small |
| 18 | `merge`, `create_or_update` | A `PUT` whose second send changes nothing, but answers `405`, `409` or `422`, and so hides a write that happened. | `idempotent false;`. I left the `DELETE`s as HTTP says, although their second send answers `404`. | small |
| 19 | `merge` | Once the arrow is `Result<Merged, MergeRefused>`, the `200` arm must be written. Without it the simple rule asks `serde` for a `Result`, reads `{"Ok": …}`, and the call is a Mismatch at run time. It compiles. | `200 { body } => Ok(body)`. | said with pain |
| 20 | The root of the contents | `{*path}` cannot be empty. | A second endpoint, `contents_root`. | small |
| 21 | `contents.get` | GitHub answers an object for a file, an array for a directory. | An untagged enum. Not the language's business. | small |
| 22 | `contents.raw` | `Binary` stops at 10 MiB; GitHub serves up to 100 MB this way. | `read Binary.limit(100 * concord::MIB);`, assuming that `Binary` has `limit` as `Json` does. | small |
| 23 | `pull.diff` | A diff is text, but a changed file in Latin-1 makes it not UTF-8, and `Text` refuses that. | `read Binary;` and `-> Bytes`. | small |
| 24 | `issue.update` | `null` removes a milestone, absence keeps it. As an input, `Option<Option<u64>>` would make `.milestone(None)` ambiguous between the two `Arg` impls. | A struct, `IssueUpdate`, with `double_option`. | small |
| 25 | Bodies made of optional inputs | `json!` writes `None` as `null`. | `compact`, a function of mine that removes the `null`s. Short, but every author of a `PATCH`-heavy API will write it again. | small |
| 26 | Inputs named `base`, `path`, `body` | GitHub's own words, and the names of statements. No page says whether that is allowed. | I used them: `pulls.list(…).base("main")`, `contents(path)`, `create_comment(body)`. | small |
| 27 | Request bodies | Some are inputs (issues, pull requests, merge), some are structs (`NewRepo`, `NewRelease`, `IssueUpdate`). | Inputs where the body is short and one endpoint has it, a struct where it is shared or long. The caller sees two styles. | small |
| 28 | Generated names | The handle `github::repo::Issue` and my data type `Issue`; `github::User` and `User`. The guide allows it. | Kept. Reading the generated docs, the two are easy to mix up. | small |
| 29 | An app's installation token | It ends after an hour. | Nothing in the description: the application calls `key.replace(…)` on the `Secret` from a task. No transport needed. | small |

What I wished I could write:

- **5.** `labels: Vec<String>` as an option that is empty when not given, set with `.labels(["bug", "ui"])`, and a join that sends nothing when empty.
- **7.** A named part: `GET files(..paged) -> Vec<DiffEntry> { path "files"; }`, where `paged` holds the two inputs, their query and `paginate link();`.
- **8.** On any `GET` call: `.send_if_changed(etag)`, giving `Option<Answer<T>>`, `None` on a `304`.
- **9.** A layer that the author puts on the client and that `with_transport` goes under, not replaces: `layer RateWatch::new();`.
- **11.** On the `repo` scope: `status { 301 { headers } => again(headers.require("location")?) }`: ask once more, same origin, same credentials.
- **14.** A base with a name, moved by its name: `base uploads "https://uploads.github.com";` and `.with_base_of("uploads", ghes_uploads)`.
- **19.** A description that does not compile when the arrow is a `Result` and no `2xx` arm says how the success becomes `Ok`.

## Read twice

| Question | Where the answer was |
|---|---|
| Does a `base` written low in the tree keep the paths of the nodes above it (`uploads.github.com` + `repos/{owner}/{name}` + `releases/{id}`)? | Rules, rule 5: "the nearest `base`, then every `path` on the way down". Inferred; no example shows a `base` below a `path`. |
| Is an input spelled `Vec<…>` required? | Not said. Rule 2 and grammar sentence 3: only `Option` makes an option. So yes, inferred. |
| In a behaviour, is an `Option` input a `&Option<T>` or an `Option<&T>`? | Not found ("a variable that borrows the input", 3.7). I wrote `labels.as_ref().map(…)`, which compiles either way. |
| May an input be named `base`, `path` or `body`? | Not found. Only grammar table row 10 ("an input named `body` is no longer hidden in an arm") suggests yes. |
| What module does the client `GitHub` give? | Not found: the contracts C1.4 to C1.6, which are not in my pages. |
| Has `Binary` a `.limit(…)`? | 5.3 shows `Json.limit` and says a limit "is written on the reader". Assumed. |
| How is a `Secret` made from text, and can `.token("…")` take text for an `Option<Secret>`? | Not found. 4.10 has text for a `Secret` and for an `Option<String>`, not for an `Option<Secret>`. |
| By what method does a transport read the endpoint's name in `TryInfo`? | Not found: 4.8 says that it is told. I wrote `info.endpoint()`, in the form of `e.endpoint()` (`riot.matches.get`). |
| How is Concord's own HTTP client made with no setting, to wrap it? | The sheet, item 5, shows only `Http::with(…)`; `Http::with(\|b\| b)` and its error type are a guess. |
| Does a `when` that asks for more than 60 s end the tries, as a long `Retry-After` does? | Not found; 6.3 says it for `Retry-After` only. `retry()` relies on it for a spent primary limit. |
| Is a `3xx` with no arm decoded into `problem()`? | Yes: 5.6 ("on a status with no arm") with 5.5 (a redirect is "an answer like any other"). Found on the second read. |
| With an arrow `Result<T, E>`, what does an unwritten `200` give? | 5.6, the `charge` example, which writes its `200` arm; the reason (serde reads a `Result` as `{"Ok": …}`) is mine. |
| Can a scope have an optional input (for `ref` on all of `contents`)? | Not found. I put `reference` on each endpoint. |
| Arms in chapters 5 and 6 are written `200(body)`, and `problem … as Json`. | The sheet: `200 { body }`, `problem … read …`. It wins; answered at once, but every example had to be translated. |

## Scores

- **Writing this description: 7.** Most of GitHub went in at the first try, `paginate link();` and the status arms best of all; the cost is the paging inputs written eleven times, and four doors I had to guess the names of.
- **Reading it back a month later: 8.** Hide the blocks and it is GitHub's own reference, but the quota watch, the retry, the rename and `compact` live beside the macro, and a reader has to know to look there.
- **The calling code: 7.** The everyday calls are short and typed (filters, pages, the merge's two refusals), but a conditional list, a renamed repository and the quotas each need a constructor, a method beside, or the door.
