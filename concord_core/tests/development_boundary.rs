use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn __development_requires_the_explicit_feature_even_in_debug_builds() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let nonce = format!("{}-{:?}", std::process::id(), std::thread::current().id());
    let work = std::env::temp_dir().join(format!("concord-development-boundary-{nonce}"));
    let target = std::env::temp_dir().join(format!("concord-development-target-{nonce}"));
    std::fs::create_dir_all(work.join("src")).expect("create development fixture directory");
    std::fs::copy(
        root.join("tests/fixtures/development_boundary.rs"),
        work.join("src/main.rs"),
    )
    .expect("copy development fixture source");

    write_manifest(&work, &root, false);
    let unavailable = cargo_check(&work, &target);
    assert!(!unavailable.status.success());
    let diagnostic = String::from_utf8_lossy(&unavailable.stderr);
    assert!(diagnostic.contains("__development"), "{diagnostic}");
    assert!(
        diagnostic.contains("dangerous-dev-tools") || diagnostic.contains("could not find"),
        "{diagnostic}"
    );

    write_manifest(&work, &root, true);
    let available = cargo_check(&work, &target);
    assert!(
        available.status.success(),
        "feature-enabled fixture failed:\n{}",
        String::from_utf8_lossy(&available.stderr)
    );

    let private_source =
        std::fs::read_to_string(root.join("tests/fixtures/development_generation_private.rs"))
            .expect("read opaque-generation negative fixture source");
    std::fs::write(work.join("src/main.rs"), private_source)
        .expect("write opaque-generation negative fixture source");
    let opaque = cargo_check(&work, &target);
    assert!(!opaque.status.success());
    let diagnostic = String::from_utf8_lossy(&opaque.stderr);
    assert!(diagnostic.contains("private"), "{diagnostic}");
    assert!(
        diagnostic.contains("CredentialGenerationSnapshot"),
        "{diagnostic}"
    );

    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all(&target);
}

#[cfg(feature = "dangerous-dev-tools")]
#[test]
fn __development_types_compile_in_the_feature_enabled_test_crate() {
    fn uses_type<T>() {}

    uses_type::<concord_core::__development::CredentialGenerationSnapshot>();
    uses_type::<concord_core::__development::CredentialLifecycleEvent>();
    uses_type::<concord_core::__development::CapturedNativeRequest>();
    uses_type::<concord_core::__development::DeterministicNativeExecutor>();
    uses_type::<concord_core::__development::ScriptedNativeResponse>();
    uses_type::<concord_core::__development::UnsafeCredentialPlacementExpectations>();
}

fn write_manifest(work: &Path, core: &Path, enabled: bool) {
    let feature = if enabled {
        ", features = [\"dangerous-dev-tools\"]"
    } else {
        ""
    };
    let core = core.display().to_string().replace('\\', "\\\\");
    let manifest = format!(
        "[package]\nname = \"concord_development_boundary\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n\n[dependencies]\nconcord_core = {{ path = \"{core}\", default-features = false{feature} }}\n"
    );
    std::fs::write(work.join("Cargo.toml"), manifest).expect("write development fixture manifest");
}

fn cargo_check(work: &Path, target: &Path) -> std::process::Output {
    Command::new(env!("CARGO"))
        .arg("check")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(work.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", target)
        .output()
        .expect("run cargo check for development boundary fixture")
}
