//! Its `KEY` is the public `x-public`; [`start!`] hands it on as `$crate::KEY`.

/// The public header name `x-public`.
pub const KEY: concord::reqwest::header::HeaderName =
    concord::reqwest::header::HeaderName::from_static("x-public");

/// Calls `$next!` with this crate's key, written `$crate::KEY`.
#[macro_export]
macro_rules! start {
    ($next:path) => {
        $next! { $crate::KEY }
    };
}
