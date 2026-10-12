#![forbid(unsafe_code)]
//! Real CLI and Rust expansion controls for macro-use origin uncertainty.
use quote::ToTokens;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct OwnedDir(PathBuf);
impl OwnedDir {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "concord-bare-origin-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for OwnedDir {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("preserved failed native control: {}", self.0.display());
            return;
        }
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn cargo(root: &Path, arguments: &[&str]) -> std::process::Output {
    let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args(arguments)
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", root.join("native-target"))
        .env("CARGO_BUILD_JOBS", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "native Cargo control: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn actual_cli_bare_macro_authority_respects_replacing_attributes() {
    let owned = OwnedDir::new();
    // The CLI controls are standalone sources; their siblings are native
    // consumer projects, never ancestor manifests changing their root proof.
    let native_root = owned.0.join("native-workspace");
    fs::create_dir(&native_root).unwrap();
    let root = &native_root;
    let facade = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/concord")
        .canonicalize()
        .unwrap();
    let facade_path = facade.to_string_lossy();
    let facade_path = facade_path
        .strip_prefix(r"\\?\")
        .unwrap_or(&facade_path)
        .replace('\\', "/");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers=[\"app\",\"sol_origin_attr\"]\nresolver=\"3\"\n",
    )
    .unwrap();
    // Both exact inputs refer to sol_origin_attr. Combine the unchanged
    // producer functions under that crate name without altering the inputs.
    let item_producer =
        syn::parse_file(include_str!("fixtures/trust_bare_item_producer.rs")).unwrap();
    let capture = item_producer
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Fn(function) if function.sig.ident == "capture_item" => {
                Some(function.to_token_stream().to_string())
            }
            _ => None,
        })
        .expect("exact capture_item producer");
    let source = format!(
        "{}\nuse proc_macro::Literal;\n{capture}\n",
        include_str!("fixtures/trust_bare_extern_producer.rs")
    );
    let crate_root = root.join("sol_origin_attr");
    fs::create_dir(&crate_root).unwrap();
    fs::write(crate_root.join("Cargo.toml"), "[package]\nname=\"sol_origin_attr\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[lib]\nproc-macro=true\npath=\"lib.rs\"\n").unwrap();
    fs::write(crate_root.join("lib.rs"), source).unwrap();
    fs::create_dir_all(root.join("app/src")).unwrap();
    fs::write(root.join("app/Cargo.toml"), format!("[package]\nname=\"bare-origin-app\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nconcord={{path=\"{facade_path}\",default-features=false}}\nsol_origin_attr={{path=\"../sol_origin_attr\"}}\n")).unwrap();
    fs::write(root.join("app/src/main.rs"), "fn main() {}\n").unwrap();
    // Only this isolated consumer lock is generated; workspace pins stay fixed.
    cargo(root, &["generate-lockfile", "--offline"]);
    let metadata = cargo(
        root,
        &["metadata", "--locked", "--offline", "--format-version", "1"],
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let resolved = packages
        .iter()
        .filter(|package| package["name"] == "concord")
        .collect::<Vec<_>>();
    assert_eq!(resolved.len(), 1);
    assert!(resolved[0]["source"].is_null());
    assert_eq!(
        Path::new(resolved[0]["manifest_path"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        facade.join("Cargo.toml")
    );
    for (label, source, foreign, child) in [
        (
            "extern",
            include_str!("fixtures/trust_bare_extern_attribute.rs"),
            true,
            None,
        ),
        (
            "item",
            include_str!("fixtures/trust_bare_item_attribute.rs"),
            true,
            None,
        ),
        (
            "genuine",
            include_str!("fixtures/trust_bare_genuine.rs"),
            false,
            None,
        ),
        (
            "unread-export",
            include_str!("fixtures/trust_bare_unread.rs"),
            true,
            Some(include_str!("fixtures/trust_bare_unread_export.rs")),
        ),
        (
            "unread-inner",
            include_str!("fixtures/trust_bare_unread.rs"),
            true,
            Some(include_str!("fixtures/trust_bare_unread_inner.rs")),
        ),
        (
            "unread-glob",
            include_str!("fixtures/trust_glob_unread.rs"),
            true,
            Some(include_str!("fixtures/trust_bare_unread_export.rs")),
        ),
    ] {
        let input_root = owned.0.join(format!("input-{label}"));
        fs::create_dir(&input_root).unwrap();
        let input = input_root.join("input.rs");
        assert!(!input_root.join("hidden.rs").exists());
        fs::write(&input, source).unwrap();
        fs::write(root.join("app/src/main.rs"), source).unwrap();
        if let Some(child) = child {
            fs::write(root.join("app/src/hidden.rs"), child).unwrap();
        }
        let before = cargo(
            root,
            &["run", "--locked", "--offline", "-p", "bare-origin-app"],
        );
        let output = owned.0.join(format!("converted-{label}"));
        let report_path = owned.0.join(format!("{label}.json"));
        let result = Command::new(env!("CARGO_BIN_EXE_concord-xtask"))
            .args(["migrate-v3", "--input"])
            .arg(&input)
            .args(["--apply", "--output"])
            .arg(&output)
            .arg("--report")
            .arg(&report_path)
            .output()
            .unwrap();
        assert_eq!(
            fs::read_to_string(&input).unwrap(),
            source,
            "protected original"
        );
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
        assert_eq!(
            result.status.code(),
            Some(i32::from(foreign)),
            "{label}: report={report}; stderr={}",
            String::from_utf8_lossy(&result.stderr)
        );
        let diagnostics = report["diagnostics"].as_array().unwrap();
        assert_eq!(
            diagnostics
                .iter()
                .any(|entry| entry["code"] == "C3-MIGRATE-API-MACRO-UNRESOLVED"),
            foreign,
            "actual diagnostics only"
        );
        assert_eq!(report["summary"]["automated_edits"], 0);
        assert_eq!(report["summary"]["declarations"], 0);
        let copy = output.join("input.rs");
        assert!(copy.exists(), "actual apply output exists for {label}");
        assert_eq!(
            fs::read_to_string(&copy).unwrap(),
            source,
            "whole foreign/genuine bytes"
        );
        fs::write(root.join("app/src/main.rs"), fs::read(copy).unwrap()).unwrap();
        let after = cargo(
            root,
            &["run", "--locked", "--offline", "-p", "bare-origin-app"],
        );
        assert_eq!(
            before.stdout, after.stdout,
            "native token/plan behavior for {label}"
        );
    }
}
