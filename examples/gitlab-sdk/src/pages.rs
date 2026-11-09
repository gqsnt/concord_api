//! Paging through a project's issues by GitLab's `Link` header.
//!
//! **Pinned strategy.** `GET /projects/:id/issues` documents both offset-based pagination (the
//! REST API's default) and keyset-based pagination (`pagination=keyset`, from GitLab 18.3). This
//! SDK pins **offset-based** pagination for this endpoint: the first request is the generated
//! listing as configured (no `page` parameter; `per_page` if set), and every later request goes to
//! the `rel="next"` URL of the previous response, **exactly as GitLab sent it**. The SDK never
//! builds a page URL, never reads `x-next-page` to compute one, and never uses `rel="last"`. It
//! does not claim keyset pagination for this or any other endpoint; a server whose `next` link
//! carries a keyset cursor instead of a page number is followed all the same, because the link is
//! authoritative. The query is not decoded and rebuilt: its ordering, repeated parameters and
//! percent-encoded values also survive a cap or failed-page continuation.
//!
//! **Rules, each tested:**
//!
//! - **Same endpoint, GitLab's URL.** A linked page is the same generated endpoint (same
//!   `PRIVATE-TOKEN` and `Accept` headers, same decoder, same Concord operation name in errors)
//!   sent to the link through the client's send policy (`Follow::follow`, which also refuses
//!   another origin itself).
//! - **The token stays on the API.** A link on another origin (scheme, host, port) is
//!   [`PaginationProblem::ForeignLink`] and is not requested. A link that is not a project issue
//!   collection under the API's base path (`.../projects/<one segment>/issues`) is
//!   [`PaginationProblem::OtherCollection`].
//! - **Continuation before items.** The `Link` header is checked before the page's items are
//!   returned: a malformed one is an error, never "last page". A `next` equal to the URL just requested is
//!   [`PaginationProblem::NotAdvancing`]. No `next` while `x-next-page` names a page is
//!   [`PaginationProblem::MissingNextLink`].
//! - **Metadata.** Every page keeps its [`ResponseMeta`] (request id, `x-page`, `x-total`, ...);
//!   `x-total` and `x-total-pages` are absent for listings over 10,000 issues.
//! - **End and failure.** `Ok(None)` from [`Pages::next_page`] only after a page without
//!   `rel="next"`. A cap ([`Pages::max_pages`], none by default) reached while GitLab links on is
//!   `PagesError::Bounded`; a failed or refused page is `PagesError::Provider` with the
//!   [`GitLabError`] and the page to retry. Neither is a silent end.
//!
//! GitLab's listing is live: issues created, closed or relabelled between requests can move
//! between offset pages (duplicated or skipped). A traversal is a best-effort enumeration, not a
//! snapshot.

use std::fmt;
use std::sync::Arc;

use concord::reqwest::Url;
use concord::{Follow, NextUrl};
use concord_pagination::{Page, PageItems, Pages, Resume};

use crate::api::GitLabProjectIssues;
use crate::error::{GitLabError, PaginationProblem};
use crate::meta::ResponseMeta;
use crate::models::Issue;

/// The page GitLab linked to (`rel="next"`): the key of the next request, and where an interrupted
/// traversal resumes. `Debug` withholds the URL.
#[derive(Clone, PartialEq, Eq)]
pub struct NextPage {
    url: Url,
    /// The path of the first-page request of the listing that produced this link.
    listing: Arc<str>,
}

impl NextPage {
    /// The URL as GitLab sent it (fragment removed), **unredacted**: it carries the listing's
    /// filters.
    pub fn raw_url(&self) -> &Url {
        &self.url
    }
}

impl fmt::Debug for NextPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NextPage(<withheld>)")
    }
}

/// One page of issues with its key and metadata.
pub type IssuePage = Page<Issue, NextPage, ResponseMeta>;

/// The page traversal of a project issue listing ([`GitLabProjectIssues::pages`]).
pub type IssuePages = Pages<Issue, NextPage, ResponseMeta, GitLabError>;

/// The item traversal of a project issue listing ([`GitLabProjectIssues::items`]).
pub type IssueItems = PageItems<Issue, NextPage, GitLabError>;

