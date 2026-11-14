//! Typed clients for tasks, regional Riot APIs, files, cursor feeds, and the shapes of one call.
//!
//! Start with [`Tasks::new`], [`Riot::new`], [`Files::new`], [`Feed::new`], or [`Shapes::new`].
//! Required inputs are arguments. Optional inputs have setters with their own names.
//! Send a paged call with `.send().await` for one page or `.pages()` for all pages.
//! Await any other call to receive its documented result.
//!
//! Calls and handles own their inputs, are cheap to clone, and are [`Send`] + [`Sync`].
//! Their types follow the API tree, such as [`riot::Matches`] and [`riot::matches::Ids`].

// Each client has a hidden trait for its input and problem types. Its implementation
// stays beside the authored code, where every type name resolves. Generated fields
// use projections through that trait, without copying the author's imports.

#![forbid(unsafe_code)]
#![deny(
    missing_docs,
    rustdoc::broken_intra_doc_links,
    missing_debug_implementations
)]

mod data;
#[doc(inline)]
pub use data::*;

// The author imported Secret for the Riot description.
use ::concord::Secret;

// A function item in a const checks the contract without running anything. Each
// input type gets one line, so a missing Clone points at `__input::<TheType>`.
const _: () = {
    fn __input<T: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static>() {}
    let _ = __input::<Secret>;
    let _ = __input::<MatchRoute>;
    let _ = __input::<AccountRoute>;
    let _ = __input::<::core::option::Option<bool>>;
    let _ = __input::<NewTask>;
    let _ = __input::<u64>;
    let _ = __input::<Puuid>;
    let _ = __input::<::core::option::Option<u32>>;
    let _ = __input::<::core::option::Option<i32>>;
    let _ = __input::<::core::option::Option<i64>>;
    let _ = __input::<MatchId>;
    let _ = __input::<String>;
    let _ = __input::<DocId>;
    let _ = __input::<FileId>;
    let _ = __input::<concord::Body>;
    let _ = __input::<::core::option::Option<String>>;
    let _ = __input::<Item>;
    let _ = __input::<Trace>;
    let _ = __input::<f64>;
    let _ = __input::<::core::option::Option<Secret>>;
    let _ = __input::<::core::option::Option<Filter>>;
};

include!("declarations.rs");

// Anonymous consts give each author's imports their own scope. Impl blocks still
// resolve at the description's level; there is no generated wildcard import.
include!("tasks.rs");
include!("riot.rs");
include!("files.rs");
include!("grant.rs");
include!("feed.rs");
include!("shapes.rs");

#[cfg(test)]
mod tests {
    use super::Files;
    use concord::transport::{TransportError, TryInfo};
    use concord::{http, reqwest, Middleware, Next};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn grant_form_uses_the_generated_secret_entry_adapter() {
        let call = Files::new("signer").grant("client_credentials", "client", "a&b");
        assert_eq!(
            call.__body().unwrap().as_bytes().unwrap(),
            b"grant_type=client_credentials&client_id=client&client_secret=a%26b"
        );
        assert!(!format!("{call:?}").contains("a&b"));
    }

    #[tokio::test]
    async fn c10_8_files_new_installs_signing_and_clones_share_middleware_after_transport_replacement(
    ) {
        struct Count(AtomicUsize);
        impl Middleware for Count {
            async fn handle(
                &self,
                mut request: reqwest::Request,
                info: TryInfo,
                next: Next<'_>,
            ) -> Result<reqwest::Response, TransportError> {
                // The caller runs before the signer.
                assert!(!request.headers().contains_key("x-signature"));
                let count = self.0.fetch_add(1, Ordering::SeqCst) + 1;
                request
                    .headers_mut()
                    .insert("x-count", count.to_string().parse().unwrap());
                next.send(request, info).await
            }
        }

        let files = Files::new("test-key")
            .with_transport(concord::fake(|_| panic!("the old transport was replaced")))
            .with_middleware(Count(AtomicUsize::new(0)))
            .with_transport(concord::fake(|request| {
                assert_eq!(request.headers()["x-signature"], "2e65ac4a944b7759");
                assert!(request.headers()["x-signature"].is_sensitive());
                let mut response = http::Response::new("");
                response
                    .headers_mut()
                    .insert("x-count", request.headers()["x-count"].clone());
                Ok(reqwest::Response::from(response))
            }));
        let cloned = files.clone();
        let default = concord::fake(|_| panic!("the configured transport must be used"));
        for (client, expected) in [(&files, "1"), (&cloned, "2")] {
            let request = reqwest::Request::new(
                http::Method::HEAD,
                "https://files.example.com/a".parse().unwrap(),
            );
            // Exercise the actual chain without entering the stub exchange engine.
            let response = client
                .__core
                .next(&default)
                .send(request, TryInfo::new("files.exists", 1))
                .await
                .unwrap();
            assert_eq!(response.headers()["x-count"], expected);
        }
    }
}
