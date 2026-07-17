use std::process::{Command, Output};

fn reqwest_feature_tree(feature: &str) -> Output {
    Command::new(env!("CARGO"))
        .args([
            "tree",
            "--offline",
            "--color",
            "never",
            "-p",
            "concord_core",
            "--no-default-features",
            "--features",
            feature,
            "-e",
            "normal,build,features",
            "-i",
            "reqwest",
        ])
        .output()
        .expect("Cargo feature-tree command should run")
}

fn diagnostics(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
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
