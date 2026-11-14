# GitHub

*[Concord](../../README.md) · [docs](../README.md) · examples · previous: [all the examples](README.md) · next: [Riot Games](riot.md)*

For a Rust developer who calls the GitHub REST API: a client of 41 endpoints to copy from. It shows pages in a `Link` header, a second host for uploads, and a middleware that keeps the rate-limit headers of every answer.

GitHub, as Concord sees it. The host is `api.github.com`; the files of a release go to a second host, `uploads.github.com`. Who you are: a token, optional (`.token(…)` on the client), sent as `authorization: Bearer …`; GitHub also wants a `user-agent` on every request, and the client pins the API version in a header. A list comes in pages, and the next one is in the `Link` header: `paginate by_link();`. GitHub says no with a status and a JSON body (`message`, `documentation_url`), read into `GithubProblem`; a `404` also means "you may not see it". GitHub's own reference: [docs.github.com/en/rest](https://docs.github.com/en/rest).

## The description

```rust
use concord::bytes::Bytes;
use concord::{by_link, json, Binary, Body, Json, Secret};

concord::api! {
    /// The GitHub REST API: users, repositories, issues, pull requests, contents, releases, search.
    pub client Github(user_agent: String, quota: QuotaLog, token?: Secret) {
        base "https://api.github.com";
        headers {
            "user-agent" = user_agent;
            "accept" = "application/vnd.github+json";
            "x-github-api-version" = "2026-03-10";
            "authorization" = "Bearer {token}";
        }
        read Json;
        problem GithubProblem;
        /// Keeps the rate-limit headers of every answer: each page, each try.
        middleware ObserveQuota(quota.clone());

        part page(per_page?: u32, page?: u32) {
            query { per_page; page; }
            paginate by_link();
        }
        part repo_order(sort?: RepoSort, direction?: Direction) {
            query { sort; direction; }
        }
        part at_ref(reference?: String) {
            query { "ref" = reference; }
        }

        GET rate_limit() -> RateLimits { path "rate_limit"; }
        GET repository(id: u64) -> Repository { path "repositories/{id}"; }

        scope user {
            path "user";
            GET get() -> User;
            /// `kind` cannot go with `visibility` or `affiliation`: such a call is not sent.
            GET repos(
                visibility?: Visibility, affiliation?: Vec<String>,
                kind?: UserRepoType, since?: String, before?: String,
                ..repo_order, ..page
            ) -> Vec<Repository> {
                path "repos";
                query {
                    visibility;
                    "affiliation" = affiliation.as_ref().map(|v| v.join(","));
                    "type" = checked_repo_kind(kind, visibility, affiliation)?;
                    since; before;
                }
            }
            POST create_repo(repo: NewRepository) -> Repository {
                path "repos";
                body json(repo);
            }
        }

        scope users(username: String) {
            path "users/{username}";
            GET get() -> User;
            GET repos(kind?: PublicRepoType, ..repo_order, ..page) -> Vec<Repository> {
                path "repos";
                query { "type" = kind; }
            }
        }

        scope orgs(org: String) {
            path "orgs/{org}";
            GET repos(kind?: OrgRepoType, ..repo_order, ..page) -> Vec<Repository> {
                path "repos";
                query { "type" = kind; }
            }
            POST create_repo(repo: NewRepository) -> Repository {
                path "repos";
                body json(repo);
            }
        }

        scope repo(owner: String, name: String) {
            path "repos/{owner}/{name}";
            /// A repository that moved answers `301`, an error with its `Location`: see `get_moved`.
            GET get() -> Repository;
            /// `None`: GitHub is still counting (`202`). Ask again later.
            GET contributor_stats() -> Option<Vec<ContributorStats>> {
                path "stats/contributors";
                status { 202 => None, 204 => Some(Vec::new()) }
            }
            /// The root folder of the repository. Not paged: at most 1,000 entries.
            GET contents_root(..at_ref) -> Vec<ContentEntry> { path "contents"; }

            scope issues {
                path "issues";
                /// Pull requests too. `milestone`, `assignee` and `kind` take `"*"` and `"none"`;
                /// `since` is a UTC time (ISO 8601); an issue has every one of the `labels`.
                GET list(
                    milestone?: String, state?: IssueState, assignee?: String,
                    kind?: String, creator?: String, mentioned?: String,
                    issue_field_values?: String, labels?: Vec<String>,
                    sort?: IssueSort, direction?: Direction, since?: String,
                    ..page
                ) -> Vec<Issue> {
                    query {
                        milestone; state; assignee; "type" = kind; creator; mentioned;
                        issue_field_values;
                        "labels" = labels.as_ref().map(|v| v.join(","));
                        sort; direction; since;
                    }
                }
                POST create(issue: NewIssue) -> Issue { body json(issue); }

                scope comment(id: u64) {
                    path "comments/{id}";
                    GET get() -> Comment;
                    PATCH update(body: String) -> Comment { body json { body; } }
                    DELETE delete();
                }

                scope issue(number: u64) {
                    path "{number}";
                    GET get() -> Issue;
                    /// A field not set is left as it is; `.milestone(None::<u64>)` clears it (`null`).
                    PATCH update(
                        title?: String, body?: Option<String>, state?: IssueState,
                        milestone?: Option<u64>, labels?: Vec<String>, assignees?: Vec<String>,
                    ) -> Issue {
                        body json { title; body; state; milestone; labels; assignees; }
                    }
                    GET comments(since?: String, ..page) -> Vec<Comment> {
                        path "comments";
                        query { since; }
                    }
                    POST create_comment(body: String) -> Comment {
                        path "comments";
                        body json { body; }
                    }
                }
            }

            scope pulls {
                path "pulls";
                GET list(
                    state?: IssueState, head?: String, base?: String,
                    sort?: PullSort, direction?: Direction, ..page
                ) -> Vec<Pull> {
                    query { state; head; base; sort; direction; }
                }
                POST create(pull: NewPull) -> Pull { body json(pull); }

                scope pull(number: u64) {
                    path "{number}";
                    GET get() -> Pull;
                    GET diff() -> Bytes {
                        headers { override "accept" = "application/vnd.github.diff"; }
                        read Binary;
                    }
                    GET files(..page) -> Vec<PullFile> { path "files"; }
                    /// `404` is GitHub's "no". A pull request that you cannot see is a `404` too.
                    GET is_merged() -> bool {
                        path "merge";
                        status { 204 => true, 404 => false }
                    }
                    PUT merge(options: MergePull) -> MergeResult {
                        path "merge";
                        body json(options);
                        idempotent false;
                    }
                }
            }

            scope contents(path: String) {
                path "contents/{*path}";
                GET get(..at_ref) -> Contents;
                /// The bytes of a file, as they are: not always UTF-8.
                GET raw(..at_ref) -> Bytes {
                    headers { override "accept" = "application/vnd.github.raw+json"; }
                    read Binary.limit(100 * concord::MIB);
                }
                /// `content` is base64; `sha` is required to replace a file that exists.
                PUT create_or_update(change: PutContent) -> ContentWrite {
                    body json(change);
                    idempotent false;
                }
            }

            scope releases {
                path "releases";
                GET list(..page) -> Vec<Release>;
                POST create(release: NewRelease) -> Release { body json(release); }
                GET latest() -> Release { path "latest"; }
                GET by_tag(tag: String) -> Release { path "tags/{tag}"; }

                scope release(id: u64) {
                    path "{id}";
                    GET get() -> Release;
                    DELETE delete();
                    GET assets(..page) -> Vec<Asset> { path "assets"; }
                    /// On the upload host. The body carries its media type; `size` is its length.
                    POST asset(name: String, file: Body, size: u64, label?: String) -> Asset {
                        base "https://uploads.github.com";
                        path "assets";
                        headers {
                            override "authorization" = "Bearer {client.token}";
                            "content-length" = size;
                        }
                        query { name; label; }
                        body file;
                    }
                }
            }
        }

        scope search {
            path "search";
            /// `q` is GitHub's search text, as you would type it.
            GET repositories(q: String, sort?: String, order?: Direction, ..page)
                -> SearchPage<Repository> {
                path "repositories";
                query { q; sort; order; }
            }
            GET issues(q: String, sort?: String, order?: Direction, ..page)
                -> SearchPage<Issue> {
                path "issues";
                query { q; sort; order; }
            }
        }
    }
}
```

