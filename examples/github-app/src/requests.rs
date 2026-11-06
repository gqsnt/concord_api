//! The request log a workflow reports: one record per request it sent, with GitHub's request id
//! and rate-limit headers, never a URL, a query or a body.

use std::sync::{Arc, Mutex};

use concord_example_github_sdk::GitHubError;
use concord_example_github_sdk::meta::{RateLimit, ResponseMeta};
use concord_example_github_sdk::paging::{LinkPage, NextLink};
use concord_pagination::PagesError;

/// One request of a workflow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestRecord {
    /// GitHub's operation id (`issues/list-for-repo`, ...).
    pub operation: &'static str,
    /// The response status; `None` when no response arrived (or nothing was sent).
    pub status: Option<u16>,
    /// `x-github-request-id`.
    pub request_id: Option<String>,
    /// The rate-limit headers of the response.
    pub rate: RateLimit,
}

impl RequestRecord {
    pub(crate) fn of(operation: &'static str, meta: Option<&ResponseMeta>) -> Self {
        Self {
            operation,
            status: meta.map(|m| m.status().as_u16()),
            request_id: meta.and_then(|m| m.request_id().map(str::to_owned)),
            rate: meta.map(|m| m.rate_limit().clone()).unwrap_or_default(),
        }
    }
}

/// A log shared by a workflow and the page factories it hands to `concord-pagination` (whose item
/// consumer does not return page metadata, so each page's future records its own).
#[derive(Clone, Default)]
pub(crate) struct RequestLog(Arc<Mutex<Vec<RequestRecord>>>);

impl RequestLog {
    pub(crate) fn push(&self, record: RequestRecord) {
        self.0.lock().expect("request log").push(record);
    }

    pub(crate) fn meta(&self, operation: &'static str, meta: &ResponseMeta) {
        self.push(RequestRecord::of(operation, Some(meta)));
    }

    pub(crate) fn error(&self, operation: &'static str, error: &GitHubError) {
        // A build, continuation or context error sent nothing: no record.
        let built_nothing = error
            .concord()
            .is_some_and(|e| e.kind() == concord::ErrorKind::Build);
        if built_nothing
            || matches!(
                error,
                GitHubError::ContinuationMismatch { .. }
                    | GitHubError::Context(_)
                    | GitHubError::ForeignLink { meta: None }
            )
        {
            return;
        }
        self.push(RequestRecord::of(operation, error.meta()));
    }

    pub(crate) fn take(&self) -> Vec<RequestRecord> {
        std::mem::take(&mut *self.0.lock().expect("request log"))
    }
}

/// The last rate-limit headers seen, for pacing a following run.
pub fn last_rate(records: &[RequestRecord]) -> Option<&RateLimit> {
    records
        .iter()
        .rev()
        .map(|r| &r.rate)
        .find(|r| r.remaining.is_some())
}

/// The result of one `next_page()` of an ordinary traversal.
pub(crate) type NextPage<T> = Result<Option<LinkPage<T>>, PagesError<GitHubError, NextLink>>;

/// Records the request behind one step of an ordinary traversal: a page, or a failed page (a
/// budget reached sends nothing and records nothing).
pub(crate) fn record_next<T>(next: &NextPage<T>, operation: &'static str, log: &RequestLog) {
    match next {
        Ok(Some(page)) => log.meta(operation, &page.meta.response),
        Err(PagesError::Provider { error, .. }) => log.error(operation, error),
        Ok(None) | Err(_) => {}
    }
}

/// Records a page's request (success or failure) and passes the result on: the page factories'
/// futures call it, because `concord-pagination`'s item consumer does not hand page metadata back.
pub(crate) fn record_page<T>(
    operation: &'static str,
    result: Result<LinkPage<T>, GitHubError>,
    log: &RequestLog,
) -> Result<LinkPage<T>, GitHubError> {
    match &result {
        Ok(page) => log.meta(operation, &page.meta.response),
        Err(error) => log.error(operation, error),
    }
    result
}
