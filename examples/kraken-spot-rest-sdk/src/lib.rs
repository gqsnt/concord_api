//! Bounded Kraken Spot REST reads, with fixture-only validation.
//!
//! Use [`Kraken::public`] for market discovery and [`Kraken::account`] for signed
//! balances and open orders. Generated `send()` uses [`Policy`]; generated
//! `prepare()` remains unsigned. See the package README for offline usage,
//! exclusions, nonce persistence and arrival-order limitations.

#![doc = include_str!("../README.md")]

pub use concord;
pub use concord_retry::http::RetryAfterError;
mod auth;
mod error;
mod models;
mod policy;
pub use auth::{Credentials, Nonces};
use concord::{Form, Json, reqwest};
pub use error::{ApiClass, ApiFailure, Error, ErrorDocument, HttpFailure, LocalError, classify};
pub use models::*;
pub use policy::{ERROR_LIMIT, Policy, RESPONSE_LIMIT, Response, Signed};

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
