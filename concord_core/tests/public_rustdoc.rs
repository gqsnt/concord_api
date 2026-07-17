use std::path::{Path, PathBuf};

use concord_test_support::cargo_fixture::{diagnostics, run};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate should be inside the workspace")
        .to_owned()
}

#[test]
fn stable_request_terminals_are_semantic_and_private_in_rendered_rustdoc() {
    let root = workspace_root();
    let target_key = "core/public-rustdoc";
    let output = run(
        &root,
        target_key,
        "rustdoc",
        [
            "-p",
            "concord_core",
            "--all-features",
            "--lib",
            "--",
            "-Dwarnings",
        ],
    );
    assert!(
        output.status.success(),
        "core public Rustdoc failed\n{}",
        diagnostics(&output)
    );

    let documentation = root
        .join("target/cargo-fixtures")
        .join(target_key)
        .join("doc/concord_core");
    let request_path = documentation.join("struct.Request.html");
    let response_path = documentation.join("struct.ResponseRequest.html");
    let request = std::fs::read_to_string(&request_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", request_path.display()));
    let response = std::fs::read_to_string(&response_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", response_path.display()));

    for expected in [
        "id=\"method.response\"",
        "Selects successful response metadata",
        "ResponseRequest",
        "buffered, bytes, or no-content",
    ] {
        assert!(
            request.contains(expected),
            "Request Rustdoc omitted the public response terminal fact {expected:?}"
        );
    }
    assert!(
        response.contains("<title>ResponseRequest in concord_core"),
        "ResponseRequest Rustdoc page omitted its public identity"
    );
    assert!(
        response.contains("pub struct ResponseRequest"),
        "ResponseRequest Rustdoc omitted its public declaration"
    );

    for forbidden in [
        "MetadataResponse",
        "__private",
        "EndpointSpec",
        "ResponseHandler",
        "ClientCore",
        "PreparedBody",
        "execute_request",
        "reqwest",
    ] {
        assert!(
            !request.contains(forbidden) && !response.contains(forbidden),
            "stable request Rustdoc exposed private integration name {forbidden}"
        );
    }
}
