//! Staged maintenance exercise for `concord-example-github-sdk` (4.4, DX-01).
//!
//! A downstream application's view of the SDK, written only against its public exports:
//!
//! - [`before`]: the **frozen initial consumer** (stage 1). It is never edited after stage 1; it
//!   must keep compiling and passing against every later version of the SDK.
//! - [`after`]: the **updated consumer** (stage 3), using the stage 2 `type` filter and
//!   [`issue_events`], an operation added from outside the SDK through its public context.
//!
//! The procedure and the findings are in `MAINTENANCE.md` next to this crate's manifest.

pub mod after;
pub mod before;
pub mod issue_events;
