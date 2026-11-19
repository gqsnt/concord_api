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
fn c12_11_extension_pages_distinguish_public_writers_from_hidden_helpers() {
    for docs in [
        include_str!("../../docs/internals/engineering.md"),
        include_str!("../../docs/reference/crates.md"),
        include_str!("../../docs/guide/extending.md"),
    ] {
        assert!(docs.contains(
            "The `tower` adapter and `transfer::at_call` also use hidden helpers of the core, for the response URL and the Build error respectively."
        ));
        assert!(docs.contains(
            "Wrapping writers use the public `Body::get_content_type`, `Body::map_stream` and `Body::is_repeatable` methods."
        ));
        assert!(!docs.contains("`multipart` a hidden method"));
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
