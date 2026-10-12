#![forbid(unsafe_code)]
//! Incomplete source authority must not authorize production CLI rewrites.
use quote::ToTokens;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const API: &str = r#"use concord::api;
use concord::Empty;
api! { client MissingModuleApi {
    base "https://missing-module.example.test/";
    GET fetch() -> () via Empty { path ["v2"]; }
} }
"#;
const UNRESOLVED: &str = "C3-MIGRATE-API-MACRO-UNRESOLVED";

struct OwnedDir(PathBuf);
impl OwnedDir {
    fn new(label: &str) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "concord-cli-missing-module-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        fs::create_dir(path.join("inputs")).unwrap();
        Self(path)
    }
}
impl Drop for OwnedDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_cli(label: &str, source: &str) -> (std::process::Output, String, serde_json::Value) {
    let owned = OwnedDir::new(label);
    let input = owned.0.join("inputs/input.rs");
    let output = owned.0.join("converted");
    let report = owned.0.join("report.json");
    fs::write(&input, source).unwrap();
    assert!(!owned.0.join("inputs/hidden.rs").exists());
    assert!(!owned.0.join("inputs/hidden/mod.rs").exists());
    // This is the package's real binary, selected by Cargo's integration-test
    // contract. No copied parser/checker and no --lib test target is involved.
    let result = Command::new(env!("CARGO_BIN_EXE_concord-xtask"))
        .args(["migrate-v3", "--input"])
        .arg(&input)
        .args(["--apply", "--output"])
        .arg(&output)
        .arg("--report")
        .arg(&report)
        .output()
        .unwrap();
    assert_eq!(
        fs::read_to_string(&input).unwrap(),
        source,
        "protected input"
    );
    let converted = fs::read_to_string(output.join("input.rs"))
        .expect("production apply must preserve an unchanged unresolved output copy");
    let diagnostics: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(report).expect("actual CLI report"))
            .expect("actual CLI report JSON");
    assert!(diagnostics.get("diagnostics").is_some_and(|d| d.is_array()));
    (result, converted, diagnostics)
}

fn has_unresolved(report: &serde_json::Value) -> bool {
    report["diagnostics"]
        .as_array()
        .expect("actual emitted diagnostics, not the report's global code catalog")
        .iter()
        .any(|d| d.get("code").and_then(serde_json::Value::as_str) == Some(UNRESOLVED))
}

fn require_missing_module_refusal(label: &str, prefix: &str) {
    let source = format!("{prefix}{API}");
    let (result, converted, report) = run_cli(label, &source);
    assert_eq!(
        result.status.code(),
        Some(1),
        "{label}: source authority must remain unresolved, stdout={} stderr={} report={report}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        converted, source,
        "{label}: entire output remains byte-identical"
    );
    assert!(has_unresolved(&report), "{label}: {report}");
}

#[test]
fn s5_namespace_missing_module_unrelated_sibling_preserves_explicit_api_origin() {
    let source = format!("mod hidden;\n{API}");
    let (result, converted, report) = run_cli("unrelated-missing-sibling", &source);
    assert!(
        result.status.success(),
        "unrelated sibling must not erase explicit API ownership: stdout={} stderr={} report={report}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!has_unresolved(&report), "{report}");
    // Preserve the unrelated module, complete imports/items/attributes and
    // every literal; require only the actual V2 path-to-V3 translation.
    let expected = source.replace("path [\"v2\"];", "path \"v2\";");
    assert_ne!(expected, source, "the control must require an actual edit");
    let actual = syn::parse_file(&converted).expect("complete converted Rust token tree");
    let expected = syn::parse_file(&expected).expect("complete expected Rust token tree");
    assert_eq!(
        actual.to_token_stream().to_string(),
        expected.to_token_stream().to_string(),
        "whole-file literal-preserving comparison"
    );
}

#[test]
fn s5_namespace_missing_module_glob_is_unresolved() {
    require_missing_module_refusal("missing-glob", "mod hidden;\nuse hidden::*;\n");
}

#[test]
fn s5_namespace_missing_module_qualified_attribute_is_unresolved() {
    require_missing_module_refusal(
        "missing-qualified-attribute",
        "mod hidden;\nuse hidden::tokio;\n#[tokio::test]\nfn f() {}\n",
    );
}

#[test]
fn s5_namespace_missing_module_complete_standalone_control_migrates() {
    let (result, converted, report) = run_cli("complete-standalone-control", API);
    assert!(
        result.status.success(),
        "positive control: stdout={} stderr={} report={report}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert_ne!(converted, API, "real V2-to-V3 edit is required");
    assert!(converted.contains("path \"v2\";"), "{converted}");
    assert!(!converted.contains("path [\"v2\"];"), "{converted}");
    assert!(!has_unresolved(&report), "{report}");
}
