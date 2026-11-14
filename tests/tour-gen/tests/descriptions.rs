//! Compare macro inputs with the hand-written clients' leading descriptions.
use std::{fs, path::PathBuf};

#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
fn comment(source: &str, marker: &str) -> String {
    let lines: Vec<_> = source.lines().collect();
    let start = lines.iter().position(|line| line.contains(marker)).unwrap();
    let mut description = String::new();
    for line in &lines[start..] {
        let Some(text) = line.strip_prefix("    // ") else {
            break;
        };
        description.push_str(text);
        description.push('\n');
        if text == "}" {
            break;
        }
    }
    description
}

#[test]
fn c1_20_all_five_generated_descriptions_match_the_hand_written_tokens() {
    let package = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for client in ["tasks", "feed", "riot", "files", "shapes"] {
        let source = fs::read_to_string(package.join(format!("../tour/src/{client}.rs"))).unwrap();
        let mut description = comment(&source, "// ///");
        if client == "files" {
            description.truncate(description.len() - 2);
            let grant = fs::read_to_string(package.join("../tour/src/grant.rs")).unwrap();
            description.push_str(&comment(&grant, "// POST grant("));
            description.push_str("}\n");
        }
        let original: syn::ItemMacro =
            syn::parse_str(&format!("concord::api! {{ {description} }}")).unwrap();
        let generated: syn::ItemMacro =
            syn::parse_str(&fs::read_to_string(package.join(format!("src/{client}.rs"))).unwrap())
                .unwrap();
        assert_eq!(
            generated.mac.tokens.to_string(),
            original.mac.tokens.to_string(),
            "{client}"
        );
    }
}

/// C10.3: each original integration test file also runs against generated clients.
#[test]
fn c10_3_every_original_integration_suite_is_included_by_path() {
    let package = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for (original, generated) in [("tour", "tour-gen"), ("conformance", "conformance-gen")] {
        let source = package.join(format!("../{original}/tests"));
        for entry in fs::read_dir(source).unwrap() {
            let file = entry.unwrap().path();
            if file.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let filename = file.file_name().unwrap();
            let wrapper = package.join(format!("../{generated}/tests")).join(filename);
            let text = fs::read_to_string(&wrapper)
                .unwrap_or_else(|error| panic!("{}: {error}", wrapper.display()));
            let expected = format!(
                "#[path = \"../../{original}/tests/{}\"]",
                filename.to_string_lossy()
            );
            assert!(
                text.contains(&expected),
                "{} must include {expected}",
                wrapper.display()
            );
            assert!(
                text.contains("extern crate tour_gen as tour;"),
                "{} must use generated clients",
                wrapper.display()
            );
        }
    }
}