## Beside the description

The data types are plain `serde` types, named here and not printed: `Repository`, `User`, `Issue`, `Comment`, `Pull`, `PullFile`, `MergeResult`, `Release`, `Asset`, `ContributorStats`, `RateLimits`, `ContentEntry`, `Contents` (one entry, or the entries of a folder), `ContentWrite`, and `GithubProblem`. What is sent: `NewRepository`, `NewIssue`, `NewPull`, `MergePull`, `PutContent`, `NewRelease`, each with `#[serde(skip_serializing_if = "Option::is_none")]` on its optional fields. The small enums (`IssueState`, `Visibility`, `RepoSort`, …) are written in lowercase, as GitHub spells them.

The Rust that the description names: the page type of a search, the middleware that keeps the quota, the check of `user.repos`, and a function that follows a repository that moved.

```rust
use concord::http::{HeaderMap, HeaderName};
use concord::transport::{TransportError, TryInfo};
use concord::{reqwest, Middleware, Next};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// A page of a search. `incomplete_results`: GitHub stopped looking; more may match.
#[derive(Debug, Deserialize)]
pub struct SearchPage<T> {
    pub total_count: u64,
    pub incomplete_results: bool,
    pub items: Vec<T>,
}

impl<T: Send> concord::page::IntoItems for SearchPage<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> {
        self.items
    }
    fn count(&self) -> usize {
        self.items.len()
    }
}

/// The last rate-limit headers of each resource (`core`, `search`, …), shared with the caller.
pub type QuotaLog = Arc<Mutex<BTreeMap<String, HeaderMap>>>;

/// `middleware ObserveQuota(quota.clone());`: it sees every answer, of every page and every try.
pub struct ObserveQuota(pub QuotaLog);

impl Middleware for ObserveQuota {
    async fn handle(&self, request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        let answer = next.send(request, info).await?;
        let mut kept = HeaderMap::new();
        for name in [
            "x-ratelimit-limit", "x-ratelimit-remaining", "x-ratelimit-used",
            "x-ratelimit-reset", "x-ratelimit-resource", "retry-after",
        ] {
            if let Some(value) = answer.headers().get(name) {
                kept.insert(HeaderName::from_static(name), value.clone());
            }
        }
        let resource = answer.headers().get("x-ratelimit-resource")
            .and_then(|v| v.to_str().ok()).unwrap_or("unknown").to_owned();
        self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(resource, kept);
        Ok(answer)
    }
}

/// The query entry of `user.repos`: an error here is a Build error, and the call is not sent.
pub fn checked_repo_kind(
    kind: &Option<UserRepoType>, visibility: &Option<Visibility>, affiliation: &Option<Vec<String>>,
) -> Result<Option<UserRepoType>, &'static str> {
    if kind.is_some() && (visibility.is_some() || affiliation.is_some()) {
        return Err("`type` cannot go with `visibility` or `affiliation`");
    }
    Ok(kind.clone())
}

/// A repository that was renamed or moved: GitHub answers `301` with its new place. This follows
/// at most four moves, on the same origin only, and keeps the generated call (its reader, its
/// middleware, its credentials).
pub async fn get_moved(github: &Github, owner: &str, name: &str)
    -> Result<Repository, Box<dyn std::error::Error + Send + Sync>>
{
    let mut url = github.repo(owner, name).get().request()?.url().clone();
    for _ in 0..4 {
        let target = url.clone();
        let result = github.repo(owner, name).get()
            .edit_request(move |request| *request.url_mut() = target.clone())
            .await;
        match result {
            Err(error) if error.is_status(301) => {
                let location = error.headers().and_then(|h| h.get("location"))
                    .ok_or("a 301 with no Location")?;
                let next = url.join(location)?;
                if next.origin() != url.origin() {
                    return Err("the repository moved to another origin".into());
                }
                url = next;
            }
            other => return Ok(other?),
        }
    }
    Err("more than four moves".into())
}
```

