//! An external caller of the Stripe SDK slice: the two required workflows as library functions.
//!
//! - [`provisioning`] — **ST-B**, form and idempotency scenario: [`provisioning::provision`]
//!   takes a configured [`StripeContext`](concord_example_stripe_sdk::StripeContext), a typed
//!   [`ProvisioningPlan`](provisioning::ProvisioningPlan), the application's
//!   [`IdempotencyPolicy`](provisioning::IdempotencyPolicy) and [`KeyLedger`](provisioning::KeyLedger),
//!   and returns a [`ProvisioningTrace`](provisioning::ProvisioningTrace).
//!
//! - [`reconciliation`] — **ST-A**, billing reconciliation: [`reconciliation::reconcile`] takes a
//!   configured context and a [`ReconcileInput`](reconciliation::ReconcileInput) (selectors,
//!   budgets, resume points) and returns a
//!   [`ReconciliationReport`](reconciliation::ReconciliationReport). Its traversals are
//!   `concord_pagination::sequential` chains over the SDK's cursor adapter.
//!
//! This crate uses only the SDK's public exports and `concord`; the binaries in `src/bin/` run
//! the same functions against the live API when explicitly enabled (see the README).

pub mod provisioning;
pub mod reconciliation;
