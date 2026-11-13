# the GitHub REST API in Concord

Written by GPT-6 (Codex), from the Concord V5 guide, the first writer's endpoint inventory and provider facts, and GitHub's public REST documentation, on 2026-11-12.

## The description

This targets public GitHub and pins the version shown in the current documentation, `2026-03-10`. The forty-two declarations preserve the earlier inventory, including its conditional alias. No request was sent to GitHub. The [repository documentation](https://docs.github.com/en/rest/repos/repos) supplies repository filters; [repository issues](https://docs.github.com/en/rest/issues/issues#list-repository-issues) supplies all thirteen list parameters, including paging, `type` and `issue_field_values`. The latter requires issue fields to be enabled.

```rust
use bytes::Bytes;
use concord::{by_link, json, Binary, Json, Secret};
use crate::model::{
    Asset, Comment, ContentEntry, Contents, ContentWrite, ContributorStats,
    Direction, EditIssue, GithubProblem, Issue, IssueSort, IssueState,
    MergePull, MergeResult, NewIssue, NewPull, NewRelease, NewRepository,
    OrgRepoType, PublicRepoType, Pull, PullFile, PullSort, PutContent,
    RateLimits, Release, Repository, RepoSort, User, UserRepoType, Visibility,
};

concord::api! {
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
        /// Records rate-limit headers on every response, including each page and try.
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
            /// kind cannot accompany visibility or affiliation.
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
            /// A renamed repository's 301 remains an error with Location; see get_with_moves.
            GET get() -> Repository;
            /// Compatibility alias for this inventory; get().send_if_none_match(etag) also works.
            GET get_if_changed(etag: String) -> Option<Repository> {
                headers { "if-none-match" = etag; }
                status { 304 => None }
            }
            /// None means calculation pending, not an absent repository.
            GET contributor_stats() -> Option<Vec<ContributorStats>> {
                path "stats/contributors";
                status { 202 => None, 204 => Some(Vec::new()) }
            }
            /// A root directory is not a paginated list.
            GET contents_root(..at_ref) -> Vec<ContentEntry> { path "contents"; }

            scope issues {
                path "issues";
                /// Includes pull requests. milestone/assignee/kind accept "*" and "none".
                /// since is UTC ISO 8601; labels is an AND filter.
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
                    PATCH update(body: String) -> Comment {
                        body json(serde_json::json!({ "body": body }));
                    }
                    DELETE delete();
                }

                scope issue(number: u64) {
                    path "{number}";
                    GET get() -> Issue;
                    PATCH update(changes: EditIssue) -> Issue { body json(changes); }
                    GET comments(since?: String, ..page) -> Vec<Comment> {
                        path "comments";
                        query { since; }
                    }
                    POST create_comment(body: String) -> Comment {
                        path "comments";
                        body json(serde_json::json!({ "body": body }));
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
                    /// 404 here is GitHub's negative answer; inaccessible resources can also hide.
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
                /// Files, including symlinks resolved to files; bytes need not be UTF-8.
                GET raw(..at_ref) -> Bytes {
                    headers { override "accept" = "application/vnd.github.raw+json"; }
                    read Binary.limit(100 * concord::MIB);
                }
                /// content is base64; sha is required when replacing an existing file.
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
                    /// Body carries its media type; size also permits a known-length stream.
                    POST asset(
                        name: String, file: concord::Body, size: u64, label?: String
                    ) -> Asset {
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
            /// q is unencoded GitHub search text. Sorting words belong to GitHub.
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

## Beside the macro

The model types are public, owned Rust values. Model inputs implement `Clone + Serialize`; responses implement `Deserialize + Send`. The following names specify the omitted, routine schemas rather than expanding GitHub's large objects:

| Types | Shape or wire rule |
|---|---|
| `Repository`, `User` | GitHub repository and user objects; retain ids, names, owner and URLs, and optional fields such as repository description. |
| `Issue`, `Comment` | Issue/comment objects. `Issue` retains `pull_request` as an optional object so lists and searches can distinguish pulls. |
| `Pull`, `PullFile`, `MergeResult` | Pull object, changed-file object with optional patch, and `{ sha, merged, message }`. |
| `Release`, `Asset` | Release/asset objects, including `Release.upload_url` and asset media type, size and URLs. |
| `ContributorStats` | `total`, nullable `author`, and weekly `w/a/d/c` counters. |
| `RateLimits` | `resources` map from resource name to limit/remaining/used/reset counters, plus the compatibility `rate` object. |
| `GithubProblem` | `message`, optional `documentation_url` and `status`, optional heterogeneous `errors: Vec<serde_json::Value>`. |
| `ContentEntry`, `Contents` | Entry object; untagged union of one entry and a directory's vector of entries. Entries retain type as extensible text, encoding, optional content, sha, links and symlink/submodule fields. |
| `ContentWrite` | GitHub's `content` metadata and `commit` object. |
| `NewRepository` | Required name; optional description, homepage, private/visibility, initialization/features, license/gitignore templates and organization team id. Omit unset fields; organization-only fields are caller responsibility. |
| `NewIssue` | Required title; optional body, assignees, labels, milestone and issue type. Omit unset fields. |
| `EditIssue` | Optional title, body, state, state_reason, labels, assignees, milestone and type; derives `Default`. Clearing nullable fields uses nested options; empty label/assignee vectors clear the lists. |
| `NewPull` | Required head/base; optional title or existing issue number, head_repo, body, draft and maintainer_can_modify. |
| `MergePull` | Optional commit_title, commit_message, sha and merge_method; derives `Default`. |
| `PutContent` | Required message and base64 content; optional sha, branch, committer and author. |
| `NewRelease` | Required tag_name; optional target_commitish, name, body, draft, prerelease, generate_release_notes, discussion_category_name and make_latest. |
| `Direction`, `RepoSort` | `asc/desc`; `created/updated/pushed/full_name`. |
| `Visibility`, `UserRepoType` | `all/public/private`; `all/owner/public/private/member`. |
| `PublicRepoType`, `OrgRepoType` | `all/owner/member`; `all/public/private/forks/sources/member`. |
| `IssueState`, `IssueSort`, `PullSort` | `open/closed/all`; `created/updated/comments`; `created/updated/popularity/long-running`. |

Request fields use their provider spelling through serde, including Rust `kind` fields renamed to `type`. Unset optional request fields have `skip_serializing_if = "Option::is_none"`. In `EditIssue` a clearable field such as `milestone` has `Option<Option<u64>>`: outer `None` omits it, `Some(None)` writes null. `body` and `type` use the corresponding nested string options. Response enums must retain unknown values. These routine DTO definitions are deliberately not expanded.

The JSON/raw split and the directory union follow [GitHub's contents media types](https://docs.github.com/en/rest/repos/contents). Raw files can be up to the provider's 100 MB limit; the reader's 100 MiB bound accommodates that. Directories have a 1,000-entry ceiling, without `Link` paging. Exact submodule type tagging under the pinned version is (unsure); retaining the text and submodule fields avoids assuming that it is always `submodule`.

Only the search DTO is expanded. `incomplete_results` and the provider's 1,000-result search ceiling mean that exhausting links is not proof of exhaustive matching. The caller below keeps the envelope. [GitHub search documentation](https://docs.github.com/en/rest/search/search)

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SearchPage<T> {
    pub total_count: u64,
    pub incomplete_results: bool,
    pub items: Vec<T>,
}
```

The shared log records the latest observed head for each rate resource, such as `core` or `search`. It records primary quota counters and `Retry-After` on success, refusal, every retry and every page. The application owns scheduling and can inspect its clone without changing any endpoint's return type. GitHub's primary and secondary limits have different signals; `403` alone is insufficient to identify a rate limit. [GitHub rate-limit headers and handling](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api)

```rust
use std::{collections::BTreeMap, sync::{Arc, Mutex}};
use http::{HeaderMap, header::HeaderName};
use concord::{Middleware, Next, TransportError, TryInfo};

pub type QuotaLog = Arc<Mutex<BTreeMap<String, HeaderMap>>>;

pub struct ObserveQuota(pub QuotaLog);

impl Middleware for ObserveQuota {
    async fn handle(
        &self, request: reqwest::Request, info: TryInfo, next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        let response = next.send(request, info).await?;
        let mut fields = HeaderMap::new();
        for key in [
            "x-ratelimit-limit", "x-ratelimit-remaining", "x-ratelimit-used",
            "x-ratelimit-reset", "x-ratelimit-resource", "retry-after",
        ] {
            if let Some(value) = response.headers().get(key) {
                fields.insert(HeaderName::from_static(key), value.clone());
            }
        }
        let resource = response.headers().get("x-ratelimit-resource")
            .and_then(|v| v.to_str().ok()).unwrap_or("unknown").to_owned();
        self.0.lock().unwrap_or_else(|p| p.into_inner()).insert(resource, fields);
        Ok(response)
    }
}
```

The entry function rejects an invalid combination before sending. Pagination sizes are documented as at most 100; their numeric validation remains GitHub's.

```rust
pub fn checked_repo_kind(
    kind: &Option<UserRepoType>, visibility: &Option<Visibility>,
    affiliation: &Option<Vec<String>>,
) -> Result<Option<UserRepoType>, std::io::Error> {
    if kind.is_some() && (visibility.is_some() || affiliation.is_some()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "type cannot accompany visibility or affiliation",
        ));
    }
    Ok(kind.clone())
}
```

[GitHub asks clients to follow a permanent move](https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api#follow-redirects). Concord leaves `301` as a Refused error with its `Location`. An arm cannot await the next request. This helper keeps the generated reader, middleware and credentials, follows at most four moves within the original origin, and returns the final repository object. It also honors a caller's `with_base` because it starts with the built URL. Its own errors use a surrounding Rust error type, since the author cannot construct `github::Error`.

```rust
pub async fn get_with_moves(
    github: &Github, owner: &str, name: &str,
) -> Result<Repository, Box<dyn std::error::Error + Send + Sync>> {
    let mut url = github.repo(owner, name).get().request()?.url().clone();
    let origin = url.origin();
    for moves in 0..=4 {
        let target = url.clone();
        let result = github.repo(owner, name).get()
            .edit_request(move |request| *request.url_mut() = target.clone()).await;
        match result {
            Ok(repo) => return Ok(repo),
            Err(error) if error.is_status(301) && moves < 4 => {
                let location = error.headers().and_then(|h| h.get("location"))
                    .ok_or("301 without Location")?;
                let next = url.join(location)?;
                if next.origin() != origin || !next.username().is_empty()
                    || next.password().is_some() {
                    return Err("repository move leaves the original origin".into());
                }
                url = next;
            }
            Err(error) => return Err(error.into()),
        }
    }
    unreachable!("the final iteration returns")
}
```

A `202` from [contributor statistics](https://docs.github.com/en/rest/metrics/statistics#get-all-contributor-commit-activity) becomes `None`; `204` becomes an empty vector wrapped in `Some`. A caller schedules the next poll. Ordinary missing or inaccessible repositories remain errors. The [pull request documentation](https://docs.github.com/en/rest/pulls/pulls) supplies diff negotiation, the merge check's `204/404`, and merge options. Both merge and content replacement opt out of automatic repetition because success changes the preconditions of a later attempt.

The upload endpoint uses the documented public-host route with the release id. GitHub recommends using the release's `upload_url`, a URI template ending with query variables; that is not a `Base`. For this fixed public API I write its documented route. A changed provider-supplied upload route would need Rust URL-template expansion and `edit_request`, with origin checks before carrying credentials. The description adds the already inherited `repos/{owner}/{name}/releases/{id}` path after its new base and deliberately writes authorization again. `Body` supplies `Content-Type`; `size` supplies the required `Content-Length` for a stream as well as bytes. [GitHub release asset upload documentation](https://docs.github.com/en/rest/releases/assets#upload-a-release-asset)

## Calling it

These six snippets share the client and repository handle constructed in the first snippet. They belong inside async application functions returning a surrounding application error; nothing here was executed. Snippet 2 starts with a cached repository and `cached_etag: Option<String>` containing its validator. The application supplies the reviewed head SHA, an immutable content commit SHA, and the artifact paths.

1. Triage every matching issue, including pulls if GitHub returns them.

```rust
let quota = QuotaLog::default();
let github = Github::new("concord-example/0.1", quota.clone())
    .token(std::env::var("GITHUB_TOKEN")?);
let repo = github.repo("octocat", "Hello-World");
let mut issues = repo.issues().list().state(IssueState::All)
    .milestone("none").assignee("*").labels(["bug", "ui"])
    .since("2026-11-01T00:00:00Z").per_page(100).pages().items();
while let Some(issue) = issues.try_next().await? {
    println!("{} {}", issue.number, issue.title);
}
```

2. Keep a cached repository when its ETag still matches, and handle a renamed repository.

```rust
match repo.get().send_if_none_match(cached_etag.as_deref().expect("cached ETag")).await {
    Ok(answer) => {
        let etag = answer.headers().get("etag").map(str::to_owned);
        if let Some(value) = answer.into_value() {
            cached_repo = value;
            cached_etag = etag;
        } else if etag.is_some() { cached_etag = etag; }
    }
    Err(error) if error.is_status(301) => {
        cached_repo = get_with_moves(&github, "octocat", "Hello-World").await?;
        cached_etag = None;
    }
    Err(error) => return Err(error.into()),
}
```

3. Inspect a pull's bytes and merge only the reviewed head.

```rust
let pull = repo.pulls().pull(42);
let diff = pull.diff().await?;
std::fs::write("review.diff", &diff)?;
let merged = pull.merge(MergePull {
    sha: Some(reviewed_sha), merge_method: Some("squash".into()),
    ..Default::default()
}).await?;
println!("{}", merged.message);
```

4. Read metadata and raw bytes at the same immutable revision.

```rust
let file = repo.contents("src/lib.rs");
let metadata = file.get().reference(&commit_sha).await?;
let raw = file.raw().reference(&commit_sha).await?;
```

5. Upload a build artifact to an existing release on the upload host.

```rust
let release = repo.releases().by_tag("v1.0.0").await?;
let bytes = std::fs::read("build/app.zip")?;
let size = bytes.len() as u64;
let asset = repo.releases().release(release.id)
    .asset("app.zip", concord::Body::new(bytes, "application/zip"), size)
    .label("Linux build").await?;
println!("{}", asset.browser_download_url);
```

6. Search while retaining incompleteness information and inspect the observed search quota.

```rust
let mut pages = github.search().issues("repo:octocat/Hello-World is:open")
    .per_page(100).pages();
while let Some(page) = pages.try_next().await? {
    if page.incomplete_results { eprintln!("GitHub returned incomplete search results"); }
    for issue in page.items { println!("{}", issue.title); }
}
let observed = quota.lock().unwrap_or_else(|p| p.into_inner());
let remaining = observed.get("search").and_then(|h| h.get("x-ratelimit-remaining"));
println!("observed search quota: {remaining:?}");
```

## Findings

| # | where | what got in the way | what you wrote instead | kind |
|---|---|---|---|---|
| 1 | A renamed repository, `301` | GitHub asks for another request; the engine follows none and an arm cannot await. Handling it moves outside the generated error type and loses the concise conditional-call shape. | The normal error retains Location; `get_with_moves` builds and checks subsequent URLs, then reuses the typed call. | cannot be said |
| 2 | Large issue/repository signatures | Each input is declared and then named again just to place it. I had to compare the long issue signature and query block to check coverage. | `page` and `repo_order` parts share what repeats across endpoints; issue filters still have explicit entries. | said with pain |
| 3 | `labels` and `affiliation` | A vector repeats a query key by default; GitHub wants comma lists. Optional joining requires reaching through the option, and an empty vector sends an empty value. | Two `as_ref().map(\|v\| v.join(","))` expressions; callers omit a filter to leave it out. | said with pain |
| 4 | `user.repos` filter combinations | Independent setters admit `type` together with visibility/affiliation. No declarative constraint says that the combination is invalid. | A fallible entry function checks the combination and produces a Build error. | said with pain |
| 5 | `PATCH` issue fields | Builder optionality does not provide JSON omission; a null can clear a value. Keeping these straight requires leaving the signature and reading serde rules. | `EditIssue` with outer-option omission and nested options for explicit null. | said with pain |
| 6 | Quota headers across pages | Page iteration yields values without heads. Keeping the headers requires middleware, shared locking state and an extra constructor argument solely so the caller can retain that state. | `QuotaLog` and `ObserveQuota`, declared once on the client. | said with pain |
| 7 | JSON contents, raw contents and pull diffs | The arrow is fixed. Request negotiation cannot turn one call from a JSON value into bytes. | Separate typed operations and explicit Accept overrides; `Json` does not negotiate for me. | said with pain |
| 8 | Release asset upload | The provider supplies a full URL template, which cannot replace the accumulated paths through `base`. I also reread the host/secret rules to confirm which inherited pieces survive. | Endpoint `base` plus an explicit credential override; a fixed documented route instead of interpreting the upload URL template. | said with pain |
| 9 | Repository root contents | An empty `{*path}` is rejected, even though omitting the provider's optional path selects the root. | The inventory's separate `contents_root` endpoint; no fake empty path. | small |
| 10 | `type` and `ref` | Provider words are Rust keywords, so the public setter names differ from the keys in the docs. | `kind` and `reference`, with literal wire keys. | small |
| 11 | Conditional alias in the inventory | The guide already supplies the conditional call. Preserving this forty-two-entry inventory means describing the same wire operation again. | `get_if_changed` plus its explicit header/304 arm; new calling code uses `send_if_none_match`. | small |
| 12 | Search result envelope | The guide says to implement `IntoItems`, but does not show that trait's method signatures in the permitted reading. I could not honestly write its implementation from this guide alone. | Keep `SearchPage<T>` and loop through `page.items`; `by_link` works on the envelope without that trait. | said with pain |

- **1 — wished:** a described same-origin permanent-move rule that resends the typed call and retains its normal error and conditional-response behavior.
- **2 — wished:** place a group of declared inputs in query with one rest entry, while naming only exceptions individually.
- **3 — wished:** one ordinary optional comma-list adapter reusable in entries, including an explicit empty-list policy.
- **4 — wished:** describe the invalid filter combination beside the inputs once, and have the generated call reject it before sending.
- **5 — wished:** express omitted versus null request fields once where the caller's edit options are described.
- **6 — wished:** a concise way to observe every page's head while retaining the typed page iterator.
- **7 — wished:** select a media representation and its reader/result type together without copying an endpoint declaration.
- **8 — wished:** use the release's upload URL template directly while keeping typed upload responses and explicit credential scoping.
- **12 — wished:** the complete `IntoItems` trait contract beside the instruction to implement it.

## Measures

Counts below concern the text as written; spreading a part does not count its declaration or placement again. A placement repetition is a reference to an input in `base`/`path`, a query/header value, or a body expression solely to send it. A short entry counts once; fixed text and semantic use in validation do not count. The upload's second credential placement counts again. Spread-expanded counts are also supplied to expose the savings from parts.

- Endpoints: **42** declarations, including the conditional alias and representation variants.
- Lines of `concord::api!`: **224**, inclusive of its opening and closing lines, blank lines and comments; excludes use lines and fences.
- Input names written again only for placement: **64** occurrences; **64** declared inputs in all (client, parts, scopes and endpoints, each declaration once).
- After expanding parts at their eleven paging, three ordering and three reference use sites: **90** input declarations and **90** placement occurrences.
- Rust beside the macro solely for gaps: **63** nonblank lines: quota observation **25**, cross-input validation **12**, following repository moves **26**. This includes their imports and declarations; excludes the routine search DTO, all omitted model schemas, code fences and calling examples.
- Whole document: **503** lines. No build, compilation, test execution or GitHub API call was run.

## Scores

- **Writing this description: 7/10.** The URL tree and parts carry most of the API cleanly, but placement bookkeeping and response observation interrupt the flow.
- **Reading it back a month later: 8/10.** The endpoint signatures and local exceptions are readable; the quota and redirect behavior require following the Rust beside them.
- **The calling code: 8/10.** Paging and conditionals say what the caller intends, while nested resource handles and permanent moves make a few common tasks longer.

