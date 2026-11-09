//! `GET /repos/{owner}/{repo}/issues/{issue_number}/events` (`issues/list-events`), an operation
//! the SDK does not declare, added **from outside the SDK** the way its documentation says
//! another crate should (stage 3 of the maintenance exercise).
//!
//! The SDK's `context` module: "another crate can add an operation without copying the
//! credentials or the version pin, and without editing this SDK", through
//! [`RepoContext::request`]; Concord's recipe ("An operation the SDK does not declare",
//! `docs/RECIPES.md`) then says: execute once with the context's client, finish with
//! `Decoder::<T>::new::<Json>()`, and name the operation in your own error type. Since Concord's
//! response boundary, that is `Decoder::execute` (one execution, any status, body unread), the
//! metadata copied before the body is read, then `Decoder::decode`: no `finish(Err(_))`
//! conversion.
//!
//! What the context gives this module: the path prefix `repos/{owner}/{repo}` with each segment
//! encoded like a Concord path value, and the four service headers (token flagged sensitive,
//! `Accept`, `X-GitHub-Api-Version`, `User-Agent`). What the SDK's public types give: the
//! response metadata ([`ResponseMeta::capture`]), the `Link` parser behind
//! [`ResponseMeta::links`], the origin check ([`GitHubContext::is_api_url`]), the continuation
//! [`Binding`] and the error type ([`GitHubError`]: its variants are public and constructible
//! here). What neither gives, and this module therefore repeats: the query encoding of
//! `per_page`, and the send/continuation logic of the SDK's `meta::send_edited` and
//! `paging::LinkPages`, which both take a Concord `Prepared` (a native request cannot become one)
//! and whose `NextLink` key has no public constructor. `MAINTENANCE.md` measures it.
//!
//! Provider contract (REST API `2026-03-10`, docs retrieved 2026-11-05): `per_page` (max 100) and
//! `page`; `200` with an array of issue events, `410 Gone`; every event variant has `id`,
//! `node_id`, `url`, `actor`, `event`, `commit_id`, `commit_url`, `created_at` and
//! `performed_via_github_app`. Pagination follows `Link: rel="next"` like every GitHub listing.

use std::future::Future;

use concord::reqwest::{self, Method, Url};
use concord::{Decoder, Json};
use concord_example_github_sdk::context::GitHubContext;
use concord_example_github_sdk::ids::IssueNumber;
use concord_example_github_sdk::meta::ResponseMeta;
use concord_example_github_sdk::models::User;
use concord_example_github_sdk::paging::Binding;
use concord_example_github_sdk::params::{PerPage, Timestamp};
use concord_example_github_sdk::{GitHubError, RepoContext};
use concord_pagination::Page;
use serde::Deserialize;

/// GitHub's operation id, the operation part of a continuation's [`Binding`].
pub const OPERATION: &str = "issues/list-events";

/// An issue event's numeric `id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(transparent)]
pub struct IssueEventId(pub u64);

/// One issue event: the fields every documented variant has, plus the `label` of
/// `labeled`/`unlabeled` events. `event` stays text: GitHub documents dozens of event types and
/// adds new ones (issue types, sub-issues, blocking relations).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct IssueEvent {
    /// The event id.
    pub id: IssueEventId,
    /// The event type (`labeled`, `closed`, `reopened`, ...).
    pub event: String,
    /// Who caused it (`null` for a deleted account).
    pub actor: Option<User>,
    /// The commit that referenced the issue, if any.
    pub commit_id: Option<String>,
    /// When it happened.
    pub created_at: Timestamp,
    /// The label of a `labeled`/`unlabeled` event.
    #[serde(default)]
    pub label: Option<EventLabel>,
}

/// The `label` object of a label event.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct EventLabel {
    /// The label's name.
    pub name: String,
    /// Its hexadecimal color.
    pub color: String,
}

