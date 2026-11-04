//! An external caller of the Stripe SDK slice: the two required workflows as library functions.
//!
//! - [`reconciliation`] — **ST-A**, billing reconciliation: [`reconciliation::reconcile`] takes a
//!   configured context and a [`ReconcileInput`](reconciliation::ReconcileInput) (selectors,
//!   budgets, resume points) and returns a
//!   [`ReconciliationReport`](reconciliation::ReconciliationReport). Its traversals are
//!   `concord_pagination::sequential` chains over the SDK's cursor adapter.
//! - [`provisioning`] — **ST-B**, form and idempotency scenario: [`provisioning::provision`]
//!   takes a configured [`StripeContext`](concord_example_stripe_sdk::StripeContext), a typed
//!   [`ProvisioningPlan`](provisioning::ProvisioningPlan), the application's
//!   [`IdempotencyPolicy`](provisioning::IdempotencyPolicy) and [`KeyLedger`](provisioning::KeyLedger),
//!   and returns a [`ProvisioningTrace`](provisioning::ProvisioningTrace).
//!
//! This crate uses only the SDK's public exports, `concord` and `concord-pagination`; the
//! binaries in `src/bin/` run the same functions against the live API when explicitly enabled
//! (see the README). Usage, compiled as a doctest:
//!
//! ```no_run
//! use std::num::NonZeroUsize;
//!
//! use concord_example_stripe_app::provisioning::{IdempotencyPolicy, KeyLedger, ProvisioningPlan, provision};
//! use concord_example_stripe_app::reconciliation::{Budgets, ReconcileInput, reconcile};
//! use concord_example_stripe_sdk::{AccountId, Limit, SecretKey, StripeContext};
//! use concord_pagination::Resume;
//!
//! # async fn run(plan: ProvisioningPlan) -> Result<(), Box<dyn std::error::Error>> {
//! let key = SecretKey::new(std::env::var("STRIPE_SECRET_KEY")?)?;
//! let stripe = StripeContext::new(concord::reqwest::Client::new(), &key)
//!     .with_account(Some(AccountId::new("acct_1Example")?));
//!
//! // ST-A: a bounded reconciliation; resume an interrupted section from its report.
//! let pages = Budgets::uniform(NonZeroUsize::new(5).unwrap());
//! let mut input = ReconcileInput::new(Limit::new(50)?, pages);
//! let report = reconcile(&stripe, &input).await;
//! if let Some(resume) = report.invoice_section.resume.clone() {
//!     input.invoices = Some(resume);
//!     input.balance = None;
//!     let rest = reconcile(&stripe, &input).await;
//!     println!("{} more invoices", rest.invoices.len());
//! }
//! input.invoices = Some(Resume::Start);
//!
//! // ST-B: application-owned keys and ledger; at most one unchanged repeat after a lost response.
//! let policy = IdempotencyPolicy::derived("onboarding-42", 1)?;
//! let mut ledger = KeyLedger::new();
//! let trace = provision(&stripe, &plan, &policy, &mut ledger).await;
//! for write in trace.attempted() {
//!     println!("{:?}: {:?} -> {:?}", write.step, write.attempts, write.result);
//! }
//! # Ok(()) }
//! ```

pub mod provisioning;
pub mod reconciliation;
