#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::path::Path;

const REQWEST: &str = include_str!("release/reqwest_assumptions.csv");
const SECURITY: &str = include_str!("release/security_sentinels.csv");
const PROPERTIES: &str = include_str!("release/properties.csv");

fn validate_registry(
    registry: &str,
    header: &str,
    expected_rows: usize,
    reference_column: usize,
) -> HashSet<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let mut lines = registry.lines();
    assert_eq!(lines.next(), Some(header));
    let expected_columns = header.split(',').count();
    let mut ids = HashSet::new();
    let mut count = 0_usize;
    for (index, line) in lines.enumerate() {
        let columns: Vec<_> = line.split(',').collect();
        assert_eq!(
            columns.len(),
            expected_columns,
            "invalid registry row {}",
            index + 2
        );
        assert!(
            ids.insert(columns[0].to_owned()),
            "duplicate {}",
            columns[0]
        );
        assert!(columns.iter().all(|value| !value.trim().is_empty()));
        let (path, marker) = columns[reference_column]
            .split_once('#')
            .expect("probe references use path#test");
        let path = root.join(path);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert!(
            source.contains(marker),
            "{} does not contain {marker:?}",
            path.display()
        );
        count += 1;
    }
    assert_eq!(count, expected_rows);
    ids
}

fn assert_role_selects_every_probe(registry: &str, reference_column: usize, role_name: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let justfile = std::fs::read_to_string(root.join("justfile")).expect("workspace justfile");
    let start_marker = format!("{role_name}:");
    let start = justfile
        .find(&start_marker)
        .unwrap_or_else(|| panic!("missing maintained role {role_name}"));
    let remainder = &justfile[start..];
    let end = remainder.find("\n[private]").unwrap_or(remainder.len());
    let role = &remainder[..end];

    for line in registry.lines().skip(1) {
        let columns: Vec<_> = line.split(',').collect();
        let (_, marker) = columns[reference_column]
            .split_once('#')
            .expect("probe references use path#test");
        assert!(
            role.contains(marker),
            "maintained role {role_name} does not select registered probe {marker}"
        );
    }
}

#[test]
fn selected_reqwest_assumptions_have_exact_executable_probes() {
    let ids = validate_registry(
        REQWEST,
        "assumption_id,selected_behavior,deterministic_probe,configuration",
        18,
        2,
    );
    for required in [
        "request_counts",
        "retry_disabled_default",
        "hidden_retry_containment",
        "body_replay_and_factory_invocation",
        "timeout_behavior",
        "logical_native_url",
        "query_authentication",
        "https_tls",
        "http2",
        "proxy_behavior",
        "response_streaming",
        "status_and_headers",
        "request_body_ownership",
        "response_body_ownership",
    ] {
        assert!(
            ids.contains(required),
            "missing Reqwest assumption {required}"
        );
    }

    let lock = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("Cargo.lock"),
    )
    .expect("workspace lockfile");
    let reqwest = lock
        .split("[[package]]")
        .find(|package| package.contains("name = \"reqwest\""))
        .expect("Reqwest package entry");
    assert!(reqwest.contains("version = \"0.13.4\""));
}

#[test]
fn whole_product_security_sentinel_registry_covers_every_required_class() {
    let ids = validate_registry(
        SECURITY,
        "sentinel_class,automatic_surfaces,positive_recipient,deterministic_probe",
        15,
        3,
    );
    for required in [
        "credentials_and_secrets",
        "authentication_header_and_query",
        "request_body",
        "multipart_values",
        "successful_response_body",
        "terminal_status_body",
        "raw_success_body",
        "native_authenticated_url",
        "pagination_cursor_and_identity",
        "rate_limit_key",
        "provider_values",
        "public_sensitive_headers",
        "continuation_path_and_query",
        "custom_controller_state",
        "collected_items",
    ] {
        assert!(ids.contains(required), "missing sentinel class {required}");
    }
    assert_role_selects_every_probe(SECURITY, 3, "_security-and-properties");
}

#[test]
fn release_property_registry_retains_reproducible_cases() {
    let ids = validate_registry(
        PROPERTIES,
        "property_id,seed_or_minimized_case,deterministic_probe,configuration",
        5,
        2,
    );
    assert_eq!(ids.len(), 5);
    assert_role_selects_every_probe(PROPERTIES, 2, "_security-and-properties");
}
