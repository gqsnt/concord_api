# concord-sdk-gitlab

A slice of the [GitLab REST API v4](https://docs.gitlab.com/api/rest/) written with
`concord::api!`: page through a project's issues, and retrieve one issue with its project.
Tested offline against hand-authored fixtures (`cargo test -p concord-sdk-gitlab`); it has never
been run against a GitLab instance.

## What it covers

| Operation | Endpoint | Generated method |
|---|---|---|
| List project issues (one page) | `GET /projects/:id/issues` | `gitlab.project(p).issues()` (+ filter setters) |
| Every page of that listing | same, then `Link: rel="next"` | `.issues().pages()` / `.items()` |
| Retrieve a project issue | `GET /projects/:id/issues/:issue_iid` | `gitlab.project(p).issue(iid)` |
| Retrieve a project | `GET /projects/:id` | `gitlab.project(p).get()` |
| Workflow B: an issue and its project | the two above | `gitlab.issue_context(p, iid)` |

`:id` is a `ProjectRef`: a project ID (`ProjectRef::id(4021)`) or the full path as GitLab shows it
(`ProjectRef::path("acme/tools/widgets")`), sent as **one** URL-encoded path segment
(`/projects/acme%2Ftools%2Fwidgets/issues`), as the REST documentation requires for namespaced
paths. Pass the path unencoded: a `%` is refused, because it would be encoded twice.

Listing filters: `state`, `labels` (all of, `Any`, `None`), `milestone`, `search`,
`author_username`, `order_by`, `sort`, `per_page` (1–100).

**Excluded:** Git operations (repository files, branches, commits), merge requests, every other
resource, writes, other API versions, GraphQL, keyset pagination, OAuth and token creation,
anonymous access, rate-limit waiting, retries.

## Construction

```rust,no_run
use concord_sdk_gitlab::{GitLab, PrivateToken};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
// Transport settings are yours. `redirect_policy()` keeps `PRIVATE-TOKEN` on the API origin.
let http = concord::reqwest::Client::builder()
    .redirect(concord_sdk_gitlab::redirect_policy())
    .timeout(std::time::Duration::from_secs(30))
    .build()?;
let token = PrivateToken::new(std::env::var("GITLAB_TOKEN")?)?; // refused if empty or padded
let gitlab = GitLab::with_token(http, token); // https://gitlab.com/api/v4
// A self-managed instance:
let gitlab = gitlab.with_base_url("https://gitlab.example.com/api/v4")?;
# let _ = gitlab; Ok(()) }
```

- The token (personal, project or group access token) is sent as `PRIVATE-TOKEN`, GitLab's
  recommended header (lowercase on HTTP/1.1). It is `#[sensitive]` and flagged sensitive:
  Concord never prints it, and `PrivateToken`'s `Debug` is `PrivateToken(<redacted>)`.
- **Redirects.** GitLab redirects `GET` requests that use the old path of a moved project.
  reqwest's default redirect policy follows a redirect to any host and forwards custom headers,
  `PRIVATE-TOKEN` included (the control test
  `control_the_default_redirect_policy_forwards_the_token` shows it). Build your client with
  `redirect_policy()` (same-origin redirects only; a cross-origin one is returned as a `3xx`
  error) or with `redirect::Policy::none()`.
- `GitLab::new(http, GitLabConfig { token })` and `set_token` are the generated equivalents and
  take a `HeaderValue`; `PrivateToken::into_header_value()` produces one for a token rotation.

## Workflow A: page through a project's issues

```rust,no_run
use std::num::NonZeroUsize;
use concord_pagination::{PagesError, Resume};
use concord_sdk_gitlab::{GitLab, IssueState, Labels, PerPage, ProjectRef};

# async fn run(gitlab: GitLab) -> Result<(), Box<dyn std::error::Error>> {
let listing = gitlab
    .project(ProjectRef::path("acme/tools/widgets")?)
    .issues()
    .set_state(IssueState::Opened)
    .set_labels(Labels::all_of(["bug"])?)
    .set_per_page(PerPage::new(50)?);

// Page by page, with each page's metadata.
let mut pages = listing.clone().pages().max_pages(NonZeroUsize::new(20).unwrap());
loop {
    match pages.next_page().await {
        Ok(Some(page)) => println!(
            "{} issues (page {:?} of {:?}, request {:?})",
            page.items.len(),
            page.meta.pagination().page,
            page.meta.pagination().total_pages, // None over 10,000 issues
            page.meta.request_id(),
        ),
        Ok(None) => break, // the last page: GitLab sent no rel="next"
        Err(PagesError::Bounded { resume, .. }) => {
            // 20 pages read and GitLab links on: continue later from `resume`.
            let rest = listing.clone().pages_from(resume).items().collect_complete().await?;
            println!("{} more", rest.len());
            break;
        }
        Err(error) => return Err(error.into()), // a failed or refused page
    }
}

// Or everything at once: `Ok` only after the last page.
match listing.clone().items().collect_complete().await {
    Ok(issues) => println!("{} open bugs", issues.len()),
    Err(incomplete) => {
        // A prefix and the reason; `resume()` is the page to retry.
        println!("{} read before: {}", incomplete.collected.len(), incomplete.error);
        if let Some(Resume::At(page)) = incomplete.error.resume().cloned() {
            let _retry = listing.pages_from(Resume::At(page));
        }
    }
}
# Ok(()) }
```

**Pinned pagination strategy.** GitLab documents offset-based pagination (the REST default) and,
for this endpoint from GitLab 18.3, keyset-based pagination. This SDK pins **offset-based**
pagination for `GET /projects/:id/issues` and does not claim keyset pagination for it or any
other endpoint. The first request is the listing as configured (no `page` parameter). Each later
request goes to the previous response's `rel="next"` URL **exactly as GitLab sent it**: the SDK
never builds a page URL, never computes one from `x-next-page`, `x-total-pages` or `rel="last"`,
and never adds its own `per_page` to a link. If GitLab's link carries another page size, another
parameter order or a cursor, that is what is requested (`a_follows_the_returned_link_whatever_its_representation`).
The wire test `changed_link_query_survives_cap_and_failed_retry` also checks repeated query keys,
empty values and encoded cursor bytes after both a cap and a failed-page retry. Its page sizes and
query are synthetic examples of the contract, not a new GitLab default or cursor format.

Rules applied to every page (`src/pages.rs`, each with a test):

- Linked pages are the same generated endpoint, with the same `PRIVATE-TOKEN` and `Accept`
  headers, sent to GitLab's URL.
- The link must be on the API's origin (scheme, host, port), or it is not requested and the
  traversal stops (`PaginationProblem::ForeignLink`): the token never goes elsewhere. It must
  name a project issue collection under the API base path (`.../projects/<id or path>/issues`),
  or it is `OtherCollection`.
- `Link` is read before the body. A malformed header is an error (`MalformedLink`), never "last
  page". A `next` equal to the URL just requested is `NotAdvancing`. No `next` while
  `x-next-page` names a page is `MissingNextLink`. When `x-next-page` and `Link` disagree, `Link`
  wins.
- Each page keeps its `ResponseMeta`: `x-request-id`, `x-page`, `x-per-page`, `x-prev-page`,
  `x-next-page`, `x-total`, `x-total-pages` (each `None` when absent or empty), and which `Link`
  relations were present.

## Workflow B: an issue and its project

```rust,no_run
use concord_sdk_gitlab::{ContextError, GitLab, IssueIid, ProjectRef};

# async fn run(gitlab: GitLab) -> Result<(), Box<dyn std::error::Error>> {
match gitlab
    .issue_context(ProjectRef::path("acme/tools/widgets")?, IssueIid::new(2)?)
    .await
{
    Ok(context) => println!(
        "{} ({}) in {} [default branch {:?}]",
        context.issue.value.title,
        context.issue.value.web_url,
        context.project.value.path_with_namespace,
        context.project.value.default_branch,
    ),
    Err(ContextError::Issue(error)) if error.is_not_found() => println!("no such issue"),
    Err(ContextError::Project { issue, error }) => {
        println!("issue {} retrieved, project not: {error}", issue.value.iid)
    }
    Err(error) => return Err(error.into()),
}
# Ok(()) }
```

Two requests, in order: the issue by the given project reference and its IID (the
project-scoped number, `IssueIid`, not the global `IssueId`), then `GET /projects/:project_id`
with the ID the issue reports. Addressing the project by that ID means a path renamed or
reassigned between the two requests cannot yield another project; a returned project with a
different ID is refused (`ContextError::Mismatch`).

## Single calls and errors

Every generated operation's `send()` returns this SDK's error and the response metadata,
`Result<Response<T>, GitLabError>` (the client declares `send SendPolicy;`; nothing to import).
`prepare()` gives Concord's native request (`concord::Error`; its own `prepare()?.send()` has no
GitLab error document):

