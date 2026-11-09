//! Provider models; monetary quantities remain decimal strings without rounding.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Kraken's finite response envelope. Generated `prepare()` exposes this wire type.
#[derive(Deserialize)]
pub struct Envelope<T> {
    /// Provider errors; any nonempty list is a failure in this SDK.
    pub error: Vec<String>,
    /// Absent on a provider failure; required for a successful SDK response.
    pub result: Option<T>,
}
impl<T> fmt::Debug for Envelope<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Envelope")
            .field("error_count", &self.error.len())
            .field("result_present", &self.result.is_some())
            .finish()
    }
}

/// Pairs indexed by Kraken's canonical pair identifier.
pub type Pairs = BTreeMap<String, Pair>;
/// Tickers indexed by canonical pair identifier.
pub type Tickers = BTreeMap<String, Ticker>;
/// Balances indexed by asset identifier (including suffixes such as `.F`).
pub type Balances = BTreeMap<String, String>;

/// The pair fields needed to select and display a market.
#[derive(Clone, Debug, Deserialize)]
pub struct Pair {
    /// Alternate REST identifier.
    pub altname: String,
    /// Display name, e.g. XBT/USD.
    pub wsname: Option<String>,
    /// Base asset identifier.
    pub base: String,
    /// Quote asset identifier.
    pub quote: String,
    /// Provider market status; selection accepts exactly `online`.
    pub status: String,
    /// Price decimal places.
    pub pair_decimals: u8,
    /// Volume decimal places.
    pub lot_decimals: u8,
    /// Minimum order volume, retained for display only.
    pub ordermin: Option<String>,
}

/// A ticker snapshot. Array positions follow Kraken's wire representation.
#[derive(Clone, Debug, Deserialize)]
pub struct Ticker {
    /// Ask: price, whole-lot volume, lot volume.
    pub a: [String; 3],
    /// Bid: price, whole-lot volume, lot volume.
    pub b: [String; 3],
    /// Last close: price, lot volume.
    pub c: [String; 2],
    /// Volume: today, last 24 hours.
    pub v: [String; 2],
    /// Volume weighted average price: today, last 24 hours.
    pub p: [String; 2],
    /// Trade count: today, last 24 hours.
    pub t: [u64; 2],
    /// Low: today, last 24 hours.
    pub l: [String; 2],
    /// High: today, last 24 hours.
    pub h: [String; 2],
    /// Today's opening price.
    pub o: String,
}

/// Options for balances. The policy injects the nonce at dispatch preparation.
#[derive(Clone, Default, Serialize)]
#[non_exhaustive]
#[derive(concord::Debug)]
#[sensitive]
pub struct BalanceOptions {
    /// One-time password if API 2FA is configured; withheld from diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp: Option<String>,
    /// Optional xStocks display convention.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebase_multiplier: Option<Rebase>,
}

/// xStocks balance and price representation.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Rebase {
    /// Underlying equity units.
    Rebased,
    /// SPV token units.
    Base,
}

/// Options for one complete open-orders read (cursor mode and limit are excluded).
/// Construct with `Default`, then assign the public option fields.
#[derive(Clone, Default, Serialize)]
#[non_exhaustive]
#[derive(concord::Debug)]
#[sensitive]
pub struct OpenOrdersOptions {
    /// One-time password if required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp: Option<String>,
    /// Include associated trade identifiers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trades: Option<bool>,
    /// Restrict to this signed 32-bit user reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub userref: Option<i32>,
    /// Restrict to this client order identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cl_ord_id: Option<String>,
    /// Whether to consolidate individual taker trades.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consolidate_taker: Option<bool>,
    /// Optional xStocks display convention.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebase_multiplier: Option<Rebase>,
}

/// All returned open orders. No pagination parameters are sent by this slice.
#[derive(Debug, Deserialize)]
pub struct OpenOrders {
    /// Orders keyed by transaction identifier.
    pub open: BTreeMap<String, OpenOrder>,
}

/// Read-only order information; there is no placement method.
#[derive(Debug, Deserialize)]
pub struct OpenOrder {
    /// Referral order identifier, if any.
    pub refid: Option<String>,
    /// Optional user reference.
    pub userref: Option<i32>,
    /// Optional client order identifier.
    pub cl_ord_id: Option<String>,
    /// Provider order status (unknown future values are retained).
    pub status: String,
    /// Opening time in fractional UNIX seconds.
    pub opentm: f64,
    /// Scheduled start time.
    pub starttm: f64,
    /// Expiration time.
    pub expiretm: f64,
    /// Description including market and order side.
    pub descr: OrderDescription,
    /// Original volume as an exact decimal string.
    pub vol: String,
    /// Executed volume.
    pub vol_exec: String,
    /// Executed cost.
    pub cost: String,
    /// Charged fee.
    pub fee: String,
    /// Average execution price.
    pub price: String,
    /// Stop price.
    pub stopprice: String,
    /// Limit price.
    pub limitprice: String,
    /// Miscellaneous provider flags.
    pub misc: String,
    /// Order flags.
    pub oflags: String,
    /// Associated trades, absent unless requested and present.
    pub trades: Option<Vec<String>>,
}

