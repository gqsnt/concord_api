//! Every whole description in the guide, reference overview, root README, and
//! eight API example pages is checked against its page and compiled by the macro.

pub mod elasticsearch;
pub mod github;
pub mod kubernetes;
pub mod riot;
pub mod s3;
pub mod slack;
pub mod stripe;
pub mod telegram;

/// Current whole descriptions and a registry of their compiled client types.
pub mod guide;

/// Compiled regression clients retained independently of current pages.
#[doc(hidden)]
pub mod regressions;
