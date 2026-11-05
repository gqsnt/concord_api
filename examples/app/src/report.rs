//! One account's report over both services, with an independent outcome per service.
//!
//! This aggregation is **application policy, not a Concord rule**. Concord sends one request per
//! `send()` and returns its result; it has no notion of "a report" over two clients. This
//! application decided that Catalog and Ledger answer different questions, so a value one service
//! cannot represent, or one service's failure, must not hide the other service's answer. That is
//! why [`account_report`] has no aggregate `?`: an early return on Catalog's refusal would never
//! ask Ledger. An application whose report is meaningless unless both parts exist would choose
//! the opposite policy, and that would be just as legitimate.
//!
//! The services are attempted one after the other (Catalog, then Ledger), which keeps the order
//! of requests deterministic; running them concurrently would not change the outcomes.

use std::future::Future;

use concord_example_sdk::{
    ActivityPage, Catalog, CatalogActivity, EntryPage, Ledger, LedgerEntries,
};

use crate::catalog_repr::{self, CatalogReprError};
use crate::domain::{AccountId, Moment};
use crate::ledger_repr::{self, LedgerReprError};

/// What happened to one service's part of a report.
#[derive(Debug)]
pub enum Outcome<T, E> {
    /// The service cannot represent the domain value: no operation was created, nothing was sent.
    Rejected(E),
    /// The operation was created, and Concord returned an error for it (a status, transport,
    /// build or decode error; [`concord::Error::kind`] tells which).
    Failed(concord::Error),
    /// The service answered and the response decoded.
    Succeeded(T),
}

impl<T, E> Outcome<T, E> {
    /// Attempts one service: `converted` is the result of a representation adapter (possibly with
    /// caller setters applied), `send` runs the operation. A rejected conversion is recorded
    /// without calling `send`.
    ///
    /// ```no_run
    /// # async fn demo(ledger: &concord_example_sdk::Ledger) {
    /// use concord_example_app::domain::AccountId;
    /// use concord_example_app::ledger_repr;
    /// use concord_example_app::report::Outcome;
    /// use concord_example_sdk::LedgerEntries;
    ///
    /// let outcome = Outcome::attempt(
    ///     ledger_repr::entries(ledger, AccountId::new(42), None).map(|r| r.set_page_size(2)),
    ///     LedgerEntries::send,
    /// )
    /// .await;
    /// # let _ = outcome;
    /// # }
    /// ```
    pub async fn attempt<R, Fut>(converted: Result<R, E>, send: impl FnOnce(R) -> Fut) -> Self
    where
        Fut: Future<Output = Result<T, concord::Error>>,
    {
        match converted {
            Err(rejected) => Self::Rejected(rejected),
            Ok(operation) => match send(operation).await {
                Ok(value) => Self::Succeeded(value),
                Err(error) => Self::Failed(error),
            },
        }
    }
}

/// One account's activity (Catalog) and entries (Ledger), each with its own outcome.
#[derive(Debug)]
pub struct AccountReport {
    /// Catalog's `accounts/{account}/activity`.
    pub catalog: Outcome<ActivityPage, CatalogReprError>,
    /// Ledger's first page of `entries`.
    pub ledger: Outcome<EntryPage, LedgerReprError>,
}

/// Reports one account since `since` (absent: from the beginning) on both services.
///
/// Each service converts the domain values its own way and is attempted independently: Catalog
/// refusing a fractional second does not stop Ledger, which represents milliseconds, from
/// answering, and a Ledger failure does not discard Catalog's answer. Ledger contributes its first
/// page at its declared page size.
pub async fn account_report(
    catalog: &Catalog,
    ledger: &Ledger,
    account: AccountId,
    since: Option<Moment>,
) -> AccountReport {
    let catalog = Outcome::attempt(
        catalog_repr::activity(catalog, account, since),
        CatalogActivity::send,
    )
    .await;
    let ledger = Outcome::attempt(
        ledger_repr::entries(ledger, account, since),
        LedgerEntries::send,
    )
    .await;
    AccountReport { catalog, ledger }
}