```rust,no_run
use concord_sdk_gitlab::{ErrorBody, GitLab, GitLabError, IssueIid, ProjectRef};

# async fn run(gitlab: GitLab) -> Result<(), Box<dyn std::error::Error>> {
let project = gitlab.project(ProjectRef::id(4021)?);
match project.issue(IssueIid::new(2)?).send().await {
    Ok(issue) => println!("{} (request {:?})", issue.value.title, issue.meta.request_id()),
    Err(GitLabError::Api(api)) => {
        // Non-2xx: status, GitLab's `message` if the body was its error document, metadata.
        println!("{} {:?} retry after {:?}", api.status, api.message(), api.meta.retry_after());
        if let ErrorBody::Unrecognized(bytes) = &api.body {
            println!("{} bytes that are not a GitLab error document", bytes.len());
        }
    }
    Err(GitLabError::Request { error, meta }) => {
        // Concord: Build (nothing sent), Transport, Metadata (a 2xx not labelled JSON), Decode.
        println!("{:?} (response arrived: {})", error.kind(), meta.is_some());
    }
    Err(error) => return Err(error.into()),
}
# Ok(()) }
```

- `GitLabError::Api` keeps the status, the operation, the response metadata (request id,
  `Retry-After`) and the body: `Document` (`{"message": ...}`, a text or a validation object),
  `Unrecognized(bytes)` (an HTML proxy page, plain text, other JSON), `TooLarge` (over 64 KiB; the
  rest is not read) or `Unreadable`.
