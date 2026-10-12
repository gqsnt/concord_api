#![forbid(unsafe_code)]
//! Real CLI and Rust expansion controls for macro-use origin uncertainty.
use quote::ToTokens;
use std::collections::BTreeSet;
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

fn registry_packages(lock: &Path) -> BTreeSet<[Option<String>; 4]> {
    let value: toml::Value = toml::from_str(&fs::read_to_string(lock).unwrap()).unwrap();
    value["package"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|package| {
            package
                .get("source")
                .and_then(toml::Value::as_str)
                .is_some_and(|source| source.starts_with("registry+"))
        })
        .map(|package| {
            ["name", "version", "source", "checksum"].map(|key| {
                package
                    .get(key)
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned)
            })
        })
        .collect()
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
    fs::write(root.join("app/Cargo.toml"), format!("[package]\nname=\"bare-origin-app\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nconcord={{path=\"{facade_path}\",default-features=false}}\nsol_origin_attr={{path=\"../sol_origin_attr\"}}\n[features]\nalt=[]\n")).unwrap();
    fs::write(root.join("app/src/main.rs"), "fn main() {}\n").unwrap();
    // Retain the fetched workspace versions when adding only these two local
    // consumer packages. generate-lockfile would discard those version pins
    // and can select a newer indexed crate absent from an offline CI cache.
    let workspace_lock = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    let workspace_lock_bytes = fs::read(&workspace_lock).unwrap();
    fs::write(root.join("Cargo.lock"), &workspace_lock_bytes).unwrap();
    cargo(root, &["metadata", "--offline", "--format-version", "1"]);
    assert!(
        registry_packages(&root.join("Cargo.lock")).is_subset(&registry_packages(&workspace_lock))
    );
    assert_eq!(fs::read(&workspace_lock).unwrap(), workspace_lock_bytes);
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
    let boolean_item = include_str!("fixtures/trust_bare_item_attribute.rs").replace(
        "#[sol_origin_attr::capture_item]",
        "#[cfg_attr(true, sol_origin_attr::capture_item)]",
    );
    assert_ne!(
        boolean_item,
        include_str!("fixtures/trust_bare_item_attribute.rs")
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
        ("boolean-item", boolean_item.as_str(), true, None),
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

    // Package-mode indexing must not treat the default child's source as
    // proof about an alternative selected by cfg_attr(path = ...).
    for (label, attribute, glob, foreign) in [
        (
            "gated-always",
            "#[cfg_attr(all(), path=\"alt.rs\")]",
            false,
            true,
        ),
        (
            "gated-feature",
            "#[cfg_attr(feature=\"alt\", path=\"alt.rs\")]",
            false,
            true,
        ),
        (
            "gated-glob",
            "#[cfg_attr(all(), path=\"alt.rs\")]",
            true,
            true,
        ),
        (
            "boolean-path",
            "#[cfg_attr(true, path=\"alt.rs\")]",
            false,
            true,
        ),
        (
            "boolean-path-glob",
            "#[cfg_attr(true, path=\"alt.rs\")]",
            true,
            true,
        ),
        (
            "boolean-false",
            "#[cfg_attr(false, path=\"alt.rs\")]",
            false,
            false,
        ),
        ("direct-path", "#[path=\"alt.rs\"]", false, true),
        ("complete-inert", "", false, false),
    ] {
        let input_root = owned.0.join(format!("package-{label}"));
        fs::create_dir_all(input_root.join("src")).unwrap();
        fs::write(input_root.join("Cargo.toml"), "[package]\nname=\"package-origin\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nconcord=\"3\"\n[features]\nalt=[]\n").unwrap();
        let mut source = if foreign {
            include_str!("fixtures/trust_bare_unread.rs").to_owned()
        } else {
            include_str!("fixtures/trust_bare_genuine.rs").replace(
                "extern crate concord;",
                "extern crate concord;\nmod hidden;",
            )
        };
        source = source.replace("mod hidden;", &format!("{attribute}\nmod hidden;"));
        if glob {
            source = source.replace(
                "#[macro_use]\nextern crate concord;",
                "extern crate concord;\nuse concord::*;",
            );
        }
        let child = "pub fn inert() {}\n";
        let alternative = include_str!("fixtures/trust_bare_unread_export.rs");
        for (file, bytes) in [
            ("main.rs", source.as_str()),
            ("hidden.rs", child),
            ("alt.rs", alternative),
        ] {
            fs::write(input_root.join("src").join(file), bytes).unwrap();
            fs::write(root.join("app/src").join(file), bytes).unwrap();
        }
        let before = cargo(
            root,
            &[
                "run",
                "--locked",
                "--offline",
                "-p",
                "bare-origin-app",
                "--features",
                "alt",
            ],
        );
        let output = owned.0.join(format!("converted-package-{label}"));
        let report_path = owned.0.join(format!("package-{label}.json"));
        let result = Command::new(env!("CARGO_BIN_EXE_concord-xtask"))
            .args(["migrate-v3", "--input"])
            .arg(&input_root)
            .args(["--apply", "--output"])
            .arg(&output)
            .arg("--report")
            .arg(&report_path)
            .output()
            .unwrap();
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
        assert_eq!(
            result.status.code(),
            Some(i32::from(foreign)),
            "{label}: {report}"
        );
        assert_eq!(report["summary"]["automated_edits"], 0);
        assert_eq!(report["summary"]["declarations"], 0);
        assert_eq!(
            report["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["code"] == "C3-MIGRATE-API-MACRO-UNRESOLVED"),
            foreign
        );
        for (file, bytes) in [
            ("main.rs", source.as_str()),
            ("hidden.rs", child),
            ("alt.rs", alternative),
        ] {
            assert_eq!(
                fs::read_to_string(input_root.join("src").join(file)).unwrap(),
                bytes,
                "protected {label}/{file}"
            );
            assert_eq!(
                fs::read_to_string(output.join("src").join(file)).unwrap(),
                bytes,
                "whole output {label}/{file}"
            );
            fs::write(
                root.join("app/src").join(file),
                fs::read(output.join("src").join(file)).unwrap(),
            )
            .unwrap();
        }
        let after = cargo(
            root,
            &[
                "run",
                "--locked",
                "--offline",
                "-p",
                "bare-origin-app",
                "--features",
                "alt",
            ],
        );
        assert_eq!(
            before.stdout, after.stdout,
            "package-mode native behavior for {label}"
        );
    }

    // A canonical-looking import in the default child cannot prove a
    // library's own export when cfg_attr selects an attributed alternative.
    // This consumer is an actual library package named concord, invoked by
    // a distinct runner; the foreign attribute producer remains exact.
    let own_root = owned.0.join("own-native-workspace");
    fs::create_dir_all(own_root.join("owner/src")).unwrap();
    fs::create_dir_all(own_root.join("runner/src")).unwrap();
    let macros = facade.parent().unwrap().join("concord-macros");
    let portable = |path: &Path| {
        let path = path.to_string_lossy();
        path.strip_prefix(r"\\?\")
            .unwrap_or(&path)
            .replace('\\', "/")
    };
    fs::write(
        own_root.join("Cargo.toml"),
        "[workspace]\nmembers=[\"owner\",\"runner\"]\nresolver=\"3\"\n",
    )
    .unwrap();
    fs::write(own_root.join("owner/Cargo.toml"), format!("[package]\nname=\"concord\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[lib]\npath=\"src/lib.rs\"\n[dependencies]\nconcord_macros={{package=\"concord-macros\",path=\"{}\"}}\nsol_origin_attr={{path=\"{}\"}}\n[features]\nalt=[]\n", portable(&macros), portable(&root.join("sol_origin_attr")))).unwrap();
    fs::write(own_root.join("owner/src/lib.rs"), "pub fn verify() {}\n").unwrap();
    fs::write(own_root.join("runner/Cargo.toml"), "[package]\nname=\"own-origin-runner\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nown_library={package=\"concord\",path=\"../owner\"}\n[features]\nalt=[\"own_library/alt\"]\n").unwrap();
    fs::write(
        own_root.join("runner/src/main.rs"),
        "fn main() { own_library::verify(); }\n",
    )
    .unwrap();
    fs::write(own_root.join("Cargo.lock"), &workspace_lock_bytes).unwrap();
    let own_metadata = cargo(
        &own_root,
        &["metadata", "--offline", "--format-version", "1"],
    );
    assert!(
        registry_packages(&own_root.join("Cargo.lock"))
            .is_subset(&registry_packages(&workspace_lock))
    );
    let own_metadata: serde_json::Value = serde_json::from_slice(&own_metadata.stdout).unwrap();
    let own_packages = own_metadata["packages"].as_array().unwrap();
    let own_package = own_packages
        .iter()
        .filter(|package| package["name"] == "concord")
        .collect::<Vec<_>>();
    assert_eq!(own_package.len(), 1);
    assert!(own_package[0]["source"].is_null());
    assert_eq!(
        Path::new(own_package[0]["manifest_path"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        own_root.join("owner/Cargo.toml").canonicalize().unwrap()
    );
    let foreign = include_str!("fixtures/trust_bare_unread.rs");
    let declaration = foreign[foreign.find("api! {").unwrap()..]
        .replacen("api! {", "concord::api! {", 1)
        .replace("fn main()", "pub fn verify()");
    let default_child = "pub(crate) use concord_macros::api;\n";
    let alternate_child =
        "#[sol_origin_attr::replace_macro_use]\nstruct Placeholder;\npub(crate) use api;\n";
    for (label, attribute) in [
        ("own-always", r#"#[cfg_attr(all(), path="alt.rs")]"#),
        ("own-boolean", r#"#[cfg_attr(true, path="alt.rs")]"#),
        (
            "own-feature",
            r#"#[cfg_attr(feature="alt", path="alt.rs")]"#,
        ),
        ("own-direct", r#"#[path="alt.rs"]"#),
    ] {
        let input = owned.0.join(label);
        fs::create_dir_all(input.join("src")).unwrap();
        fs::write(input.join("Cargo.toml"), "[package]\nname=\"concord\"\nversion=\"3.0.0\"\nedition=\"2024\"\n[lib]\npath=\"src/lib.rs\"\n[dependencies]\nconcord_macros={package=\"concord-macros\",version=\"3\"}\nsol_origin_attr={path=\"../producer\"}\n[features]\nalt=[]\n").unwrap();
        let source = format!(
            "extern crate self as concord;\n{attribute}\nmod hidden;\npub(crate) use hidden::api;\n{declaration}"
        );
        for (file, bytes) in [
            ("lib.rs", source.as_str()),
            ("hidden.rs", default_child),
            ("alt.rs", alternate_child),
        ] {
            fs::write(input.join("src").join(file), bytes).unwrap();
            fs::write(own_root.join("owner/src").join(file), bytes).unwrap();
        }
        let before = cargo(
            &own_root,
            &[
                "run",
                "--locked",
                "--offline",
                "-p",
                "own-origin-runner",
                "--features",
                "alt",
            ],
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
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
        assert_eq!(result.status.code(), Some(1), "{label}: {report}");
        assert_eq!(report["summary"]["automated_edits"], 0);
        assert_eq!(report["summary"]["declarations"], 0);
        assert!(
            report["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["code"] == "C3-MIGRATE-API-MACRO-UNRESOLVED")
        );
        for (file, bytes) in [
            ("lib.rs", source.as_str()),
            ("hidden.rs", default_child),
            ("alt.rs", alternate_child),
        ] {
            assert_eq!(
                fs::read_to_string(input.join("src").join(file)).unwrap(),
                bytes,
                "protected {label}/{file}"
            );
            assert_eq!(
                fs::read_to_string(output.join("src").join(file)).unwrap(),
                bytes,
                "whole output {label}/{file}"
            );
            fs::write(
                own_root.join("owner/src").join(file),
                fs::read(output.join("src").join(file)).unwrap(),
            )
            .unwrap();
        }
        let after = cargo(
            &own_root,
            &[
                "run",
                "--locked",
                "--offline",
                "-p",
                "own-origin-runner",
                "--features",
                "alt",
            ],
        );
        assert_eq!(
            before.stdout, after.stdout,
            "actual own-library native behavior {label}"
        );
    }
    assert_eq!(fs::read(&workspace_lock).unwrap(), workspace_lock_bytes);
}
