//! What this crate compiles is what the pages print, word for word.
use std::{fs, path::PathBuf};

const APIS: [&str; 8] = [
    "elasticsearch",
    "github",
    "kubernetes",
    "riot",
    "s3",
    "slack",
    "stripe",
    "telegram",
];

#[test]
fn c1_20_every_documented_description_is_accepted_and_compiles() {
    let package = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let status = std::process::Command::new("python3")
        .args(["-B", "tools/doc_examples.py", "--check"])
        .current_dir(package.join("../.."))
        .status()
        .unwrap();
    assert!(
        status.success(),
        "page descriptions or their compiled sources drifted"
    );
    let inventory: Vec<serde_json::Value> = serde_json::from_str(
        &fs::read_to_string(package.join("fixtures/descriptions/manifest.json")).unwrap(),
    )
    .unwrap();
    let compiled = real_apis::guide::compiled_descriptions();
    assert_eq!(compiled.len(), inventory.len());
    for (client, case) in compiled.iter().zip(&inventory) {
        assert!(
            client.ends_with(case["client"].as_str().unwrap()),
            "{case}: {client}"
        );
    }
    for api in APIS {
        let page = package.join(format!("../../docs/examples/{api}.md"));
        let page = fs::read_to_string(page).unwrap();
        // Every file of the module but `mod.rs` and the stand-ins is a block of the page.
        let mut blocks = 0;
        for entry in fs::read_dir(package.join(format!("src/{api}"))).unwrap() {
            let path = entry.unwrap().path();
            let file = path.file_name().unwrap().to_str().unwrap().to_owned();
            if file == "mod.rs" || file.starts_with("stand_in") {
                continue;
            }
            let code = fs::read_to_string(&path).unwrap();
            assert!(
                page.contains(&format!("```rust\n{}\n```", code.trim())),
                "{api}: the page does not print src/{api}/{file} as one block"
            );
            blocks += 1;
        }
        assert!(
            blocks >= 3,
            "{api}: a description, the Rust beside it, the calls"
        );
    }
}