/// A page GitHub linked to (`rel="next"`). `Debug` withholds the URL.
#[derive(Clone, PartialEq, Eq, concord::Debug)]
pub struct EventsLink {
    #[sensitive]
    url: Url,
    binding: Binding,
}

impl EventsLink {
    /// What it belongs to (operation, repository, issue number).
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
}

/// One page of events, with its response metadata.
pub type EventsPage = Page<IssueEvent, EventsLink, ResponseMeta>;

/// A page factory for the events of one issue, for `concord-pagination` (the counterpart of the
/// SDK's `RepoContext::*_pages`).
#[derive(Clone, Debug)]
pub struct IssueEventPages {
    repo: RepoContext,
    number: IssueNumber,
    per_page: Option<PerPage>,
    binding: Binding,
}

impl IssueEventPages {
    /// The events of issue (or pull request) `number` of `repo`.
    pub fn new(repo: &RepoContext, number: IssueNumber) -> Self {
        Self {
            repo: repo.clone(),
            number,
            per_page: None,
            binding: Binding::new(OPERATION, repo.repo(), Some(number.get())),
        }
    }

    /// Sends `per_page` on the first page (linked pages carry GitHub's own query).
    pub fn per_page(mut self, per_page: PerPage) -> Self {
        self.per_page = Some(per_page);
        self
    }

    /// What this factory's continuations belong to.
    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    /// Fetches the first page (`None`) or a linked page. The request is built now (no I/O); the
    /// returned future owns everything it needs.
    pub fn fetch(
        &self,
        key: Option<EventsLink>,
    ) -> impl Future<Output = Result<EventsPage, GitHubError>> + use<> {
        let built = self.build(key);
        let context = self.repo.context().clone();
        let binding = self.binding.clone();
        async move { send(context, built?, binding).await }
    }

    fn build(&self, key: Option<EventsLink>) -> Result<reqwest::Request, GitHubError> {
        let number = self.number.to_string();
        let mut request = self
            .repo
            .request(Method::GET, &["issues", &number, "events"])?;
        match key {
            None => {
                if let Some(per_page) = self.per_page {
                    request
                        .url_mut()
                        .query_pairs_mut()
                        .append_pair("per_page", &per_page.to_string());
                }
            }
            Some(link) => {
                if link.binding != self.binding {
                    return Err(GitHubError::ContinuationMismatch {
                        expected: Box::new(self.binding.clone()),
                        found: Box::new(link.binding),
                    });
                }
                if !self.repo.context().is_api_url(&link.url) {
                    return Err(GitHubError::ForeignLink { meta: None });
                }
                *request.url_mut() = link.url;
            }
        }
        Ok(request)
    }
}

async fn send(
    context: GitHubContext,
    request: reqwest::Request,
    binding: Binding,
) -> Result<EventsPage, GitHubError> {
    // One execution, any status, body unread; a transport failure has its URL removed and no
    // metadata (no head arrived).
    let received = Decoder::<Vec<IssueEvent>>::new::<Json>()
        .execute(context.http(), request)
        .await
        .map_err(|error| GitHubError::Api { error, meta: None })?;
    // The metadata is read on the native response: relative `Link` targets resolve against its
    // URL, which `Received::head` does not carry.
    let (response, decoder) = received.into_parts();
    let meta = ResponseMeta::capture(&response);
    let events = match decoder.decode(response).await {
        Ok(events) => events,
        Err(error) => {
            return Err(GitHubError::Api {
                error,
                meta: Some(Box::new(meta)),
            });
        }
    };
    let next = match meta.links() {
        Err(error) => {
            return Err(GitHubError::Link {
                error,
                meta: Box::new(meta),
            });
        }
        Ok(links) => match links.next() {
            None => None,
            Some(url) if context.is_api_url(url) => Some(EventsLink {
                url: url.clone(),
                binding,
            }),
            Some(_) => {
                return Err(GitHubError::ForeignLink {
                    meta: Some(Box::new(meta)),
                });
            }
        },
    };
    Ok(Page {
        items: events,
        next,
        meta,
    })
}
