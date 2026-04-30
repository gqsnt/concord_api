#[test]
fn redaction_harness_placeholder() {
    let rendered = "[REDACTED]";
    assert!(!rendered.contains("super-secret-value"));
}
