//! Compile-only real API descriptions with deliberately inert model stubs.
#![allow(dead_code, unused_imports)]
pub mod wire {
    //! Inert body writers required by page examples.
    /// Write an inert body.
    pub fn xml<T>(_: T) -> concord::Body {
        concord::Body::new("", "application/xml")
    }
}
/// Compile probe for `again_kubernetes`.
pub mod again_kubernetes;
/// Compile probe for `again_slack`.
pub mod again_slack;
/// Compile probe for `again_telegram`.
pub mod again_telegram;
/// Compile probe for `elasticsearch`.
pub mod elasticsearch;
/// Compile probe for `github`.
pub mod github;
/// Compile probe for `kubernetes`.
pub mod kubernetes;
/// Compile probe for `riot`.
pub mod riot;
/// Compile probe for `s3`.
pub mod s3;
/// Compile probe for `slack`.
pub mod slack;
/// Compile probe for `stripe`.
pub mod stripe;
/// Compile probe for `telegram`.
pub mod telegram;

/// Names explicitly qualified by the S3 page.
pub mod model {
    /// The page's problem type.
    pub type S3Problem = crate::s3::S3Problem;
}

/// Compile-only roots from the guide pages.
pub mod guide;
