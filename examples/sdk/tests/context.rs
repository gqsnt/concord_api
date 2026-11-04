//! The public context boundary agrees with the generated endpoints it sits beside: same base and
//! prefix, same path encoding and refusals, same service headers, independent snapshots.

use concord::reqwest::header::ACCEPT;
use concord::reqwest::{self, Method};
use concord::{Decoder, ErrorKind, Json};
use concord_example_fixture::services;
use concord_example_sdk::context::{CatalogContext, ContextError};
use concord_example_sdk::{CatalogConfig, Item};

fn context(key: &str) -> CatalogContext {
    CatalogContext::new(reqwest::Client::new(), CatalogConfig { key: key.into() })
}

/// Tenants and ids covering every ASCII character, controls, percent escapes, dots and Unicode.
fn hard_values() -> Vec<String> {
    let mut values: Vec<String> = (0u8..=0x7F)
        .map(|b| format!("a{}b", char::from(b)))
        .collect();
    values.extend(
        [
            "acme",
            "acme/east?x=1%20+ü #",
            "%2F",
            "%",
            "...",
            ".a",
            "a.",
            " ",
            "~",
            "ü",
            "東京",
            "emoji-🦀",
            "a\\b",
            "\u{7f}",
            "line\nbreak",
            "tab\tsep",
            "semi;colon=1",
        ]
        .map(str::to_owned),
    );
    values
}

#[test]
fn built_requests_agree_with_generated_endpoints() {
    let bases = [
        None,
        Some("http://127.0.0.1:9/edge/catalog/v1"),
        Some("http://127.0.0.1:9/edge/catalog/v1/"),
        Some("http://127.0.0.1:9"),
        Some("https://catalog.example.test/a%20b/v1"),
    ];
    for base in bases {
        for region in [None, Some("north".to_owned())] {
            let mut ctx = context("k-1").maybe_region(region.clone());
            if let Some(base) = base {
                ctx = ctx.with_base_url(base).unwrap();
            }
            for value in hard_values() {
                let tenant = ctx.tenant(value.as_str());
                let generated = tenant
                    .catalog_tenant()
                    .item(value.as_str())
                    .prepare()
                    .unwrap();
                let generated = generated.request();
                let built = tenant
                    .request(Method::GET, &["items", value.as_str()])
                    .unwrap();
                assert_eq!(
                    built.url(),
                    generated.url(),
                    "base {base:?}, value {value:?}"
                );
                assert_eq!(built.method(), generated.method());

                // Same headers, except the `Accept` that the generated endpoint's JSON format
                // suggests (a built request leaves `Accept` to the operation).
                let mut generated_headers = generated.headers().clone();
                generated_headers.remove(ACCEPT);
                assert_eq!(built.headers(), &generated_headers);
                for (name, value) in built.headers() {
                    assert_eq!(
                        value.is_sensitive(),
                        generated.headers()[name].is_sensitive(),
                        "sensitivity of {name}"
                    );
                }
                assert!(built.headers()["x-api-key"].is_sensitive());
                assert_eq!(
                    built.headers().get("x-region").map(|v| v.to_str().unwrap()),
                    region.as_deref()
                );
            }
        }
    }
}

#[test]
fn client_level_requests_agree_with_generated_endpoints() {
    let ctx = context("k-1")
        .with_base_url("http://127.0.0.1:9/edge/catalog/v1/")
        .unwrap();
    for value in hard_values() {
        let generated = ctx.catalog().activity(value.as_str()).prepare().unwrap();
        let built = ctx
            .request(Method::GET, &["accounts", value.as_str(), "activity"])
            .unwrap();
        assert_eq!(built.url(), generated.request().url(), "value {value:?}");
    }
    // No segments: the base URL exactly as given.
    let bare = ctx.request(Method::GET, &[]).unwrap();
    assert_eq!(bare.url().as_str(), "http://127.0.0.1:9/edge/catalog/v1/");
}

#[test]
fn empty_and_dot_segments_are_refused_like_generated_endpoints() {
    let ctx = context("k-1");
    for bad in ["", ".", ".."] {
        let tenant = ctx.tenant(bad);
        assert_eq!(
            tenant.request(Method::GET, &["items", "x"]).unwrap_err(),
            ContextError::Tenant
        );
        let generated = tenant.catalog_tenant().item("x").prepare().unwrap_err();
        assert_eq!(generated.kind(), ErrorKind::Build);

        let good = ctx.tenant("acme");
        assert_eq!(
            good.request(Method::GET, &["items", bad]).unwrap_err(),
            ContextError::Segment { index: 1 }
        );
        let generated = good.catalog_tenant().item(bad).prepare().unwrap_err();
        assert_eq!(generated.kind(), ErrorKind::Build);
    }
    // Only exact dot segments: other dotted text is ordinary data.
    for fine in ["...", ".a", "a."] {
        assert!(ctx.tenant(fine).request(Method::GET, &[fine]).is_ok());
    }
}

