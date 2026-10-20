use concord_inspect::{InspectErrorKind, Verdict, diff_documents, export_value, validate_document};
use serde_json::json;
concord::api! {
    client CompositeIdentity {
        base "https://fixture.test/";
        GET read() -> () via concord::Empty;
    }
}
#[test]
fn distinct_supplied_components_are_not_duplicate_keys_and_both_records_survive_diff() {
    let base = export_value(CompositeIdentity::contract());
    let left = json!({"kind":"a:b","reference":"c"});
    let right = json!({"kind":"a","reference":"b:c"});
    for record in [&left, &right] {
        let mut single = base.clone();
        single["dependencies"] = json!([record]);
        validate_document(&single).unwrap();
        assert_eq!(
            diff_documents(&single, &single).unwrap().verdict(),
            Verdict::StructurallyEqual
        );
    }
    let mut pair = base;
    pair["dependencies"] = json!([left, right]);
    validate_document(&pair).expect("two distinct supplied composite identities");
    assert_eq!(
        diff_documents(&pair, &pair).unwrap().verdict(),
        Verdict::StructurallyEqual
    );
    let mut reversed = pair.clone();
    reversed["dependencies"].as_array_mut().unwrap().reverse();
    assert_eq!(
        diff_documents(&pair, &reversed).unwrap().verdict(),
        Verdict::StructurallyEqual
    );
    for index in 0..2 {
        let mut changed = pair.clone();
        changed["dependencies"][index]["unknown"] = json!(index);
        let report = diff_documents(&pair, &changed).unwrap();
        assert_eq!(report.verdict(), Verdict::Different);
        let edits = report
            .changes()
            .iter()
            .filter(|c| c.collection() == "dependencies")
            .collect::<Vec<_>>();
        assert_eq!(edits.len(), 1);
        let expected = concord_inspect::to_canonical_string(&json!([
            pair["dependencies"][index]["kind"],
            pair["dependencies"][index]["reference"]
        ]));
        assert_eq!(edits[0].record(), expected);
    }
}
#[test]
fn genuine_composite_duplicates_and_malformed_keys_remain_rejected() {
    let mut base = export_value(CompositeIdentity::contract());
    base["dependencies"] = json!([{"kind":"a","reference":"b"},{"kind":"a","reference":"b"}]);
    assert_eq!(
        validate_document(&base).unwrap_err().kind(),
        InspectErrorKind::DuplicateKey
    );
    base["dependencies"] = json!([{"kind":42,"reference":"b"}]);
    assert_eq!(
        validate_document(&base).unwrap_err().kind(),
        InspectErrorKind::InvalidRecordKey
    );
}
#[test]
fn established_positional_fallback_collision_denial_is_unchanged() {
    let mut base = export_value(CompositeIdentity::contract());
    base["owners"] = json!([{}, {"path":"#0"}]);
    assert_eq!(
        validate_document(&base).unwrap_err().kind(),
        InspectErrorKind::DuplicateKey
    );
    base["owners"] = json!([{}]);
    assert_eq!(
        diff_documents(&base, &base).unwrap().verdict(),
        Verdict::StructurallyEqual
    );
}
