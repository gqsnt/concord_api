use std::path::{Path, PathBuf};

use concord_test_support::cargo_fixture::{diagnostics, run};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate should be inside the workspace")
        .to_owned()
}

#[test]
fn multipart_feature_activates_only_the_reqwest_multipart_capability() {
    let output = run(
        &workspace_root(),
        "multipart-feature/tree",
        "tree",
        [
            "-p",
            "concord_core",
            "--no-default-features",
            "--features",
            "multipart",
            "-e",
            "normal,build,features",
            "-i",
            "reqwest",
        ],
    );
    let diagnostics = diagnostics(&output);
    assert!(
        output.status.success(),
        "multipart feature tree failed\n{diagnostics}"
    );
    for expected in [
        "reqwest feature \"multipart\"",
        "reqwest feature \"stream\"",
    ] {
        assert!(
            diagnostics.contains(expected),
            "multipart feature tree omitted {expected}\n{diagnostics}"
        );
    }
    for forbidden in [
        "reqwest feature \"json\"",
        "reqwest feature \"gzip\"",
        "reqwest feature \"brotli\"",
        "reqwest feature \"deflate\"",
        "reqwest feature \"http2\"",
        "reqwest feature \"rustls",
    ] {
        assert!(
            !diagnostics.contains(forbidden),
            "multipart activated unrelated Reqwest capability {forbidden}\n{diagnostics}"
        );
    }
}
