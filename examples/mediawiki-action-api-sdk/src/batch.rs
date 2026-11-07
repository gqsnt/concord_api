use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use crate::{Error, MediaWikiQuery, PageContent, QueryResponse, models::ContentPage, models::Row};

/// The documented `pageids` limit for the caller's rights (API:Query, read 2026-11-07).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageIdLimit {
    /// 50 page IDs per query: every client.
    #[default]
    Standard,
    /// 500 page IDs per query: only clients the wiki grants `apihighlimits`. This SDK sends no
    /// credentials, so choose it only for a wiki that grants it to anonymous readers.
    HighLimits,
}

impl PageIdLimit {
    /// The maximum number of page IDs in one request.
    pub const fn max(self) -> usize {
        match self {
            Self::Standard => 50,
            Self::HighLimits => 500,
        }
    }
}

/// More page IDs than the chosen [`PageIdLimit`] allows; nothing was sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooManyPageIds {
    /// IDs the caller passed, duplicates included.
    pub count: usize,
    /// The chosen documented limit.
    pub limit: usize,
}

impl fmt::Display for TooManyPageIds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} page IDs exceed the limit of {}",
            self.count, self.limit
        )
    }
}

impl std::error::Error for TooManyPageIds {}

/// Page IDs for one batch, in the caller's order, checked against a documented limit.
///
/// `Display` is the wire value: each distinct ID once, first occurrence first, joined by `|`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageIds {
    requested: Vec<u64>,
    distinct: Vec<u64>,
}

impl PageIds {
    /// Accept up to `limit.max()` IDs (duplicates count). Empty is valid and sends nothing.
    pub fn new(
        ids: impl IntoIterator<Item = u64>,
        limit: PageIdLimit,
    ) -> Result<Self, TooManyPageIds> {
        let requested: Vec<u64> = ids.into_iter().collect();
        if requested.len() > limit.max() {
            return Err(TooManyPageIds {
                count: requested.len(),
                limit: limit.max(),
            });
        }
        let mut seen = BTreeSet::new();
        let distinct = requested
            .iter()
            .copied()
            .filter(|id| seen.insert(*id))
            .collect();
        Ok(Self {
            requested,
            distinct,
        })
    }

    /// The IDs exactly as passed, duplicates included.
    pub fn requested(&self) -> &[u64] {
        &self.requested
    }
}

impl fmt::Display for PageIds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, id) in self.distinct.iter().enumerate() {
            if index > 0 {
                f.write_str("|")?;
            }
            write!(f, "{id}")?;
        }
        Ok(())
    }
}

/// Why a requested page has no content in a batch answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    /// The wiki has no page with this ID (`missing`).
    Missing,
    /// The wiki reported the page as `invalid`.
    Invalid,
    /// The page exists but has no readable latest main-slot content: no revision, or hidden
    /// (`texthidden`) or otherwise absent content.
    NoContent,
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Missing => "the page is missing",
            Self::Invalid => "the page is invalid",
            Self::NoContent => "the page has no readable content",
        })
    }
}

impl std::error::Error for Unavailable {}

/// One requested page ID's answer, at the position the caller requested it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageAnswer {
    /// The requested page ID.
    pub pageid: u64,
    /// The latest revision's raw main-slot content, or why there is none.
    pub content: Result<PageContent, Unavailable>,
}

type Answers = BTreeMap<u64, Option<Result<PageContent, Unavailable>>>;

impl MediaWikiQuery {
    /// Fetch the latest content of every page in `pageids`, one answer per requested ID in the
    /// caller's order (a repeated ID is answered at each position).
    ///
    /// A continued response is followed with its whole continuation until every ID is answered;
    /// a page listed without a revision while a continuation is present is not answered yet
    /// (API:Continue, API:Revisions). Each continued response must answer a new ID. An ID that
    /// is never answered, a row for an ID that was not requested or a second answer for one ID
    /// is `Error::InvalidResponse`; any whole-response failure of `send()` fails the batch.
    pub async fn fetch_pages(&self, pageids: &PageIds) -> Result<Vec<PageAnswer>, Error> {
        let mut answers: Answers = pageids.distinct.iter().map(|id| (*id, None)).collect();
        let mut pending = answers.len();
        let mut continuation = None;
        while pending > 0 {
            let QueryResponse {
                data,
                continuation: next,
                meta,
            } = self
                .contents(pageids.clone())
                .maybe_continuation(continuation)
                .send()
                .await?;
            let answered = merge(&mut answers, data.pages, next.is_none());
            match answered {
                Some(answered) if answered == pending => pending = 0,
                Some(answered) if answered > 0 && next.is_some() => pending -= answered,
                _ => return Err(Error::InvalidResponse(Box::new(meta))),
            }
            continuation = next;
        }
        let requested = &pageids.requested;
        Ok(requested
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let content = if requested[index + 1..].contains(id) {
                    answers[id].clone()
                } else {
                    answers.remove(id).flatten()
                };
                PageAnswer {
                    pageid: *id,
                    content: content.expect("every requested ID was answered"),
                }
            })
            .collect())
    }
}

/// Record this response's answers; `None` if a row cannot belong to this batch.
fn merge(answers: &mut Answers, pages: Vec<ContentPage>, last: bool) -> Option<usize> {
    let mut listed = BTreeSet::new();
    let mut answered = 0;
    for page in pages {
        let id = page.pageid?;
        let slot = answers.get_mut(&id)?;
        if !listed.insert(id) {
            return None;
        }
        if slot.is_some() {
            // Re-listed with the next properties; a second revision would be a replay.
            if !page.revisions.is_empty() {
                return None;
            }
            continue;
        }
        *slot = Some(match page.project(id) {
            Row::Content(content) => Ok(content),
            Row::Unavailable(reason) => Err(reason),
            Row::NoRevision if last => Err(Unavailable::NoContent),
            Row::NoRevision => continue,
            Row::Inconsistent => return None,
        });
        answered += 1;
    }
    Some(answered)
}
