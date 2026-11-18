//! C8.6: an attribute macro inside a `paginate` block can write a `return` of a closure that the
//! macro cannot see; the expression keeps the inferred path, so the closure's parameters are
//! inferred, and it runs. The attribute is a proc-macro crate of its own, so this test builds a
//! small project beside it (`fixtures/attribute-macro`), offline, in `target/`.
#![allow(clippy::unwrap_used)]

use std::{fs, path::PathBuf, process::Command};

#[test]
fn c8_6_an_attribute_that_writes_a_return_keeps_the_inferred_path() {
    let package = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = package.join("../..").canonicalize().unwrap();
    let fixture = package.join("fixtures/attribute-macro");
    let work = repo.join("target/attribute-macro");
    fs::create_dir_all(work.join("src")).unwrap();
    fs::create_dir_all(work.join("probe/src")).unwrap();
    fs::copy(fixture.join("main.rs"), work.join("src/main.rs")).unwrap();
    fs::copy(fixture.join("probe.rs"), work.join("probe/src/lib.rs")).unwrap();
    fs::copy(repo.join("Cargo.lock"), work.join("Cargo.lock")).unwrap();
    fs::write(
        work.join("probe/Cargo.toml"),
        "[package]\nname = \"concord-attr-probe\"\nversion = \"0.0.0\"\nedition = \"2021\"\npublish = false\n\n[lib]\nproc-macro = true\n",
    )
    .unwrap();
    fs::write(
        work.join("Cargo.toml"),
        format!(
            r#"[package]
name = "attribute-macro"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
concord = {{ package = "concord-api", path = {:?}, default-features = false, features = ["json"] }}
concord-attr-probe = {{ path = "probe" }}
serde = {{ version = "1", features = ["derive"] }}
tokio = {{ version = "1", features = ["macros", "rt"] }}

[workspace]
"#,
            repo.join("concord-api")
        ),
    )
    .unwrap();
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .current_dir(&work)
        .args(["run", "--offline", "--quiet"])
        .env("CARGO_TARGET_DIR", work.join("target"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // The closure asks for the same call again: the first page, then the repeat Mismatch.
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "page [1], Mismatch"
    );
}
