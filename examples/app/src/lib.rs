//! An application over the shared SDK (`examples/sdk`): the code an application writes between
//! its own domain and two independent Concord clients, Catalog and Ledger.
//!
//! Everything here is application policy, not a Concord rule. Concord sends what the generated
//! operation holds; which domain types exist, how they become each service's wire text, how two
//! services' failures are reported together, which listing calls deserve a name and how far to
//! walk a paginated listing are this application's decisions.
//!
//! - [`domain`]: the application's own values, [`AccountId`](domain::AccountId) and
//!   [`Moment`](domain::Moment). They know nothing about either service.
//! - [`catalog_repr`], [`ledger_repr`]: each service's representation of those values, with its
//!   own typed error. The conversion is synchronous and returns the **generated operation**
//!   (`CatalogActivity`, `LedgerEntries`), so the caller keeps every setter, `prepare()` and
//!   `send()`; a value the service cannot represent exactly is refused before anything is built.
//! - [`report`]: one account's report over both services, where each service has its own
//!   [`Outcome`](report::Outcome) and one service's refusal or failure never hides the other's.
//! - [`workflow`]: a named listing workflow ([`Browser`](workflow::Browser)) that returns the
//!   generated list operation, not a response.
//! - [`traversal`]: one bounded page walk shared by both services, with a continuation to resume
//!   and an explicit partial-result policy when a page fails.

pub mod catalog_repr;
pub mod domain;
pub mod ledger_repr;
pub mod report;
pub mod traversal;
pub mod workflow;
