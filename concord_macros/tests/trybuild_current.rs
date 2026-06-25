static TRYBUILD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn trybuild_pass_contract_fixtures() {
    run_trybuild(|t| {
        t.pass("tests/trybuild/pass/*.rs");
    });
}

#[test]
fn trybuild_auth_and_secret_diagnostics() {
    run_trybuild(|t| {
        t.compile_fail("tests/trybuild/fail/auth/*.rs");
    });
}

#[test]
fn trybuild_route_and_fmt_diagnostics() {
    run_trybuild(|t| {
        t.compile_fail("tests/trybuild/fail/route/*.rs");
        t.compile_fail("tests/trybuild/fail/fmt/*.rs");
    });
}

#[test]
fn trybuild_policy_diagnostics() {
    run_trybuild(|t| {
        t.compile_fail("tests/trybuild/fail/policy/*.rs");
    });
}

#[test]
fn trybuild_pagination_diagnostics() {
    run_trybuild(|t| {
        t.compile_fail("tests/trybuild/fail/pagination/*.rs");
    });
}

#[test]
fn trybuild_codegen_contract_diagnostics() {
    run_trybuild(|t| {
        t.compile_fail("tests/trybuild/fail/codegen/*.rs");
    });
}

fn run_trybuild(run: impl FnOnce(&trybuild::TestCases)) {
    let _guard = TRYBUILD_LOCK.lock().expect("trybuild lock poisoned");
    set_trybuild_target();

    let t = trybuild::TestCases::new();
    run(&t);
}

fn set_trybuild_target() {
    let target_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("trybuild-current");

    // Keep trybuild artifacts out of the workspace target while allowing the
    // serialized category tests to reuse dependency builds.
    unsafe {
        std::env::set_var("CARGO_TARGET_DIR", target_dir);
    }
}