A search stops at 1,000 results, and `incomplete_results` says that GitHub stopped looking: following every `Link` is then not every match. That is why the search keeps its envelope, `SearchPage<T>`, with `IntoItems` for `.pages().items()`.

## Calling it

```rust
use concord::Body;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A client, and every open bug of a repository, all pages.
pub async fn bugs() -> Result<Github, BoxError> {
    let quota = QuotaLog::default();
    let github = Github::new("my-app/1.0", quota.clone()).token(std::env::var("GITHUB_TOKEN")?);
    let mut issues = github.repo("rust-lang", "rust").issues().list()
        .state(IssueState::Open).labels(["C-bug"]).per_page(100)
        .pages().items();
    while let Some(issue) = issues.try_next().await? {
        println!("#{} {}", issue.number, issue.title);
    }
    Ok(github)
}

/// The repository again, only if it changed since its ETag. `None`: it did not.
pub async fn refresh(github: &Github, etag: &str) -> github::Result<Option<Repository>> {
    let answer = github.repo("rust-lang", "rust").get().send_if_none_match(etag).await?;
    Ok(answer.into_value())
}

/// Read the diff of a pull request, then merge exactly the commit that was read.
pub async fn merge(github: &Github, reviewed_sha: String) -> github::Result<()> {
    let pull = github.repo("octocat", "hello-world").pulls().pull(42);
    let diff = pull.diff().await?;
    println!("{} bytes of diff", diff.len());
    let merged = pull.merge(MergePull {
        sha: Some(reviewed_sha),
        merge_method: Some("squash".into()),
        ..Default::default()
    }).await?;
    println!("{}", merged.message);
    Ok(())
}

/// A file of a release, sent to the upload host.
pub async fn upload(github: &Github, zip: Vec<u8>) -> github::Result<()> {
    let releases = github.repo("octocat", "hello-world").releases();
    let release = releases.by_tag("v1.0.0").await?;
    let size = zip.len() as u64;
    let asset = releases.release(release.id)
        .asset("app.zip", Body::new(zip, "application/zip"), size)
        .label("Linux build")
        .await?;
    println!("{}", asset.browser_download_url);
    Ok(())
}

/// A search, page by page; then what is left of the search quota.
pub async fn search(github: &Github, quota: &QuotaLog) -> github::Result<()> {
    let mut pages = github.search().issues("repo:rust-lang/rust is:open label:C-bug")
        .per_page(100).pages();
    while let Some(page) = pages.try_next().await? {
        if page.incomplete_results {
            eprintln!("GitHub did not look everywhere");
        }
        for issue in page.items {
            println!("{}", issue.title);
        }
    }
    let log = quota.lock().unwrap_or_else(|e| e.into_inner());
    let left = log.get("search").and_then(|h| h.get("x-ratelimit-remaining"));
    println!("search quota left: {left:?}");
    Ok(())
}
```

