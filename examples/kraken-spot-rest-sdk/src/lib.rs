//! Bounded Kraken Spot REST reads, with fixture-only validation.
//!
//! Use [`Kraken::public`] for market discovery and [`Kraken::account`] for signed
//! balances, open orders and one trade-history page; [`Kraken::trade_history`]
//! walks every page. Generated `send()` uses [`Policy`]; generated
//! `prepare()` remains unsigned. See the package README for offline usage,
//! exclusions, nonce persistence and arrival-order limitations.

#![doc = include_str!("../README.md")]

pub use concord;
pub use concord_pagination;
pub use concord_pagination::{Incomplete, Page, PagesError, Resume};
pub use concord_retry::http::RetryAfterError;
mod auth;
mod error;
mod models;
mod policy;
mod trades;
pub use auth::{Credentials, Nonces};
use concord::{Form, Json, reqwest};
pub use error::{
    ApiClass, ApiFailure, Error, ErrorDocument, HttpFailure, LocalError, PageFailure, PageProblem,
    classify,
};
pub use models::*;
pub use policy::{ERROR_LIMIT, Policy, RESPONSE_LIMIT, Response, Signed};
pub use trades::{TRADES_PAGE_CAP, TRADES_PAGE_SIZE, TradePages, TradesPageMeta};

concord::api! {
    /// Read-only Kraken Spot REST slice. Use [`transport`] to disable redirects and replays.
    pub client Kraken(policy: Policy) {
        base "https://api.kraken.com";
        path "0";
        response Json;
        limit RESPONSE_LIMIT;
        send policy;

        /// Public market data; never decorates requests with account credentials.
        scope public {
            path "public";
            /// Fetch tradable pairs, optionally restricting the provider's result.
            GET pairs(query pair?: String) -> Envelope<Pairs> { path "AssetPairs"; }
            /// Fetch ticker data for the required pair selector.
            GET ticker(query pair: String) -> Envelope<Tickers> { path "Ticker"; }
        }

        /// Private reads: API-Key and API-Sign plus a fresh form-body nonce.
        #[read]
        scope account {
            path "private";
            request Form;
            /// Fetch balances in the default wallet or an explicitly selected wallet.
            POST balances(body options: BalanceOptions, #[sensitive] query account_id?: String) -> Envelope<Balances> {
                path "Balance";
            }
            /// Fetch all open orders in one bounded response, without a server-side limit.
            POST open_orders(body options: OpenOrdersOptions, #[sensitive] query account_id?: String) -> Envelope<OpenOrders> {
                path "OpenOrders";
            }
            /// Fetch the first trade-history page (at most 50 trades) with these filters.
            /// Use [`Kraken::trade_history`] for later pages.
            POST trades_history(body options: TradesHistoryOptions, #[sensitive] query account_id?: String) -> Envelope<TradesHistory> {
                path "TradesHistory";
            }
        }
    }
}

/// A finite, single-dispatch transport for local fixtures. No TLS feature is enabled
/// by this package; an application choosing HTTPS must enable reqwest TLS itself.
/// Custom clients must likewise disable redirects and native retries to avoid
/// forwarding custom credential headers or replaying a signed nonce.
pub fn transport() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .referer(false)
        .retry(reqwest::retry::never())
        .timeout(std::time::Duration::from_secs(10))
        .build()
}

/// Connected public workflow output, retaining both response heads.
#[derive(Debug)]
pub struct MarketSelection {
    /// Canonical identifier used for the ticker request.
    pub id: String,
    /// Selected online pair.
    pub pair: Pair,
    /// Latest ticker for this pair.
    pub ticker: Ticker,
    /// Pair listing metadata.
    pub pairs_meta: concord::ResponseMeta,
    /// Ticker metadata.
    pub ticker_meta: concord::ResponseMeta,
}

/// Two sequential account reads. They are not an atomic provider snapshot.
#[derive(Debug)]
pub struct AccountSnapshot {
    /// Balances were fetched first.
    pub balances: Response<Balances>,
    /// Open orders were fetched second.
    pub orders: Response<OpenOrders>,
}

impl Kraken {
    /// Fetches pairs, uniquely selects an online canonical/alternate/display name,
    /// then fetches the selected canonical ticker. No trading action is performed.
    pub async fn select_market(&self, selector: &str) -> Result<MarketSelection, Error> {
        let pairs = self.public().pairs().send().await?;
        let mut matches = pairs.value.into_iter().filter(|(id, pair)| {
            id == selector || pair.altname == selector || pair.wsname.as_deref() == Some(selector)
        });
        let (id, pair) = matches.next().ok_or(LocalError::PairUnavailable)?;
        if matches.next().is_some() || pair.status != "online" {
            return Err(LocalError::PairUnavailable.into());
        }
        let mut tickers = self.public().ticker(id.clone()).send().await?;
        if tickers.value.len() != 1 {
            return Err(LocalError::TickerMismatch.into());
        }
        let ticker = tickers
            .value
            .remove(&id)
            .ok_or(LocalError::TickerMismatch)?;
        Ok(MarketSelection {
            id,
            pair,
            ticker,
            pairs_meta: pairs.meta,
            ticker_meta: tickers.meta,
        })
    }

    /// Authenticates and fetches default-wallet balances, then all open orders.
    /// Fails on either read; there is no automatic replay and no atomicity promise.
    /// Keys requiring OTP should call the generated endpoints with explicit options.
    pub async fn account_snapshot(&self) -> Result<AccountSnapshot, Error> {
        let balances = self
            .account()
            .balances(BalanceOptions::default())
            .send()
            .await?;
        let orders = self
            .account()
            .open_orders(OpenOrdersOptions::default())
            .send()
            .await?;
        Ok(AccountSnapshot { balances, orders })
    }
}

pub(crate) use __KrakenOperation as KrakenOperation;

#[cfg(test)]
mod operation_inventory_tests {
    use super::*;
    #[test]
    fn the_account_scope_marks_all_three_posts_as_reads() {
        use KrakenOperation as Op;
        let expected = [
            (Op::PublicPairs, "GET"),
            (Op::PublicTicker, "GET"),
            (Op::AccountBalances, "POST"),
            (Op::AccountOpenOrders, "POST"),
            (Op::AccountTradesHistory, "POST"),
        ];
        assert_eq!(Op::ALL.len(), expected.len());
        for (identity, method) in expected {
            let operation = identity.operation();
            assert_eq!(operation.method(), Some(method));
            assert!(operation.is_read());
            assert_eq!(Op::from_operation(operation), Some(identity));
        }
    }
}
