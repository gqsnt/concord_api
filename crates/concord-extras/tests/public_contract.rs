use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn package_metadata<'a>(metadata: &'a Value, name: &str) -> &'a Value {
    metadata["packages"]
        .as_array()
        .expect("Cargo metadata packages should be an array")
        .iter()
        .find(|package| package["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("Cargo metadata omitted package {name}"))
}

#[test]
fn extras_metadata_has_only_the_core_concord_edge() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("extras crate should be inside crates/ in the workspace");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(workspace)
        .output()
        .expect("Cargo metadata should run");
    assert!(
        output.status.success(),
        "Cargo metadata failed\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: Value =
        serde_json::from_slice(&output.stdout).expect("Cargo metadata should be valid JSON");
    let extras = package_metadata(&metadata, "concord-extras");
    let concord_edges = extras["dependencies"]
        .as_array()
        .expect("package dependencies should be an array")
        .iter()
        .filter(|dependency| {
            dependency["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("concord"))
        })
        .collect::<Vec<_>>();

    assert_eq!(
        concord_edges.len(),
        1,
        "extras must have exactly one Concord-package edge: {concord_edges:#?}"
    );
    let core = concord_edges[0];
    assert_eq!(core["name"].as_str(), Some("concord-core"));
    assert!(core["kind"].is_null(), "the core edge must be normal");
    assert_eq!(core["req"].as_str(), Some("=0.1.0"));
    assert_eq!(core["uses_default_features"].as_bool(), Some(false));
}

#[test]
fn extras_public_types_satisfy_only_semantic_core_contracts() {
    struct CursorPage {
        next: Option<String>,
    }

    impl concord_extras::pagination::HasNextCursor<String> for CursorPage {
        fn next_cursor(&self) -> Option<&String> {
            self.next.as_ref()
        }
    }

    #[cfg(feature = "json")]
    fn response_codec<T: concord_core::codec::DecodeBody>() {}
    #[cfg(feature = "json")]
    fn request_codec<T: concord_core::codec::EncodeBody>() {}
    fn controller<Page, T: concord_core::pagination::PaginationController<Page>>() {}
    #[cfg(any(feature = "csv", feature = "ndjson"))]
    fn decoder<T: concord_core::records::RecordDecoder>() {}
    #[cfg(feature = "oauth2-client")]
    fn provider<T: concord_core::contracts::CredentialProvider>() {}
    #[cfg(feature = "rate-limit-governor")]
    fn limiter<T: concord_core::rate_limit::RateLimiter>() {}
    #[cfg(feature = "rate-limit-governor")]
    fn observer<T: concord_core::rate_limit::RateLimitObserver>() {}

    controller::<(), concord_extras::pagination::OffsetPagination>();
    controller::<(), concord_extras::pagination::PagePagination>();
    controller::<CursorPage, concord_extras::pagination::CursorPagination<String>>();

    #[cfg(feature = "json")]
    {
        response_codec::<concord_extras::codec::Json<String>>();
        request_codec::<concord_extras::codec::Json<String>>();
    }
    #[cfg(feature = "csv")]
    decoder::<concord_extras::records::Csv<String>>();
    #[cfg(feature = "ndjson")]
    decoder::<concord_extras::records::NdJson<String>>();
    #[cfg(feature = "oauth2-client")]
    provider::<concord_extras::auth::OAuth2ClientCredentials>();
    #[cfg(feature = "rate-limit-governor")]
    {
        limiter::<concord_extras::rate_limit::GovernorRateLimiter>();
        observer::<concord_extras::rate_limit::DefaultRateLimitObserver>();
    }
}