/// Trade-history filters, sent unchanged on every page of a traversal.
/// Construct with `Default`, then assign the public option fields. The offset
/// (`ofs`) is owned by [`Kraken::trade_history`](crate::Kraken::trade_history):
/// a generated `trades_history(options)` call reads the first page.
/// `without_count`, `limit`, `with_cursor` and `cursor` are excluded: the
/// traversal needs the provider's `count` and its default page size.
#[derive(Clone, Default, Serialize)]
#[non_exhaustive]
#[derive(concord::Debug)]
#[sensitive]
pub struct TradesHistoryOptions {
    /// One-time password if required; sent on every page, never refreshed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp: Option<String>,
    /// Position-related trade selection (provider default `all`).
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub trade_type: Option<TradeType>,
    /// Exclusive lower bound: a UNIX timestamp or a trade transaction identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<TradeBound>,
    /// Inclusive upper bound; a fixed value keeps later trades out of a running walk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<TradeBound>,
    /// Result offset; set per page by the traversal only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ofs: Option<u64>,
    /// Whether to consolidate trades by individual taker trades.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consolidate_taker: Option<bool>,
    /// Include related ledger identifiers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ledgers: Option<bool>,
    /// Optional xStocks display convention.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebase_multiplier: Option<Rebase>,
    /// Asset class of the `pair` filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aclass: Option<AssetClass>,
    /// Restrict to one trading pair.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pair: Option<String>,
}

/// A `start`/`end` bound: Kraken accepts either form in the same field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum TradeBound {
    /// UNIX timestamp in whole seconds.
    Time(u64),
    /// Trade transaction identifier, e.g. `THVRQM-33VKH-UCI7BS`.
    Trade(String),
}

/// Position-related trade selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TradeType {
    /// Every trade.
    #[serde(rename = "all")]
    All,
    /// Trades related to any position.
    #[serde(rename = "any position")]
    AnyPosition,
    /// Trades of closed positions.
    #[serde(rename = "closed position")]
    ClosedPosition,
    /// Trades closing a position.
    #[serde(rename = "closing position")]
    ClosingPosition,
    /// Trades without a position.
    #[serde(rename = "no position")]
    NoPosition,
}

/// Asset class of a `pair` filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    /// Currency pairs (provider default).
    Forex,
    /// Equity pairs.
    EquityPair,
    /// Futures contracts.
    FuturesContract,
    /// Synthetic pairs.
    SyntheticPair,
    /// External pairs.
    ExternalPair,
}

/// One trade-history page as Kraken returns it.
#[derive(Debug, Deserialize)]
pub struct TradesHistory {
    /// Trades keyed by trade transaction identifier (provider order is not kept).
    pub trades: BTreeMap<String, Trade>,
    /// Matching trades in total. The reference's response example omits it;
    /// the traversal requires it on every page.
    pub count: Option<u64>,
}

/// A read-only trade (fill). Quantities stay exact decimal strings. Position
/// closure fields (`posstatus`, `cprice`, `ccost`, `cfee`, `cvol`, `cmargin`,
/// `net`, `trades`) are not modelled and are ignored when present.
#[derive(Clone, Debug, Deserialize)]
pub struct Trade {
    /// Order responsible for the trade.
    pub ordertxid: String,
    /// Position responsible for the trade, if any.
    pub postxid: Option<String>,
    /// Canonical pair identifier.
    pub pair: String,
    /// Trade time in fractional UNIX seconds.
    pub time: f64,
    /// Buy or sell; retained as a string for forward compatibility.
    #[serde(rename = "type")]
    pub side: String,
    /// Order type.
    pub ordertype: String,
    /// Order type at execution, when the provider reports it.
    pub tradeordertype: Option<String>,
    /// Average execution price (quote currency).
    pub price: String,
    /// Total cost (quote currency).
    pub cost: String,
    /// Total fee (quote currency).
    pub fee: String,
    /// Volume (base currency).
    pub vol: String,
    /// Initial margin (quote currency).
    pub margin: Option<String>,
    /// Leverage used.
    pub leverage: Option<String>,
    /// Comma-delimited provider flags, e.g. `closing`.
    pub misc: Option<String>,
    /// Ledger identifiers, when requested.
    pub ledgers: Option<Vec<String>>,
    /// Provider's numeric trade identifier.
    pub trade_id: Option<u64>,
    /// Whether the account was the maker.
    pub maker: Option<bool>,
    /// Asset class of the traded pair.
    pub aclass: Option<String>,
    /// External execution identifier.
    pub ext_exec_id: Option<String>,
}

/// Provider description of an existing order.
#[derive(Debug, Deserialize)]
pub struct OrderDescription {
    /// Alternate pair name.
    pub pair: String,
    /// Buy or sell; retained as a string for forward compatibility.
    #[serde(rename = "type")]
    pub side: String,
    /// Provider order type.
    pub ordertype: String,
    /// Primary price.
    pub price: String,
    /// Secondary price.
    pub price2: String,
    /// Leverage description.
    pub leverage: String,
    /// Human-readable order description.
    pub order: String,
    /// Conditional close description.
    pub close: String,
}
