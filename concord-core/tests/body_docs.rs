//! Body documentation agrees with the public behavior pinned by C12.
#[test]
fn c12_12_promises_document_one_boundary_per_streaming_multipart_body() {
    let docs = include_str!("../../docs/guide/promises.md");
    let row = docs
        .lines()
        .find(|line| line.starts_with("| G6 |"))
        .unwrap();
    assert!(row.contains("Streaming form-data multipart chooses its random boundary once at `finish()` and reuses it for every attempt."));
    assert!(!row.contains("such a body is sent only once"));
}
#[test]
fn c12_11_extension_pages_name_only_tower_as_using_a_hidden_core_helper() {
    for docs in [
        include_str!("../../docs/internals/engineering.md"),
        include_str!("../../docs/reference/crates.md"),
        include_str!("../../docs/guide/extending.md"),
    ] {
        assert!(docs.contains(
            "Only the `tower` adapter uses a hidden helper of the core, for the response URL."
        ));
        assert!(!docs.contains("`multipart` a hidden method"));
        assert!(!docs.contains("two items also use a hidden part"));
        assert!(!docs.contains("Two items also use a hidden part"));
    }
}
#[test]
fn c12_12_mime_contracts_separate_construction_errors_from_streaming_failures() {
    let docs = include_str!("../../docs/reference/contracts.md");
    let parts = docs
        .lines()
        .find(|line| line.starts_with("| C12.3 |"))
        .unwrap();
    assert!(parts.contains("A one-shot part is refused without being claimed or polled."));
    assert!(parts.contains("Repeatable parts are checked for boundary collisions while read."));
    let writers = docs
        .lines()
        .find(|line| line.starts_with("| C12.5 |"))
        .unwrap();
    assert!(writers.contains("Construction failures are Build errors with no attempt."));
    assert!(writers.contains("Repeatable MIME parts are streamed; opening, reading or boundary failures are Transport errors (C12.13)."));
    assert!(!writers.contains("A successful body is buffered"));
}
#[test]
fn c12_14_generated_edit_docs_explain_detection_limits_and_actual_body_ownership() {
    let helper = include_str!("../src/__private/call.rs");
    for line in [
        "/// Reading it, removing it or a detected replacement is a Build error.",
        "/// Wrappers and look-alikes cannot always be detected; edits must leave the body alone.",
        "/// If building succeeds, each attempt receives the declared repeatable body as a fresh stream.",
    ] { assert!(helper.contains(line),"missing generated edit documentation: {line}"); }
    assert!(!helper.contains("/// replacing or removing it is a Build error."));
}