## What fits badly, and the way out

| GitHub wants | What to write |
|---|---|
| A repository that was renamed answers `301`, and asks the client to follow it. Concord follows no redirect: the call fails with the `301` and its `Location`. | `get_moved` above, which sends the same call to the new URL. Or a `reqwest::Client` of yours that follows redirects, given to `with_transport` (see [when Concord does not say what you need](../guide/extending.md)). |
| A list in one query value, joined by commas: `labels=bug,ui`. A `Vec` sends its key once for each item. | `"labels" = labels.as_ref().map(\|v\| v.join(","));` on an optional input: an input that was not set sends nothing. |
| Filters that cannot go together (`type` with `visibility`). The language says only "required" or "optional". | A function in the entry that returns a `Result`: an `Err` is a Build error, and nothing is sent (`checked_repo_kind`). |
| A field cleared with `null`, apart from a field left out. | An input `milestone?: Option<u64>` in a body of entries: not set is left out, `.milestone(None::<u64>)` writes `null`, a number writes the number. |
| One resource in two shapes: JSON, or the raw bytes of a file, or the diff of a pull request. The type after the arrow is fixed. | One endpoint for each shape, with its own `accept` (`override "accept" = …;`) and its own reader. |
| The upload URL that a release gives, a URI template with a query. It is not a `Base`. | The documented route on the upload host: `base "https://uploads.github.com";` on that endpoint, and the token written again under it. |
| The root folder of a repository: `contents/` with an empty path. An empty `{*path}` is refused. | A second endpoint, `contents_root`. |
| `type` and `ref`, which are Rust keywords. | Another name in the signature, and the key in the entry: `"type" = kind;`, `"ref" = reference;`. |
