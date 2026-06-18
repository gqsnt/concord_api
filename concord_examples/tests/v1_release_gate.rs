fn workspace_file(path: &str) -> String {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples crate has workspace parent")
        .to_path_buf();

    std::fs::read_to_string(workspace.join(path))
        .unwrap_or_else(|err| panic!("read workspace file {path}: {err}"))
}

#[test]
fn v1_release_gate_script_contains_required_commands() {
    let script = workspace_file("scripts/check_v1.sh");

    assert!(script.starts_with("#!/usr/bin/env bash"));
    assert!(script.contains("set -euo pipefail"));

    for command in [
        "cargo fmt --check",
        "cargo test -p concord_core redaction",
        "cargo test -p concord_core auth_runtime",
        "cargo test -p concord_examples live_smoke",
        "cargo test -p concord_core",
        "cargo test -p concord_macros",
        "cargo test -p concord_examples",
        "cargo test --workspace",
        "cargo doc --workspace --no-deps",
        "cargo clippy --workspace --all-targets -- -D warnings",
    ] {
        assert!(
            script.contains(command),
            "scripts/check_v1.sh should contain `{command}`"
        );
    }

    for forbidden in ["cargo publish", "cargo package"] {
        assert!(
            !script.contains(forbidden),
            "scripts/check_v1.sh must not contain `{forbidden}`"
        );
    }
}

#[test]
fn v1_release_checklist_links_local_gate_and_manual_audit() {
    let checklist = workspace_file("dev_doc/release_checklist.md");

    for snippet in [
        "./scripts/check_v1.sh",
        "Manual v1 audit",
        "Public DSL docs are complete",
        "Cache sizing syntax",
        "Same-site duplicate behavior",
        "Live smoke examples are environment-gated",
        "does not require external credentials",
        "Query auth redaction tests pass",
        "No auth secret appears in debug output tests",
        "`401`/`403` auth rejection behavior",
        "No crates.io publishing",
    ] {
        assert!(
            checklist.contains(snippet),
            "release checklist should contain `{snippet}`"
        );
    }
}
