//! Executable Rust and a readable expansion from the original descriptions.
use std::{fs, path::PathBuf};

fn package() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn description(source: &str) -> proc_macro2::TokenStream {
    let file = syn::parse_file(source).unwrap();
    file.items
        .into_iter()
        .find_map(|item| match item {
            syn::Item::Macro(item) if item.mac.path.segments.last().unwrap().ident == "api" => {
                Some(item.mac.tokens)
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn tasks_expansion_is_readable_and_stable() {
    let source = fs::read_to_string(package().join("../tests/tour-gen/src/tasks.rs")).unwrap();
    let expanded = crate::expand(description(&source));
    let file = syn::parse2::<syn::File>(expanded).unwrap();
    let text = prettyplease::unparse(&file);
    fs::create_dir_all(package().join("../target/expansion")).unwrap();
    fs::write(package().join("../target/expansion/tasks.rs"), &text).unwrap();
    assert_eq!(
        text,
        fs::read_to_string(package().join("tests/snapshots/generated/tasks.rs")).unwrap()
    );
}

#[test]
fn c1_20_regression_api_descriptions_expand_to_rust_files() {
    let mut count = 0;
    for entry in fs::read_dir(package().join("tests/snapshots/resolved")).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("tour_")
            || path.extension().is_none_or(|x| x != "api")
        {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        let tokens = if source.trim_start().starts_with("concord::api!") {
            description(&source)
        } else {
            source.parse().unwrap()
        };
        let expanded = crate::expand(tokens);
        assert!(
            !expanded.to_string().contains("compile_error"),
            "{}: {expanded}",
            path.display()
        );
        syn::parse2::<syn::File>(expanded)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        count += 1;
    }
    assert_eq!(count, 11, "all eleven API descriptions must be checked");
}

#[test]
fn c1_6_retry_alias_reserves_its_generated_type_namespace() {
    let expanded = crate::expand(
        r#"client Names { base "https://names.test"; GET retry(); }"#
            .parse()
            .unwrap(),
    )
    .to_string();
    assert!(expanded.contains("`retry` is reserved by Concord"));
}

#[path = "generated_docs.rs"]
mod docs;

#[path = "generated_surface.rs"]
mod surface;

#[path = "generation.rs"]
mod generation;

/// Measure an input named by CONCORD_MEASURE_INPUT; write JSON to CONCORD_MEASURE_OUTPUT.
#[test]
#[ignore = "run just bench --expansion to measure documentation descriptions"]
fn c1_16_measure_macro_expansion_once() {
    let input = std::fs::read_to_string(std::env::var("CONCORD_MEASURE_INPUT").unwrap()).unwrap();
    let tokens = description(&input);
    let start = std::time::Instant::now();
    let expanded = std::hint::black_box(crate::expand(tokens));
    let seconds = start.elapsed().as_secs_f64();
    let text = expanded.to_string();
    assert!(!text.contains("compile_error"));
    let lines = prettyplease::unparse(&syn::parse2(expanded).unwrap())
        .lines()
        .count();
    std::fs::write(
        std::env::var("CONCORD_MEASURE_OUTPUT").unwrap(),
        format!(
            "{{\"seconds\":{seconds},\"output_bytes\":{},\"formatted_lines\":{lines}}}\n",
            text.len()
        ),
    )
    .unwrap();
}