impl GitLabProjectIssues {
    /// Every page of this listing (filters as set), one request at a time, without a cap unless
    /// you set one. Nothing is sent until the first `next_page()`. See the
    /// [module documentation](self) for the rules.
    ///
    /// ```no_run
    /// use concord_sdk_gitlab::{GitLab, IssueState, PerPage, ProjectRef};
    ///
    /// # async fn run(gitlab: GitLab) -> Result<(), Box<dyn std::error::Error>> {
    /// let project = gitlab.project(ProjectRef::path("acme/widgets")?);
    /// let mut pages = project.issues().set_state(IssueState::Opened).set_per_page(PerPage::new(50)?).pages();
    /// while let Some(page) = pages.next_page().await? {
    ///     println!("{} issues; total {:?}", page.items.len(), page.meta.pagination().total);
    /// }
    /// # Ok(()) }
    /// ```
    pub fn pages(self) -> IssuePages {
        Pages::new(Resume::Start, move |key| fetch(self.clone(), key))
    }

    /// Every issue of this listing, one at a time (the [`pages`](Self::pages) rules).
    pub fn items(self) -> IssueItems {
        self.pages().items()
    }
}

/// Fetches the first page (`None`) or a linked page, both through the client's send policy.
async fn fetch(op: GitLabProjectIssues, key: Option<NextPage>) -> Result<IssuePage, GitLabError> {
    // The first page's URL: the API origin, the base path and this listing's collection path.
    let first = op.clone().prepare()?.request().url().clone();
    let listing: Arc<str> = first.path().into();
    let refused = |problem| GitLabError::Pagination {
        problem,
        meta: None,
    };
    let (page, requested) = match key {
        None => (op.send().await?, first.clone()),
        Some(key) => {
            if key.listing != listing {
                return Err(refused(PaginationProblem::ContinuationMismatch));
            }
            // A key only comes from a checked response, but the caller can pass any.
            check_link(&first, &key.url).map_err(refused)?;
            let mut requested = key.url.clone();
            requested.set_fragment(None);
            (
                op.follow(NextUrl::from_url(key.url)).send().await?,
                requested,
            )
        }
    };
    let next = match next_link(&first, &requested, &page.meta) {
        Ok(next) => next.map(|url| NextPage {
            url,
            listing: listing.clone(),
        }),
        Err(problem) => {
            return Err(GitLabError::Pagination {
                problem,
                meta: Some(Box::new(page.meta)),
            });
        }
    };
    Ok(Page {
        items: page.value,
        next,
        meta: page.meta,
    })
}

/// The checked `rel="next"` of a response, or `None` on the last page.
fn next_link(
    first: &Url,
    requested: &Url,
    meta: &ResponseMeta,
) -> Result<Option<Url>, PaginationProblem> {
    let links = meta.links().map_err(PaginationProblem::MalformedLink)?;
    let Some(next) = links.next() else {
        if meta.next_page_announced() {
            return Err(PaginationProblem::MissingNextLink);
        }
        return Ok(None);
    };
    let mut next = next.clone();
    next.set_fragment(None);
    check_link(first, &next)?;
    if next == *requested {
        return Err(PaginationProblem::NotAdvancing);
    }
    Ok(Some(next))
}

/// A link may be requested with the token only if it is on the first page's origin and names a
/// project issue collection under the same base path: `<base>/projects/<segment>/issues`. The
/// project segment may differ in form (an ID where the path was requested, or the new path of a
/// moved project); the query is GitLab's.
fn check_link(first: &Url, link: &Url) -> Result<(), PaginationProblem> {
    if !crate::same_origin(first, link) || link.cannot_be_a_base() {
        return Err(PaginationProblem::ForeignLink);
    }
    // The first page's path is `<base>/projects/<id>/issues`, by the declaration.
    let base: Vec<&str> = first.path_segments().into_iter().flatten().collect();
    let base = &base[..base.len().saturating_sub(3)];
    let segments: Vec<&str> = link.path_segments().into_iter().flatten().collect();
    match segments.strip_prefix(base) {
        Some(["projects", project, "issues"]) if !project.is_empty() => Ok(()),
        _ => Err(PaginationProblem::OtherCollection),
    }
}
