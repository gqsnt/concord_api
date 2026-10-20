use concord_inspect::{
    InspectErrorKind, Verdict, diff_documents, export_value, parse_document, to_canonical_string,
    validate_document,
};
use serde_json::{Value, json};
concord::api! { client UnusedRegistry { base "https://fixture.test/"; rate_limits { windows unused { allow 1 per 1s; } policy unused() { id "s4.unused"; key [origin]; using unused; } } GET read() -> () via concord::Empty; } }
concord::api! { client RedactedFixture { base "https://fixture.test/"; values { #[sensitive] query private: String = "private-sentinel"; } GET read() -> () via concord::Empty; } }
#[test]
fn generated_same_name_registries_roundtrip_without_losing_diagnostics() {
    let document = export_value(UnusedRegistry::contract());
    assert_eq!(document["diagnostics"].as_array().unwrap().len(), 2);
    validate_document(&document).unwrap();
    assert_eq!(
        parse_document(&to_canonical_string(&document)).unwrap(),
        document
    );
    assert_eq!(
        diff_documents(&document, &document).unwrap().verdict(),
        Verdict::StructurallyEqual
    );
}
#[test]
fn represented_diagnostic_identity_is_complete_and_collision_free() {
    let base = export_value(UnusedRegistry::contract());
    let record = json!({"code":"C3-RATE","severity":"note","owner":"UnusedRegistry","subject":"unused","message":"first","correction":null});
    for (field, value) in [
        ("message", json!("second")),
        ("severity", json!("warning")),
        ("correction", json!("attach it")),
        ("future", json!("unknown")),
    ] {
        let mut document = base.clone();
        let mut second = record.clone();
        second[field] = value;
        document["diagnostics"] = json!([record, second]);
        validate_document(&document).unwrap();
        let mut one = document.clone();
        one["diagnostics"].as_array_mut().unwrap().pop();
        assert_eq!(
            diff_documents(&one, &document).unwrap().verdict(),
            Verdict::Different,
            "{field}"
        );
    }
    let mut identical = base.clone();
    identical["diagnostics"] = json!([record, record]);
    assert_eq!(
        validate_document(&identical).unwrap_err().kind(),
        InspectErrorKind::DuplicateKey
    );
    let mut composite = base.clone();
    composite["diagnostics"] =
        json!([{"code":"a:b","owner":"c","subject":"d"},{"code":"a","owner":"b:c","subject":"d"}]);
    validate_document(&composite).unwrap();
    let mut malformed = base;
    malformed["diagnostics"] = json!([{"code":42}]);
    assert_eq!(
        validate_document(&malformed).unwrap_err().kind(),
        InspectErrorKind::InvalidRecordKey
    );
}
#[test]
fn redaction_evidence_survives_an_empty_or_inconsistent_dependency_index() {
    let base = export_value(RedactedFixture::contract());
    assert_eq!(
        diff_documents(&base, &base).unwrap().verdict(),
        Verdict::StructurallyEqualWithUnresolved
    );
    let mut stripped = base.clone();
    stripped["dependencies"] = json!([]);
    validate_document(&stripped).unwrap();
    let report = diff_documents(&stripped, &stripped).unwrap();
    assert_eq!(report.verdict(), Verdict::StructurallyEqualWithUnresolved);
    assert!(
        report
            .unresolved()
            .iter()
            .any(|u| u.location() == "RedactedFixture#values.private.initializer")
    );
    assert_eq!(
        diff_documents(&base, &stripped).unwrap().verdict(),
        Verdict::Different
    );
    let mut absent = stripped.clone();
    absent.as_object_mut().unwrap().remove("dependencies");
    assert_eq!(
        validate_document(&absent).unwrap_err().kind(),
        InspectErrorKind::MissingKey
    );
    // A redacted kind also supplies evidence without its redundant boolean.
    stripped["future"] = json!({"kind":"redacted"});
    assert!(
        diff_documents(&stripped, &stripped)
            .unwrap()
            .unresolved()
            .iter()
            .any(|u| u.location() == "json:/future")
    );
    // Future marker with missing/malformed authored location remains opaque.
    stripped["future"] = json!({"redacted":true,"location":42});
    let report = diff_documents(&stripped, &stripped).unwrap();
    assert!(
        report
            .unresolved()
            .iter()
            .any(|u| u.location() == "json:/future")
    );
    assert!(!report.to_json_string().contains("private-sentinel"));
    let mut one_sided = stripped.clone();
    one_sided["future"] = Value::Null;
    assert_eq!(
        diff_documents(&stripped, &one_sided).unwrap().verdict(),
        Verdict::Different
    );
}
