//! Staged maintenance exercise for the Kubernetes SDK (`concord-example-kubernetes-sdk`).
//!
//! [`before`] is the frozen initial consumer, written against the SDK's public exports before the
//! SDK changed; it is never edited afterwards. [`after`] is the updated consumer: it uses the new
//! Ingress family and a changed selector, reusing the frozen consumer's generic list and watch
//! helpers. See `MAINTENANCE.md` for the task, the stages and the findings.

pub mod after;
pub mod before;
