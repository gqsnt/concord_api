use std::process::{Command, Output};

fn no_default_managed_http_test() -> Output {
    Command::new(env!("CARGO"))
        .args([
            "test",
            "--offline",
            "--locked",
            "--color",
            "never",
            "-p",
            "concord_core",
            "--lib",
            "--no-default-features",
            "http::build::tests::no_default_http_works_and_https_preflight_is_typed",
            "--",
            "--exact",
        ])
        .output()
        .expect("focused no-default managed HTTP test should run")
}

#[test]
fn no_default_managed_http_contract_runs() {
    let output = no_default_managed_http_test();
    let diagnostics = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "no-default managed HTTP contract failed\n{diagnostics}"
    );
    assert!(
        diagnostics.contains("1 passed"),
        "no-default managed HTTP contract did not execute its focused test\n{diagnostics}"
    );
}
