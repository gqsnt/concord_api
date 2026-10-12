//! Export, check, diff and explanation tests over the Catalog-like corpus
//! (V3-O02, V3-O03).

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord_core::contracts::{
    Buffered, Bytes, ResponseBodyError, ResponseHead, ResponseRepresentation,
};
use concord_core::inspection::{
    ContractDescription, EndpointSpec, OwnerSpec, RepresentationSelection, ResultSpec,
};
use serde_json::Value;

use crate::fixtures::{CatalogMutation, catalog};
use crate::{
    Axis, DiffReport, InspectErrorKind, Verdict, check, diff_documents, explain_endpoint,
    explain_policy, export_string, export_value, parse_document, to_canonical_string,
};

fn diff(before: CatalogMutation, after: CatalogMutation) -> DiffReport {
    diff_documents(
        &export_value(&catalog(before)),
        &export_value(&catalog(after)),
    )
    .expect("valid documents")
}

#[test]
fn r7_duplicate_owner_cannot_disappear_into_snapshot_equality() {
    let before = export_value(&catalog(CatalogMutation::Baseline));
    let mut after = before.clone();
    let owners = after["owners"].as_array_mut().unwrap();
    let mut duplicate = owners[0].clone();
    duplicate["r7_private_sentinel"] = serde_json::json!("private-body-sentinel");
    owners.insert(0, duplicate);
    let result = diff_documents(&before, &after);
    assert!(
        result.is_err(),
        "duplicate owner was silently accepted: {result:?}"
    );
}

#[test]
fn r7_indexed_keys_reject_identical_changed_reordered_and_nested_duplicates_safely() {
    let baseline = export_value(&catalog(CatalogMutation::Baseline));
    for (collection, fields) in [
        ("owners", &["path"][..]),
        ("endpoints", &["path"][..]),
        ("policies", &["id"][..]),
        ("window_sets", &["name"][..]),
        ("dependencies", &["kind", "reference"][..]),
        ("diagnostics", &["code", "owner", "subject"][..]),
    ] {
        for changed in [false, true] {
            for reverse in [false, true] {
                let mut document = baseline.clone();
                let mut record = serde_json::json!({});
                for field in fields {
                    record[*field] = serde_json::json!("private-key-sentinel");
                }
                let mut duplicate = record.clone();
                if changed {
                    duplicate["unknown"] = serde_json::json!("private-body-sentinel");
                }
                let pair = if reverse {
                    vec![duplicate, record]
                } else {
                    vec![record, duplicate]
                };
                document[collection] = serde_json::json!(pair);
                if collection == "diagnostics" && changed {
                    // Full represented identity preserves a changed unknown
                    // field as a distinct diagnostic, rather than losing it.
                    crate::validate_document(&document).expect("distinct represented diagnostics");
                    continue;
                }
                let error = crate::validate_document(&document).expect_err("duplicate indexed key");
                assert_eq!(error.kind(), InspectErrorKind::DuplicateKey);
                let rendered = format!("{error} {error:?}");
                assert!(!rendered.contains("private-key-sentinel"));
                assert!(!rendered.contains("private-body-sentinel"));
            }
        }
    }
    for (collection, key) in [
        ("values", "name"),
        ("secrets", "name"),
        ("credentials", "name"),
        ("rate", "id"),
        ("coverage", "id"),
    ] {
        for records in [
            serde_json::json!([{key: "x"}, {key: "x"}]),
            serde_json::json!([{}, {key: "#0"}]),
        ] {
            let mut document = baseline.clone();
            document["client"][collection] = records;
            assert_eq!(
                crate::validate_document(&document).unwrap_err().kind(),
                InspectErrorKind::DuplicateKey
            );
        }
        let mut malformed = baseline.clone();
        malformed["client"][collection] = serde_json::json!([{key: 123}]);
        assert_eq!(
            crate::validate_document(&malformed).unwrap_err().kind(),
            InspectErrorKind::InvalidRecordKey
        );
    }
    assert!(crate::validate_document(&baseline).is_ok());
    assert_eq!(
        parse_document(&to_canonical_string(&baseline)).unwrap(),
        baseline
    );
}

fn field_changes<'a>(report: &'a DiffReport, record: &str, field: &str) -> Vec<&'a crate::Change> {
    report
        .changes()
        .iter()
        .filter(|change| change.record() == record && change.field() == field)
        .collect()
}

