use std::path::{Path, PathBuf};
use std::process::Output;

use concord_test_support::cargo_fixture::{diagnostics, run};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate should be inside the workspace")
        .to_owned()
}

fn reqwest_feature_tree(feature: &str) -> Output {
    run(
        &workspace_root(),
        &format!("compression-tree/{feature}"),
        "tree",
        [
            "-p",
            "concord-core",
            "--no-default-features",
            "--features",
            feature,
            "-e",
            "normal,build,features",
            "-i",
            "reqwest",
        ],
    )
}

#[test]
fn compression_features_are_independently_selectable() {
    let features = ["gzip", "brotli", "deflate"];

    for enabled in features {
        let output = reqwest_feature_tree(enabled);
        let diagnostics = diagnostics(&output);
        assert!(
            output.status.success(),
            "feature tree failed for {enabled}\n{diagnostics}"
        );

        for candidate in features {
            let marker = format!("reqwest feature \"{candidate}\"");
            assert_eq!(
                diagnostics.contains(&marker),
                candidate == enabled,
                "feature {enabled} had an unexpected Reqwest compression edge for {candidate}\n{diagnostics}"
            );
        }
    }
}
