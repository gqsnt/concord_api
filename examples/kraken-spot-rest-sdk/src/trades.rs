//! Offset traversal of trade history over the generated single-page endpoint.

use crate::{
    Error, Kraken, PageFailure, PageProblem, Response, Trade, TradesHistory, TradesHistoryOptions,
};
use concord::ResponseMeta;
use concord_pagination::{Page, Pages, Resume, next_offset};
use std::num::NonZeroUsize;

/// Trades per page: the provider's documented default, since no `limit` is sent.
/// A page with more trades is refused ([`PageProblem::Offset`]).
pub const TRADES_PAGE_SIZE: u64 = 50;

/// Default cap on signed page requests per traversal (1,000 trades at the page
/// size). Replace it with [`Pages::max_pages`] before reading.
pub const TRADES_PAGE_CAP: NonZeroUsize = NonZeroUsize::new(20).unwrap();

/// What a traversal keeps with each page besides its trades.
#[derive(Debug)]
pub struct TradesPageMeta {
    /// Offset at which this page was requested (0 for the first page).
    pub offset: u64,
    /// The provider's `count` reported by this page.
    pub count: u64,
    /// HTTP head and endpoint identity of this page.
    pub meta: ResponseMeta,
}

/// A trade-history traversal: `(trade id, trade)` items keyed by the next `ofs`.
pub type TradePages = Pages<(String, Trade), u64, TradesPageMeta, Error>;

impl Kraken {
    /// Walks `TradesHistory` by `ofs` from `from` until the provider's `count` is
    /// reached. Every page sends the same `options` and `account_id`, and is
    /// signed by the shared policy with a fresh nonce, after the previous page
    /// arrived. Nothing is sent until the traversal is read.
    ///
    /// The cap defaults to [`TRADES_PAGE_CAP`]. Reaching it with trades left is
    /// `PagesError::Bounded` with the offset to resume from. A failed page is
    /// `PagesError::Provider` with the SDK [`Error`] and the failed page as resume
    /// point; `collect_complete` keeps every completed page in `Incomplete`.
    /// Nothing is retried. Resume with the same options: a resume point belongs
    /// to its filters. Without a fixed `end`, newer trades shift offsets during a
    /// walk, so a trade can appear on two pages (merge by trade id).
    pub fn trade_history(
        &self,
        options: TradesHistoryOptions,
        account_id: Option<String>,
        from: Resume<u64>,
    ) -> TradePages {
        let client = self.clone();
        Pages::new(from, move |ofs: Option<u64>| {
            let mut page = options.clone();
            page.ofs = ofs;
            let request = client
                .account()
                .trades_history(page)
                .maybe_account_id(account_id.clone());
            async move { project(request.send().await?, ofs.unwrap_or(0)) }
        })
        .max_pages(TRADES_PAGE_CAP)
    }
}

fn project(
    response: Response<TradesHistory>,
    offset: u64,
) -> Result<Page<(String, Trade), u64, TradesPageMeta>, Error> {
    let Response { value, meta } = response;
    let refuse = |meta, problem| {
        Error::Page(Box::new(PageFailure {
            meta,
            offset,
            problem,
        }))
    };
    let Some(count) = value.count else {
        return Err(refuse(meta, PageProblem::MissingCount));
    };
    let len = u64::try_from(value.trades.len()).unwrap_or(u64::MAX);
    let next = match next_offset(offset, TRADES_PAGE_SIZE, len, Some(count)) {
        Ok(next) => next,
        Err(e) => return Err(refuse(meta, PageProblem::Offset(e))),
    };
    Ok(Page {
        items: value.trades.into_iter().collect(),
        next,
        meta: TradesPageMeta {
            offset,
            count,
            meta,
        },
    })
}
