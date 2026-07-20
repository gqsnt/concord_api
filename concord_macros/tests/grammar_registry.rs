#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::path::Path;

const REGISTRY: &str = include_str!("release/grammar_registry.csv");

#[test]
fn completed_dsl_grammar_registry_has_exact_trace_families() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let mut lines = REGISTRY.lines();
    assert_eq!(
        lines.next(),
        Some(
            "production,positive_parser,semantic_resolution,normalized_model,generated_compile,invalid_diagnostic"
        )
    );

    let mut productions = HashSet::new();
    let mut count = 0_usize;
    for (index, line) in lines.enumerate() {
        assert!(
            !line.trim().is_empty(),
            "blank registry row at {}",
            index + 2
        );
        let columns: Vec<_> = line.split(',').collect();
        assert_eq!(columns.len(), 6, "invalid registry row at {}", index + 2);
        assert!(
            productions.insert(columns[0]),
            "duplicate grammar production {}",
            columns[0]
        );
        for reference in &columns[1..] {
            if *reference == "not_applicable" {
                continue;
            }
            let (path, marker) = reference
                .split_once('#')
                .expect("registry references use path#marker");
            let path = root.join(path);
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            assert!(
                source.contains(marker),
                "{} does not contain registered marker {marker:?}",
                path.display()
            );
        }
        count += 1;
    }

    assert_eq!(
        count, 68,
        "the release grammar registry changed unexpectedly"
    );
    for required in [
        "credentials_block",
        "retry_custom",
        "rate_limit_observer",
        "auth_replace",
        "request_multipart",
        "response_stream",
        "paginate_controller_type",
        "paginate_bound_field",
        "paginate_stable_field",
        "paginate_struct_block",
    ] {
        assert!(productions.contains(required), "missing {required}");
    }
}