#[test]
fn invalid_header_values_are_refused_like_generated_endpoints() {
    let bad_key = context("k\n1").tenant("acme");
    assert_eq!(
        bad_key.request(Method::GET, &["items"]).unwrap_err(),
        ContextError::HeaderValue { name: "x-api-key" }
    );
    let generated = bad_key.catalog_tenant().item("x").prepare().unwrap_err();
    assert_eq!(generated.kind(), ErrorKind::Build);

    let bad_region = context("k-1").set_region("north\r\n").tenant("acme");
    assert_eq!(
        bad_region.request(Method::GET, &["items"]).unwrap_err(),
        ContextError::HeaderValue { name: "x-region" }
    );
    let generated = bad_region.catalog_tenant().item("x").prepare().unwrap_err();
    assert_eq!(generated.kind(), ErrorKind::Build);

    let shown = ContextError::HeaderValue { name: "x-api-key" }.to_string();
    assert_eq!(
        shown,
        "Catalog context: request not built: x-api-key value is not a valid header value"
    );
}

#[test]
fn base_urls_are_validated_by_concord() {
    for bad in [
        "ftp://catalog.example.test/v1",
        "https://user:pw@catalog.example.test/v1",
        "https://catalog.example.test/v1?x=1",
        "https://catalog.example.test/v1#f",
        "catalog.example.test/v1",
    ] {
        let from_context = context("k-1").with_base_url(bad).unwrap_err();
        let from_handle = context("k-1")
            .catalog()
            .clone()
            .with_base_url(bad)
            .unwrap_err();
        assert_eq!(from_context.kind(), ErrorKind::Build);
        assert_eq!(from_context.endpoint(), None);
        assert_eq!(from_context.to_string(), from_handle.to_string());
    }
}

#[test]
fn contexts_are_snapshots_and_their_handles_follow_them() {
    let base = context("k-1")
        .with_base_url("http://127.0.0.1:9/catalog/v1")
        .unwrap();
    let north = base.clone().set_region("north");
    let none = north.clone().maybe_region(None);

    assert_eq!(base.region(), None);
    assert_eq!(north.region(), Some("north"));
    assert_eq!(none.region(), None);

    let region_of = |ctx: &CatalogContext| {
        let tenant = ctx.tenant("acme");
        let built = tenant.request(Method::GET, &["items"]).unwrap();
        let generated = tenant.catalog_tenant().item("x").prepare().unwrap();
        let built = built.headers().get("x-region").cloned();
        let generated = generated.request().headers().get("x-region").cloned();
        assert_eq!(built, generated);
        built.map(|v| v.to_str().unwrap().to_owned())
    };
    assert_eq!(region_of(&base), None);
    assert_eq!(region_of(&north), Some("north".to_owned()));
    assert_eq!(region_of(&none), None);

    // A tenant context keeps the context it was created from.
    let acme = north.tenant("acme");
    let _moved = north.clone().set_region("south");
    let built = acme.request(Method::GET, &["items"]).unwrap();
    assert_eq!(built.headers()["x-region"], "north");
    assert_eq!(acme.context().region(), Some("north"));
    assert_eq!(acme.tenant(), "acme");
}

#[test]
fn debug_withholds_the_key_but_the_native_request_shows_the_url() {
    let ctx = context("key-s3cr3t")
        .with_base_url("http://127.0.0.1:9/catalog/v1")
        .unwrap()
        .set_region("north");
    let tenant = ctx.tenant("tenant-p4th");

    let shown = format!("{ctx:?}");
    assert_eq!(
        shown,
        r#"CatalogContext { base: "http://127.0.0.1:9/catalog/v1", key: <redacted>, region: Some("north") }"#
    );
    let shown = format!("{tenant:?} {tenant:#?}");
    assert!(!shown.contains("key-s3cr3t"), "{shown}");
    assert!(shown.contains("tenant-p4th"));

    // The documented boundary: the native request is not Concord output.
    let request = tenant.request(Method::GET, &["items"]).unwrap();
    let native = format!("{request:?}");
    assert!(native.contains("tenant-p4th"), "{native}");
    assert!(!native.contains("key-s3cr3t"), "{native}");
    assert!(native.contains("Sensitive"), "{native}");

    let error = ctx.tenant("..").request(Method::GET, &[]).unwrap_err();
    assert_eq!(
        format!("{error} {error:?}"),
        "Catalog context: request not built: tenant is empty, `.` or `..` Tenant"
    );
}

#[tokio::test]
async fn a_built_request_reaches_the_service_like_a_declared_one() {
    let server = services().await;
    let tenant = context("k-1")
        .with_base_url(format!("{}/catalog/v1", server.url()))
        .unwrap()
        .set_region("south")
        .tenant("acme/east");

    let declared = tenant
        .catalog_tenant()
        .item("item-002")
        .send()
        .await
        .unwrap();
    let request = tenant.request(Method::GET, &["items", "item-002"]).unwrap();
    let decoder = Decoder::<Item>::new::<Json>();
    let built = decoder
        .finish(tenant.http().execute(request).await)
        .await
        .unwrap();

    assert_eq!(built, declared);
    let seen = server.requests();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].target, seen[1].target);
    assert_eq!(
        seen[1].target,
        "/catalog/v1/tenants/acme%2Feast/items/item-002"
    );
    for name in ["x-client", "x-api-key", "x-region"] {
        assert_eq!(seen[0].header(name), seen[1].header(name), "{name}");
    }
    assert_eq!(seen[1].header("x-region"), Some("south"));
}
