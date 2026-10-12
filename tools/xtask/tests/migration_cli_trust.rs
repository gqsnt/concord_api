#![forbid(unsafe_code)]
//! Actual production CLI output must agree with the compiled V3 oracle.
use quote::ToTokens;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "fixtures/trust_zero_expected.rs"]
mod expected;

struct OwnedDir(PathBuf);
impl OwnedDir {
    fn new(label: &str) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "concord-cli-trust-{label}-{}-{nonce}",
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

fn has_actual_diagnostic(report: &str, code: &str) -> bool {
    let report: serde_json::Value = serde_json::from_str(report).expect("actual CLI report JSON");
    report
        .get("diagnostics")
        .and_then(serde_json::Value::as_array)
        .expect("actual report diagnostics array")
        .iter()
        .any(|diagnostic| diagnostic.get("code").and_then(serde_json::Value::as_str) == Some(code))
}

fn migrate(root: &Path, source: &str) -> (std::process::Output, PathBuf) {
    let input = root.join("inputs/input.rs");
    fs::write(&input, source).unwrap();
    let output = root.join("converted");
    let result = Command::new(env!("CARGO_BIN_EXE_concord-xtask"))
        .args(["migrate-v3", "--input"])
        .arg(&input)
        .args(["--apply", "--output"])
        .arg(&output)
        .arg("--report")
        .arg(root.join("report.json"))
        .output()
        .unwrap();
    assert_eq!(
        fs::read_to_string(&input).unwrap(),
        source,
        "input bytes must stay protected"
    );
    (result, output.join("input.rs"))
}

fn source_parts(source: &str) -> (String, Vec<String>, Vec<String>) {
    let file = syn::parse_file(source).expect("complete native Rust source");
    let attributes = file
        .attrs
        .iter()
        .map(|attr| attr.to_token_stream().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let mut imports = Vec::new();
    let mut items = Vec::new();
    for item in file.items {
        let tokens = item.to_token_stream().to_string();
        if matches!(item, syn::Item::Use(_)) {
            imports.push(tokens);
        } else {
            items.push(tokens);
        }
    }
    (attributes, imports, items)
}

#[test]
fn actual_cli_negative_zero_output_is_the_compiled_plan_oracle() {
    let owned = OwnedDir::new("zero");
    let source = include_str!("fixtures/trust_zero_v2.rs");
    let (result, path) = migrate(&owned.0, source);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut actual = source_parts(&fs::read_to_string(path).unwrap());
    let mut compiled = source_parts(include_str!("fixtures/trust_zero_expected.rs"));
    let original_imports = source_parts(source).1;
    assert_eq!(original_imports.len(), 1);
    assert_eq!(
        actual.1[..original_imports.len()],
        original_imports,
        "migration preserves the original import before its added Binary import"
    );
    // Rustfmt sorts the registered compiled fixture's two imports. Compare
    // their complete tokens as a set, retaining exact order for every other
    // item and preserving all attributes and literal values.
    assert_eq!(actual.1.len(), 2);
    actual.1.sort();
    compiled.1.sort();
    assert_eq!(
        actual, compiled,
        "literal-preserving whole output comparison"
    );
    // The expected module above is real api! code; its native generated plan
    // asserts /0 and the adjacent -1/2 control. The CLI text must match it.
}

fn compile_and_run(source: &Path, root: &Path, label: &str) -> std::process::Output {
    let binary = root.join(format!("{label}{}", std::env::consts::EXE_SUFFIX));
    let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compiled = Command::new(compiler)
        .arg("--edition=2024")
        .arg(source)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let result = Command::new(binary).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result
}

#[test]
fn actual_cli_never_rewrites_an_imported_application_macro() {
    let original = include_str!("fixtures/trust_foreign_api.rs");
    for (label, source) in [
        ("import", original.to_owned()),
        (
            "rename",
            original
                .replace("use concord::api;", "use concord::api as imported_api;")
                .replace("api! { client", "imported_api! { client"),
        ),
        (
            "glob",
            original.replace("use concord::api;", "use concord::*;"),
        ),
        (
            "raw",
            original
                .replace("mod concord {", "mod r#concord {")
                .replace("use concord::api;", "use r#concord::api;"),
        ),
    ] {
        let owned = OwnedDir::new(label);
        let input = owned.0.join("inputs/input.rs");
        fs::write(&input, &source).unwrap();
        let before = compile_and_run(&input, &owned.0, "before");
        let (result, output) = migrate(&owned.0, &source);
        assert_eq!(
            result.status.code(),
            Some(1),
            "foreign ownership must remain unresolved: {label}"
        );
        let report = fs::read_to_string(owned.0.join("report.json")).unwrap();
        assert!(
            has_actual_diagnostic(&report, "C3-MIGRATE-API-MACRO-UNRESOLVED"),
            "{report}"
        );
        // If apply writes an unchanged copy for an unresolved file, exercise
        // that actual copy too. Some modes may correctly refuse to write it.
        if output.exists() {
            assert_eq!(
                fs::read_to_string(&output).unwrap(),
                source,
                "{label}: no foreign edits"
            );
            let after = compile_and_run(&output, &owned.0, "after");
            assert_eq!(
                before.stdout, after.stdout,
                "application token contract must survive"
            );
        }
    }
}

#[test]
fn actual_cli_opaque_generated_namespace_never_authorizes_foreign_edits() {
    let original = include_str!("fixtures/trust_generated_namespace.rs");
    for (label, source) in [
        ("generated", original.to_owned()),
        ("generated-bom", format!("\u{feff}{original}")),
        (
            "generated-shebang",
            format!("#!/usr/bin/env rust-script\n{original}"),
        ),
    ] {
        let owned = OwnedDir::new(label);
        let input = owned.0.join("inputs/input.rs");
        fs::write(&input, &source).unwrap();
        let before = compile_and_run(&input, &owned.0, "before");
        let (result, copied) = migrate(&owned.0, &source);
        assert_eq!(
            result.status.code(),
            Some(1),
            "{label}: unknown expansion must stay unresolved"
        );
        let report = fs::read_to_string(owned.0.join("report.json")).unwrap();
        assert!(
            has_actual_diagnostic(&report, "C3-MIGRATE-API-MACRO-UNRESOLVED"),
            "{report}"
        );
        if copied.exists() {
            assert_eq!(fs::read_to_string(&copied).unwrap(), source);
            let after = compile_and_run(&copied, &owned.0, "after");
            assert_eq!(
                before.stdout, after.stdout,
                "{label}: actual application token behavior"
            );
        }
    }
}

#[test]
fn actual_cli_thread_local_forwarded_attribute_cannot_prove_macro_ownership() {
    let owned = OwnedDir::new("thread-local-attribute");
    let producer = owned.0.join("namespace_attribute.rs");
    fs::write(
        &producer,
        include_str!("fixtures/trust_namespace_attribute.rs"),
    )
    .unwrap();
    let library = owned.0.join(format!(
        "{}namespace_attribute{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let result = Command::new(&compiler)
        .args([
            "--edition=2024",
            "--crate-type=proc-macro",
            "--crate-name=namespace_attribute",
        ])
        .arg(&producer)
        .arg("-o")
        .arg(&library)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "attribute producer setup: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let source = include_str!("fixtures/trust_thread_local_attribute.rs");
    let input = owned.0.join("inputs/input.rs");
    fs::write(&input, source).unwrap();
    let compile_run = |input: &Path, label: &str| {
        let binary = owned
            .0
            .join(format!("{label}{}", std::env::consts::EXE_SUFFIX));
        let result = Command::new(&compiler)
            .arg("--edition=2024")
            .arg("--crate-name=foreign_thread_local")
            .arg(input)
            .arg("--extern")
            .arg(format!("namespace_attribute={}", library.display()))
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "foreign source positive control: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result = Command::new(binary).output().unwrap();
        assert!(
            result.status.success(),
            "foreign token behavior: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        result
    };
    let before = compile_run(&input, "before");
    let (result, copied) = migrate(&owned.0, source);
    assert_eq!(result.status.code(), Some(1));
    assert!(has_actual_diagnostic(
        &fs::read_to_string(owned.0.join("report.json")).unwrap(),
        "C3-MIGRATE-API-MACRO-UNRESOLVED"
    ));
    if copied.exists() {
        assert_eq!(fs::read_to_string(&copied).unwrap(), source);
        let after = compile_run(&copied, "after");
        assert_eq!(before.stdout, after.stdout);
    }
}

#[test]
fn actual_cli_reflexive_own_library_import_cannot_borrow_foreign_glob_api() {
    let owned = OwnedDir::new("own-self-glob");
    let package = owned.0.join("package");
    fs::create_dir_all(package.join("src")).unwrap();
    fs::write(
        package.join("Cargo.toml"),
        "[package]\nname=\"concord\"\nversion=\"3.0.0\"\nedition=\"2024\"\n",
    )
    .unwrap();
    let input = package.join("src/lib.rs");
    let source = include_str!("fixtures/trust_self_glob_api.rs");
    fs::write(&input, source).unwrap();
    let before = compile_and_run(&input, &owned.0, "before");
    let output = owned.0.join("converted");
    let result = Command::new(env!("CARGO_BIN_EXE_concord-xtask"))
        .args(["migrate-v3", "--input"])
        .arg(&input)
        .args(["--apply", "--output"])
        .arg(&output)
        .arg("--report")
        .arg(owned.0.join("report.json"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&input).unwrap(), source);
    assert!(has_actual_diagnostic(
        &fs::read_to_string(owned.0.join("report.json")).unwrap(),
        "C3-MIGRATE-API-MACRO-UNRESOLVED"
    ));
    let copied = output.join("lib.rs");
    if copied.exists() {
        assert_eq!(fs::read_to_string(&copied).unwrap(), source);
        let after = compile_and_run(&copied, &owned.0, "after");
        assert_eq!(before.stdout, after.stdout);
    }
}

#[test]
fn actual_cli_unknown_standard_item_macro_does_not_authorize_foreign_edits() {
    let owned = OwnedDir::new("standard-item-producer");
    let source = include_str!("fixtures/trust_std_cfg_select.rs");
    let input = owned.0.join("inputs/input.rs");
    fs::write(&input, source).unwrap();
    let before = compile_and_run(&input, &owned.0, "before");
    let (result, copied) = migrate(&owned.0, source);
    assert_eq!(result.status.code(), Some(1));
    assert!(has_actual_diagnostic(
        &fs::read_to_string(owned.0.join("report.json")).unwrap(),
        "C3-MIGRATE-API-MACRO-UNRESOLVED"
    ));
    if copied.exists() {
        assert_eq!(fs::read_to_string(&copied).unwrap(), source);
        let after = compile_and_run(&copied, &owned.0, "after");
        assert_eq!(before.stdout, after.stdout);
    }
}