- `GitLabError::Request` is Concord's error with the metadata once a response head arrived.
- `GitLabError::Pagination` is a broken pagination contract (above).
- `Display` names the operation and the stage, never a URL, the token or a body text; GitLab's
  message is read with `ApiError::message()`. `ResponseMeta`'s `Debug` never prints a URL.
- Inputs the SDK can refuse locally (an unnamespaced or pre-encoded project path, a zero ID,
  `per_page` outside 1–100, a label with a comma, an empty or padded token) are `InputError`s at
  construction: nothing is sent.

## Completion and partial outcomes

- One `send()` is one request; nothing is retried.
- A traversal is complete only when `next_page()` returns `Ok(None)` or `collect_complete()`
  returns `Ok`. A cap is `PagesError::Bounded { resume }`; a failed or refused page is
  `PagesError::Provider { error, failed }` with the `GitLabError` and the page to retry;
  `collect_complete()` returns `Incomplete { collected, error }`. A `None` after an `Err` from an
  item stream is not completion.
- `pages_from(resume)` continues a listing from a `NextPage` of **that** listing (same project
  reference and collection); one from another listing is refused before anything is sent. The
  link's own query carries GitLab's filters; it is not compared with the listing's setters.
- Workflow B returns the issue in `ContextError::Project` and `ContextError::Mismatch`; with
  `ContextError::Issue` nothing else was requested.

## Limitations

- **Live listing.** Offset pages of a changing project can skip or repeat issues; a traversal is
  a best-effort enumeration, not a snapshot. GitLab recommends keyset pagination for consecutive
  pages on 18.3 and later; this SDK does not offer it (it would need `pagination=keyset` with the
  keyset `order_by` values and an instance version check).
- **Offset limits.** GitLab documents a maximum offset for offset-based pagination (configurable
  on self-managed instances). The SDK does not predict it; whatever GitLab answers beyond it is
  returned as is (a non-2xx page is `PagesError::Provider`).
- **External URL.** The next link must be on the origin of the configured base URL. An instance
  whose links name another host than the one you connect to (an internal address, a proxy) stops
  the traversal with `ForeignLink` instead of sending the token there.
- **Continuations** live in memory (`NextPage`); there is no serialized form.
- Response models keep the fields listed in `src/models.rs`; unknown fields are ignored and
  timestamps are kept as the strings GitLab sent.
- `Retry-After` is read as seconds only.

## Fixtures are not conformance

The fixtures in `fixtures/` are hand-authored from the documented field lists and examples, with
synthetic values (`PROVENANCE.md` says which are documented and which are invented). Passing
these tests shows that the SDK handles those bytes as described; it does not show that GitLab
sends them, and no test contacted GitLab.
