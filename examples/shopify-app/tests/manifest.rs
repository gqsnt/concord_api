//! The fixture manifest is complete and current: every fixture file is listed once, with the
//! sha256 of its bytes, an allowed provenance class, a source URL and a test command.
//!
//! `cargo test -p concord-example-shopify-app --test manifest`

mod support;

use std::collections::BTreeSet;

use serde_json::Value;
use support::{fixture, fixture_bytes, sha256_hex};

#[test]
fn sha256_matches_the_standard_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let long = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
    assert_eq!(
        sha256_hex(long),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

#[test]
fn every_fixture_is_listed_with_its_current_digest() {
    let manifest: Value = serde_json::from_str(&fixture("MANIFEST.json")).unwrap();
    let allowed = [
        "official_example",
        "schema_derived_synthetic",
        "fault_injection",
    ];
    let mut listed = BTreeSet::new();
    for entry in manifest["fixtures"].as_array().unwrap() {
        let file = entry["file"].as_str().unwrap();
        assert!(listed.insert(file.to_owned()), "{file} listed twice");
        assert_eq!(
            entry["sha256"].as_str().unwrap(),
            sha256_hex(&fixture_bytes(file)),
            "{file}"
        );
        assert!(
            allowed.contains(&entry["provenance"].as_str().unwrap()),
            "{file}"
        );
        assert_eq!(entry["provider_version"], "2026-10", "{file}");
        assert!(
            entry["authoritative_url"]
                .as_str()
                .unwrap()
                .starts_with("https://shopify.dev/")
        );
        assert!(
            entry["test_command"]
                .as_str()
                .unwrap()
                .starts_with("cargo test -p concord-example-shopify-app")
        );
        for key in [
            "workflow",
            "operation",
            "sdk_source",
            "expected_outcome",
            "wire_assertions",
            "schema",
        ] {
            assert!(!entry[key].is_null(), "{file}: {key}");
        }
    }
    let dir = format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"));
    let mut on_disk = BTreeSet::new();
    for sub in ["sh_a", "sh_b"] {
        for item in std::fs::read_dir(format!("{dir}/{sub}")).unwrap() {
            let name = item.unwrap().file_name().into_string().unwrap();
            on_disk.insert(format!("{sub}/{name}"));
        }
    }
    assert_eq!(listed, on_disk);
    // No real credential shapes: the synthetic tokens are not in any fixture.
    for file in &on_disk {
        let text = String::from_utf8_lossy(&fixture_bytes(file)).into_owned();
        assert!(!text.contains("shpat_"), "{file}");
    }
}
