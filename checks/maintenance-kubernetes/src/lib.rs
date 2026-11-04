//! Staged maintenance exercise for the Kubernetes SDK (`concord-example-kubernetes-sdk`).
//!
//! [`before`] is the frozen initial consumer, written against the SDK's public exports before the
//! SDK changed; it is never edited afterwards. See `MAINTENANCE.md` for the task, the stages and
//! the findings.

pub mod before;
