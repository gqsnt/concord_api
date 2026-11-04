//! Staged maintenance exercise for the Stripe SDK slice (`concord-example-stripe-sdk`).
//!
//! An external application that uses only the SDK's public exports. [`before`] is the consumer
//! frozen before the SDK change (stage 1); it is never edited afterwards, so compiling and
//! running it against later SDK versions is the compatibility check. The procedure and findings
//! are recorded in `MAINTENANCE.md`.

pub mod before;
