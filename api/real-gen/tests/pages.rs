//! Verify the compile probes retain the page's exact macro text.
use std::{fs, path::PathBuf};
#[test]
fn c1_20_eleven_real_descriptions_are_verbatim_and_the_failure_is_preserved() {
    let package = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let inventory: Vec<(String, String)> =
        serde_json::from_str(&fs::read_to_string(package.join("inventory.json")).unwrap()).unwrap();
    assert_eq!(inventory.len(), 11);
    for (module, page) in inventory {
        let source = if module == "telegram" {
            package.join("cases/fail/telegram.rs")
        } else {
            package.join(format!("src/{module}/description.rs"))
        };
        let description = fs::read_to_string(source).unwrap();
        let page = fs::read_to_string(package.join("../../docs/apis").join(page)).unwrap();
        assert!(
            page.contains(description.trim()),
            "{module}: the page's description was changed"
        );
    }
    let message = fs::read_to_string(package.join("cases/fail/telegram.stderr")).unwrap();
    assert!(message.contains("Secret: serde::Serialize"));
}

#[test]
fn c1_20_front_page_is_compiled_without_editing_its_description() {
    let package = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(package.join("src/guide/front.rs")).unwrap();
    let page = fs::read_to_string(package.join("../../docs/5.0.md")).unwrap();
    assert!(page.contains(source.trim()));
}