fn endpoint_ids(report: &DiffReport) -> BTreeSet<String> {
    report
        .changed_endpoints()
        .iter()
        .map(|endpoint| endpoint.id().to_owned())
        .collect()
}

#[test]
fn export_has_the_versioned_top_level_shape() {
    let document = export_value(&catalog(CatalogMutation::Baseline));
    let keys = document
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        keys,
        BTreeSet::from([
            "client",
            "dependencies",
            "diagnostics",
            "endpoints",
            "format",
            "owners",
            "policies",
            "window_sets"
        ])
    );
    assert_eq!(document["format"], "concord.contract/3");
    crate::validate_document(&document).expect("valid document");
    for endpoint in document["endpoints"].as_array().expect("endpoints") {
        assert!(
            endpoint["path"].is_string(),
            "every endpoint has a source path"
        );
        assert!(
            endpoint["id"]["value"].is_string(),
            "every endpoint has an ID"
        );
    }
    for owner in document["owners"].as_array().expect("owners") {
        assert!(owner["path"].is_string());
    }
    for policy in document["policies"].as_array().expect("policies") {
        assert!(policy["id"].is_string());
    }
    let list = document["endpoints"]
        .as_array()
        .and_then(|endpoints| {
            endpoints
                .iter()
                .find(|endpoint| endpoint["id"]["value"] == "catalog.items.list")
        })
        .expect("list");
    assert_eq!(list["timeout"]["duration"]["secs"], 20);
    assert_eq!(list["timeout"]["duration"]["nanos"], 0);
    assert_eq!(list["rate"][0]["windows_canonical"][0]["capacity"], 120);
    assert_eq!(
        list["rate"][0]["windows_canonical"][0]["duration"]["secs"],
        60
    );
    assert_eq!(
        list["route"]["url_template"],
        "https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}/items"
    );
}

