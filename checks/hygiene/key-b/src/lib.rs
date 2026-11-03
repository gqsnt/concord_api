//! Its `KEY` is `authorization`; [`next!`] adds it, also written `$crate::KEY`, after the key it
//! receives and calls `finish!` (resolved where the macro is invoked).

/// The standard `authorization` header name.
pub const KEY: concord::reqwest::header::HeaderName =
    concord::reqwest::header::HeaderName::from_static("authorization");

/// Calls `finish!` with the key it receives and this crate's key, written `$crate::KEY`.
#[macro_export]
macro_rules! next {
    ($first:path) => {
        finish! { $first; $crate::KEY }
    };
}
