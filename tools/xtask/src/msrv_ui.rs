use std::path::Path;

use crate::cargo::assert_semantic_compile_failures;

pub(crate) fn run(root: &Path) {
    let manifest = root.join("fixtures/Cargo.toml");
    for (target_key, package, owner) in [
        (
            "msrv/facade-semantic-ui",
            "concord-msrv-facade",
            "crates/concord/tests/ui/msrv",
        ),
        (
            "msrv/core-semantic-ui",
            "concord-msrv-core",
            "crates/concord-core/tests/ui/msrv",
        ),
    ] {
        let owner = root.join(owner);
        assert_semantic_compile_failures(
            root,
            target_key,
            &manifest,
            package,
            &owner.join("expectations.txt"),
            &owner.join("forbidden.txt"),
        );
    }
    println!("MSRV semantic UI checks passed");
}
