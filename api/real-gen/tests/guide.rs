//! Refused roots stay verbatim and are checked by the real compiler too.
use std::{fs, path::PathBuf, process::Command};
#[test]
fn c1_20_fable_6_every_refused_paper_root_is_compiled_and_its_reason_is_held() {
    // Use this binary's Cargo fingerprints, not timestamps: a standalone test
    // can rebuild a different feature set after the workspace artifacts.
    let executable = std::env::current_exe().unwrap();
    let deps = executable.parent().unwrap();
    let fingerprints = deps.parent().unwrap().join(".fingerprint");
    let executable_name = executable.file_stem().unwrap().to_str().unwrap();
    let hash = executable_name.rsplit_once('-').unwrap().1;
    let own_fingerprint =
        fingerprints.join(format!("real-gen-{hash}/test-integration-test-guide.json"));
    let metadata: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(own_fingerprint).unwrap()).unwrap();
    let library = |name: &str| -> PathBuf {
        let wanted = metadata["deps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|dep| dep[1] == name)
            .unwrap()[3]
            .as_u64()
            .unwrap();
        fs::read_dir(&fingerprints)
            .unwrap()
            .find_map(|entry| {
                let directory = entry.unwrap().path();
                let text = fs::read_to_string(directory.join(format!("lib-{name}"))).ok()?;
                // Check/Clippy can leave invalidated fingerprint files empty.
                if text.len() != 16 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return None;
                }
                let bytes = std::array::from_fn(|i| {
                    u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap()
                });
                if u64::from_le_bytes(bytes) != wanted {
                    return None;
                }
                let hash = directory
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .rsplit_once('-')
                    .unwrap()
                    .1;
                let artifact = deps.join(format!("lib{name}-{hash}.rlib"));
                artifact.is_file().then_some(artifact)
            })
            .unwrap_or_else(|| panic!("no library for this test's {name} fingerprint {wanted}"))
    };
    let package = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let api = package.parent().unwrap();
    let out = api.join("target/p45-paper-rustc");
    fs::create_dir_all(&out).unwrap();
    let mut code =
        "#![allow(dead_code,unused_imports,non_snake_case,non_upper_case_globals)]\n".to_owned();
    code += &format!(
        "mod telegram {{ include!({:?}); include!({:?}); include!({:?}); }}\n",
        package.join("src/scaffold.rs"),
        package.join("src/telegram/types.rs"),
        package.join("cases/fail/telegram.rs")
    );
    let file = out.join("telegram.rs");
    fs::write(&file, code).unwrap();
    let mut command = Command::new("rustc");
    command
        .args([
            "--edition=2021",
            "--crate-type=lib",
            "--emit=metadata",
            "--crate-name=paper_refusal",
        ])
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("--out-dir")
        .arg(&out);
    for name in ["real_gen", "concord", "serde", "serde_json", "bytes"] {
        command
            .arg("--extern")
            .arg(format!("{name}={}", library(name).display()));
    }
    let result = command.arg(&file).output().unwrap();
    let text = String::from_utf8_lossy(&result.stderr);
    fs::write(out.join("telegram.log"), text.as_bytes()).unwrap();
    assert!(!result.status.success(), "Telegram unexpectedly compiled");
    assert!(
        text.contains("Secret") && text.contains("Serialize"),
        "{text}"
    );
    let errors = text
        .lines()
        .filter(|line| line.starts_with("error") && !line.starts_with("error: aborting"))
        .count();
    assert_eq!(errors, 1, "{text}");
}