#[test]
fn export_is_byte_identical_sorted_and_order_preserving() {
    let first = export_string(&catalog(CatalogMutation::Baseline));
    let second = export_string(&catalog(CatalogMutation::Baseline));
    assert_eq!(first, second, "exports are byte-identical across builds");
    let reparsed = parse_document(&first).expect("parse");
    assert_eq!(
        to_canonical_string(&reparsed),
        first,
        "keys are already sorted"
    );
    fn keys_sorted(value: &Value) -> bool {
        match value {
            Value::Object(map) => {
                let keys = map.keys().collect::<Vec<_>>();
                let mut sorted = keys.clone();
                sorted.sort();
                keys == sorted && map.values().all(keys_sorted)
            }
            Value::Array(items) => items.iter().all(keys_sorted),
            _ => true,
        }
    }
    assert!(keys_sorted(
        &serde_json::from_str::<Value>(&first).expect("json")
    ));
    let object_keys = first
        .lines()
        .filter(|line| line.starts_with("  \""))
        .map(|line| line.trim().split('"').nth(1).unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    let mut sorted_keys = object_keys.clone();
    sorted_keys.sort();
    assert_eq!(
        object_keys, sorted_keys,
        "top-level keys are emitted in sorted order"
    );
    let list = reparsed["endpoints"]
        .as_array()
        .and_then(|endpoints| {
            endpoints
                .iter()
                .find(|endpoint| endpoint["id"]["value"] == "catalog.items.list")
        })
        .expect("list");
    let query = list["query"]
        .as_array()
        .expect("query")
        .iter()
        .map(|operation| operation["key"].as_str().expect("key"))
        .collect::<Vec<_>>();
    assert_eq!(
        query,
        ["tenant", "locale", "category", "offset", "count", "tag"]
    );
    assert!(first.ends_with("}\n"));
}

#[test]
fn export_is_value_free_and_redacts_sensitive_literals() {
    const SENTINEL: &str = "catalog-sentinel-5f1e-hidden";
    let contract = catalog(CatalogMutation::SensitiveLiterals(SENTINEL));
    let text = export_string(&contract);
    assert!(
        !text.contains(SENTINEL),
        "sensitive literal leaked into export"
    );
    assert!(!text.contains("/home/") && !text.contains("\\\\Users") && !text.contains("C:\\"));
    assert!(text.contains("\"redacted\": true"));
    assert!(text.contains("\"classification\": \"sensitive\""));
    assert!(text.contains("CatalogApi#values.audit_tag.initializer"));
    let document = parse_document(&text).expect("integer/string-only document");
    let dependencies = document["dependencies"].as_array().expect("dependencies");
    let redacted = dependencies
        .iter()
        .filter(|dependency| dependency["kind"] == "redacted_literal")
        .map(|dependency| dependency["reference"].as_str().expect("reference"))
        .collect::<Vec<_>>();
    assert_eq!(redacted.len(), 2);
    assert!(redacted.contains(&"CatalogApi#values.audit_tag.initializer"));
    assert!(
        redacted
            .iter()
            .any(|location| location.starts_with("CatalogApi#headers["))
    );
    // Ordinary declared literals remain present.
    assert!(text.contains("\"text\": \"3\""));
}

static CONVERTER_CALLS: AtomicUsize = AtomicUsize::new(0);

struct VendorJson;

impl ResponseRepresentation<String> for VendorJson {
    type Driver = Buffered;
    const ACCEPT: Option<&'static str> = Some("application/vnd.catalog+json; version=3");

    fn validate(_head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        CONVERTER_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn from_body(_body: Bytes, _head: &ResponseHead<'_>) -> Result<String, ResponseBodyError> {
        CONVERTER_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(String::new())
    }
}

#[test]
fn static_accept_is_exported_without_converter_calls() {
    let contract =
        ContractDescription::builder(
            OwnerSpec::client("Vendor")
                .response_default(RepresentationSelection::Type("VendorJson")),
            "https://vendor.example.test/",
        )
        .endpoint(EndpointSpec::new("Vendor::read", "GET").result(
            ResultSpec::from_representation::<VendorJson, String>("String"),
        ))
        .build()
        .expect("vendor contract");
    let document = export_value(&contract);
    assert_eq!(
        document["endpoints"][0]["result"]["accept"],
        "application/vnd.catalog+json; version=3"
    );
    assert_eq!(document["endpoints"][0]["result"]["driver"], "buffered");
    assert_eq!(CONVERTER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn check_is_equal_for_the_selected_snapshot_and_reports_differences() {
    let baseline = catalog(CatalogMutation::Baseline);
    let snapshot = export_string(&baseline);
    let report = check(&baseline, &snapshot).expect("check");
    assert_eq!(report.verdict(), Verdict::StructurallyEqual);
    assert!(report.is_structurally_equal());

    let changed = catalog(CatalogMutation::ChangeCost);
    let report = check(&changed, &snapshot).expect("check");
    assert_eq!(report.verdict(), Verdict::Different);
    assert!(endpoint_ids(&report).contains("catalog.item.update"));
    let text = report.to_text();
    assert!(text.contains("catalog.item.update (CatalogApi::regional::update_item)"));
    assert!(text.contains("RuntimePolicy"));
}

#[test]
fn mutation_corpus_detects_and_classifies_every_edit() {
    use Axis::*;
    use CatalogMutation as M;
    /// (mutation, required axes, forbidden axes, endpoints that must be named)
    type Case = (M, &'static [Axis], &'static [Axis], &'static [&'static str]);
    let cases: [Case; 16] = [
        (
            M::SwitchCredentialAtSameHeader,
            &[AuthAuthority],
            &[Wire],
            &["catalog.auth.login", "catalog.admin.stats"],
        ),
        (
            M::ChangeLiteralKeepKey,
            &[Wire],
            &[IdentityAndRateSchema],
            &["catalog.items.list"],
        ),
        (
            M::RemapValue,
            &[Wire],
            &[AuthAuthority],
            &["catalog.item.inspect"],
        ),
        (
            M::RemoveRepresentationFence,
            &[Provenance],
            &[Wire, CallerApi],
            &["catalog.archive.download"],
        ),
        (
            M::ChangeDefaultExpression,
            &[PresenceAndDefaults, OpaqueDependency],
            &[Wire],
            &["catalog.items.list"],
        ),
        (
            M::RenameRateLabel,
            &[IdentityAndRateSchema],
            &[Wire],
            &["catalog.item.update"],
        ),
        (
            M::SplitSharing,
            &[SharingCoverage, IdentityAndRateSchema],
            &[Wire],
            &["catalog.admin.audit", "catalog.admin.stats"],
        ),
        (
            M::ChangeCost,
            &[RuntimePolicy],
            &[IdentityAndRateSchema, SharingCoverage, Wire],
            &["catalog.item.update"],
        ),
        (
            M::ChangeKeyCodec,
            &[IdentityAndRateSchema, OpaqueDependency],
            &[Wire],
            &["catalog.items.list"],
        ),
        (
            M::MoveEndpoint,
            &[CallerApi, IdentityAndRateSchema, Provenance],
            &[],
            &["catalog_api.ping"],
        ),
        (
            M::AddDefault,
            &[PresenceAndDefaults, CallerApi],
            &[Wire],
            &["catalog.admin.stats"],
        ),
        (
            M::HoistQuota,
            &[SharingCoverage, Provenance],
            &[Wire],
            &["catalog_api.admin.health"],
        ),
        (
            M::RenameScope,
            &[CallerApi, Provenance, IdentityAndRateSchema],
            &[Wire],
            &["catalog.admin.stats", "catalog_api.operations.health"],
        ),
        (
            M::InsertSameNamedAncestorValue,
            &[Wire, CallerApi],
            &[IdentityAndRateSchema],
            &["catalog.item.get"],
        ),
        (
            M::MergeEqualDefinitions,
            &[SharingCoverage, IdentityAndRateSchema],
            &[Wire],
            &["catalog.admin.stats"],
        ),
        // RATE-02: a window edit is reported on the runtime-policy and
        // schema axes, never as a sharing/coverage or wire change.
        (
            M::ChangeWindow,
            &[RuntimePolicy, IdentityAndRateSchema],
            &[SharingCoverage, Wire],
            &["catalog.items.list", "catalog.item.get"],
        ),
    ];
    assert_eq!(cases.len(), CatalogMutation::CORPUS.len());
    for (mutation, required, forbidden, endpoints) in cases {
        let report = diff(M::Baseline, mutation);
        assert_eq!(
            report.verdict(),
            Verdict::Different,
            "{mutation:?} was not detected"
        );
        let axes = report.axes();
        for axis in required {
            assert!(
                axes.contains(axis),
                "{mutation:?} missing axis {axis:?}; got {axes:?}"
            );
        }
        for axis in forbidden {
            let wrong = report
                .changes_on(*axis)
                .iter()
                .map(|change| {
                    format!(
                        "{}:{}:{}",
                        change.collection(),
                        change.record(),
                        change.field()
                    )
                })
                .collect::<Vec<_>>();
            assert!(
                wrong.is_empty(),
                "{mutation:?} wrongly classified {axis:?}: {wrong:?}"
            );
        }
        assert!(
            !axes.contains(&Unknown),
            "{mutation:?} produced an unclassified change"
        );
        let ids = endpoint_ids(&report);
        for endpoint in endpoints {
            assert!(
                ids.contains(*endpoint),
                "{mutation:?} did not name {endpoint}; got {ids:?}"
            );
        }
        for change in report.changes() {
            assert!(!change.axes().is_empty());
        }
    }
}

#[test]
fn credential_switch_at_same_header_is_an_authority_change_with_origins() {
    let report = diff(
        CatalogMutation::Baseline,
        CatalogMutation::SwitchCredentialAtSameHeader,
    );
    let changes = field_changes(
        &report,
        "CatalogApi::authenticate",
        "auth.placements[0].credential",
    );
    assert_eq!(changes.len(), 1);
    let change = changes[0];
    assert_eq!(change.axes(), [Axis::AuthAuthority]);
    assert_eq!(change.before(), Some(&Value::from("application_key")));
    assert_eq!(change.after(), Some(&Value::from("partner_key")));
    assert_eq!(change.before_origin(), Some("CatalogApi"));
    assert_eq!(change.after_origin(), Some("CatalogApi"));
    assert!(
        field_changes(
            &report,
            "CatalogApi::authenticate",
            "auth.placements[0].target.name"
        )
        .is_empty()
    );
}

#[test]
fn literal_and_remapped_values_are_wire_changes_without_key_changes() {
    let report = diff(
        CatalogMutation::Baseline,
        CatalogMutation::ChangeLiteralKeepKey,
    );
    let change = report
        .changes()
        .iter()
        .find(|change| {
            change.record() == "CatalogApi::regional::item_summaries"
                && change.field().ends_with("emission.atom.text")
        })
        .expect("literal change on an inheriting endpoint");
    assert_eq!(change.before(), Some(&Value::from("3")));
    assert_eq!(change.after(), Some(&Value::from("4")));
    assert_eq!(change.axes(), [Axis::Wire]);
    assert!(
        !report
            .changes()
            .iter()
            .any(|change| change.field().ends_with(".key"))
    );

    let report = diff(CatalogMutation::Baseline, CatalogMutation::RemapValue);
    let change = report
        .changes()
        .iter()
        .find(|change| {
            change.record() == "CatalogApi::regional::inspect"
                && change.field() == "query[3].emission.atom.value.owner"
        })
        .expect("remapped owner");
    assert_eq!(
        change.before(),
        Some(&Value::from("CatalogApi::regional::inspect"))
    );
    assert_eq!(change.after(), Some(&Value::from("CatalogApi::regional")));
}

#[test]
fn rate_edits_are_separated_into_schema_cost_and_sharing_axes() {
    let label = diff(CatalogMutation::Baseline, CatalogMutation::RenameRateLabel);
    let change = label
        .changes()
        .iter()
        .find(|change| change.field() == "rate[catalog.item.write].dimensions[2].label")
        .expect("label change");
    assert_eq!(change.before(), Some(&Value::from("client.tenant")));
    assert_eq!(change.after(), Some(&Value::from("tenant")));
    assert!(change.axes().contains(&Axis::IdentityAndRateSchema));
    assert!(
        !label.has_axis(Axis::SharingCoverage),
        "a label rename keeps the partition"
    );

    let split = diff(CatalogMutation::Baseline, CatalogMutation::SplitSharing);
    assert!(
        split
            .changes()
            .iter()
            .any(|change| change.collection() == "policies"
                && change.record() == "catalog.admin.audit"
                && change.field().is_empty())
    );
    assert!(
        split
            .changes_on(Axis::SharingCoverage)
            .iter()
            .any(|change| change.record() == "catalog.admin.read")
    );
    let merge = diff(CatalogMutation::SplitSharing, CatalogMutation::Baseline);
    assert!(
        merge.has_axis(Axis::SharingCoverage),
        "merging sharing is detected"
    );
    assert!(endpoint_ids(&merge).contains("catalog.admin.audit"));

    let hoist = diff(CatalogMutation::Baseline, CatalogMutation::HoistQuota);
    assert!(
        hoist
            .changes()
            .iter()
            .any(|change| change.record() == "catalog.admin.read"
                && change.field() == "coverage[catalog_api.admin.health]")
    );
}

/// RATE-02 (§12.2): a window edit (120/1m -> 100/1m on `tenant_reads`) is
/// shown separately from coverage and sharing: the policy schema and every
/// covered endpoint's effective windows change on the runtime-policy and
/// schema axes, while coverage, sharing, cost and wire stay unchanged.
#[test]
fn window_edits_are_reported_separately_from_coverage_and_sharing() {
    let report = diff(CatalogMutation::Baseline, CatalogMutation::ChangeWindow);
    assert_eq!(report.verdict(), Verdict::Different);
    // The policy schema records the edit once.
    let schema = field_changes(
        &report,
        "catalog.tenant.read",
        "schema.windows_canonical[0].capacity",
    );
    assert_eq!(schema.len(), 1);
    assert_eq!(schema[0].collection(), "policies");
    assert_eq!(schema[0].before(), Some(&Value::from(120)));
    assert_eq!(schema[0].after(), Some(&Value::from(100)));
    assert!(schema[0].axes().contains(&Axis::IdentityAndRateSchema));
    assert!(schema[0].axes().contains(&Axis::RuntimePolicy));
    // Every endpoint covered by the policy shows its new effective window.
    let covered = [
        "CatalogApi::regional::archive::download",
        "CatalogApi::regional::export",
        "CatalogApi::regional::feed",
        "CatalogApi::regional::find_item",
        "CatalogApi::regional::inspect",
        "CatalogApi::regional::item",
        "CatalogApi::regional::item_summaries",
        "CatalogApi::regional::ping",
        "CatalogApi::regional::update_item",
    ];
    for endpoint in covered {
        let window = field_changes(
            &report,
            endpoint,
            "rate[catalog.tenant.read].windows_canonical[0].capacity",
        );
        assert_eq!(window.len(), 1, "{endpoint}");
        assert_eq!(
            window[0].axes(),
            [Axis::RuntimePolicy, Axis::IdentityAndRateSchema]
        );
        assert_eq!(window[0].after(), Some(&Value::from(100)));
    }
    assert_eq!(endpoint_ids(&report).len(), covered.len());
    // Nothing else moved: no coverage/sharing, cost, identity or wire change,
    // and only window fields changed at all.
    for axis in [Axis::SharingCoverage, Axis::Wire, Axis::CallerApi] {
        assert!(!report.has_axis(axis), "{axis:?}");
    }
    for change in report.changes() {
        assert!(
            change.field().contains("windows_canonical[0].capacity")
                || change.field().contains("windows_source_order[0].capacity")
                || change.field() == "definition.inline_windows[0].capacity",
            "unexpected change {}:{}:{}",
            change.collection(),
            change.record(),
            change.field()
        );
    }
}

#[test]
fn moved_endpoint_reports_derived_identity_change() {
    let report = diff(CatalogMutation::Baseline, CatalogMutation::MoveEndpoint);
    let record = "CatalogApi::regional::ping -> CatalogApi::ping";
    let id = field_changes(&report, record, "id.value");
    assert_eq!(id.len(), 1);
    assert_eq!(
        id[0].before(),
        Some(&Value::from("catalog_api.regional.ping"))
    );
    assert_eq!(id[0].after(), Some(&Value::from("catalog_api.ping")));
    assert_eq!(id[0].axes(), [Axis::IdentityAndRateSchema]);

    let rename = diff(CatalogMutation::Baseline, CatalogMutation::RenameScope);
    let pinned = "CatalogApi::admin::stats -> CatalogApi::operations::stats";
    assert!(
        field_changes(&rename, pinned, "id.value").is_empty(),
        "pinned IDs survive renames"
    );
    assert_eq!(
        field_changes(
            &rename,
            "CatalogApi::admin::health -> CatalogApi::operations::health",
            "id.value"
        )
        .len(),
        1,
        "navigation-derived IDs report the rename"
    );
    assert!(field_changes(&rename, pinned, "route.url_template").is_empty());
}

#[test]
fn same_named_ancestor_value_leaves_local_references_local() {
    let report = diff(
        CatalogMutation::Baseline,
        CatalogMutation::InsertSameNamedAncestorValue,
    );
    assert!(
        report
            .changes()
            .iter()
            .filter(|change| change.record() == "CatalogApi::regional::item")
            .all(|change| !change.field().starts_with("route"))
    );
    assert!(
        report
            .changes()
            .iter()
            .any(|change| change.record() == "CatalogApi" && change.field() == "values[id]")
    );
}

#[test]
fn redacted_literals_never_compare_equal() {
    use CatalogMutation::SensitiveLiterals;
    let same = diff(SensitiveLiterals("hidden-a"), SensitiveLiterals("hidden-a"));
    assert_eq!(same.verdict(), Verdict::StructurallyEqualWithUnresolved);
    assert_ne!(same.verdict(), Verdict::StructurallyEqual);
    let locations = same
        .unresolved()
        .iter()
        .map(|unresolved| unresolved.location())
        .collect::<Vec<_>>();
    assert!(locations.contains(&"CatalogApi#values.audit_tag.initializer"));
    assert!(
        same.unresolved()
            .iter()
            .all(|unresolved| unresolved.axis() == Axis::OpaqueDependency)
    );

    let different_hidden = diff(SensitiveLiterals("hidden-a"), SensitiveLiterals("hidden-b"));
    assert_eq!(
        different_hidden.verdict(),
        Verdict::StructurallyEqualWithUnresolved,
        "hidden literal edits are unresolved, never proven equal"
    );
    assert_eq!(
        export_string(&catalog(SensitiveLiterals("hidden-a"))),
        export_string(&catalog(SensitiveLiterals("hidden-b"))),
        "exports never carry hidden values"
    );
    let json = same.to_json_string();
    assert!(json.contains("structurally-equal-with-unresolved"));
    assert!(!json.contains("hidden-a"));
}

#[test]
fn unknown_structure_is_reported_never_ignored() {
    let baseline = export_value(&catalog(CatalogMutation::Baseline));
    let mut tampered = baseline.clone();
    tampered["future_section"] = Value::from("x");
    tampered["endpoints"][0]["future_field"] = Value::from(1);
    let report = diff_documents(&baseline, &tampered).expect("diff");
    let unknown = report.changes_on(Axis::Unknown);
    assert_eq!(unknown.len(), 2);
    assert_eq!(report.verdict(), Verdict::Different);
}

#[test]
fn invalid_snapshots_are_rejected_without_echoing_content() {
    let error = parse_document("{not json").expect_err("invalid json");
    assert_eq!(error.kind(), InspectErrorKind::InvalidJson);
    let error = parse_document(r#"{"format":"concord.contract/2"}"#).expect_err("format");
    assert_eq!(error.kind(), InspectErrorKind::UnsupportedFormat);
    let mut document = export_value(&catalog(CatalogMutation::Baseline));
    document.as_object_mut().expect("object").remove("policies");
    let error = parse_document(&document.to_string()).expect_err("missing key");
    assert_eq!(error.kind(), InspectErrorKind::MissingKey);
    let mut document = export_value(&catalog(CatalogMutation::Baseline));
    document["endpoints"][0]["timeout"]["duration"]["secs"] = Value::from(1.5);
    let error = parse_document(&document.to_string()).expect_err("float");
    assert_eq!(error.kind(), InspectErrorKind::NonIntegerNumber);
    assert!(!error.to_string().contains("1.5"));
}

#[test]
fn diff_reports_are_deterministic_in_text_and_json() {
    let first = diff(
        CatalogMutation::Baseline,
        CatalogMutation::MergeEqualDefinitions,
    );
    let second = diff(
        CatalogMutation::Baseline,
        CatalogMutation::MergeEqualDefinitions,
    );
    assert_eq!(first.to_json_string(), second.to_json_string());
    assert_eq!(first.to_text(), second.to_text());
    let json = first.to_json();
    assert_eq!(json["format"], "concord.contract-diff/3");
    assert_eq!(json["verdict"], "different");
    assert!(
        json["changed_endpoints"]
            .as_array()
            .is_some_and(|endpoints| !endpoints.is_empty())
    );
    for change in json["changes"].as_array().expect("changes") {
        assert!(change["record"].is_string());
        assert!(
            change["axes"]
                .as_array()
                .is_some_and(|axes| !axes.is_empty())
        );
    }
}

#[test]
fn explanation_expresses_the_catalog_worked_example() {
    let contract = catalog(CatalogMutation::Baseline);
    let list = contract.endpoint("catalog.items.list").expect("list");
    let text = explain_endpoint(&contract, list);
    for expected in [
        "CatalogApi::regional::item_summaries",
        "id: catalog.items.list (explicit)",
        "GET https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}/items",
        "tenant <- client.tenant, required; sensitive",
        "locale <- client.locale, omitted when absent",
        "category <- endpoint.category, omitted when absent",
        "offset <- endpoint.offset, initialized 0; page-controlled",
        "count <- endpoint.count, initialized DEFAULT_PAGE_SIZE",
        "tag <- each endpoint.tags, required; empty emits no values",
        "auth: bearer session; client application_key placement replaced",
        "endpoint-backed source; a challenge does not auto-login",
        "quota: catalog.tenant.read; key origin + label \"tenant\"",
        "120/1m AND 5000/1h; cost 1; coverage shown separately",
        "one-response output: Vec<ItemSummary> via Json",
        "[client default]",
        "traversal: OffsetPagination; Init(offset: offset, page_size: count), Cursor -> offset",
        "timeout: 20 seconds for the whole operation/traversal",
    ] {
        assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
    }
    assert!(text.contains("[scope regional]"));

    let policy = contract.policy("catalog.tenant.read").expect("policy");
    let text = explain_policy(&contract, policy);
    assert!(text.contains("catalog.items.list (CatalogApi::regional::item_summaries)"));
    assert!(text.contains(
        "share a bucket only with same limiter service, same policy ID, same normalized origin, same canonical \"tenant\" value (client.tenant)"
    ));
}
