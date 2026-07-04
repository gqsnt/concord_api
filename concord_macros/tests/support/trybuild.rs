static TRYBUILD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn run_trybuild(run: impl FnOnce(&trybuild::TestCases)) {
    let _guard = TRYBUILD_LOCK.lock().expect("trybuild lock poisoned");
    set_trybuild_target();

    let t = trybuild::TestCases::new();
    run(&t);
}

#[allow(dead_code)]
pub fn run_trybuild_pass(paths: &[&str]) {
    run_trybuild(|t| {
        for path in paths {
            t.pass(path);
        }
    });
}

#[allow(dead_code)]
pub fn run_trybuild_fail(paths: &[&str]) {
    run_trybuild(|t| {
        for path in paths {
            t.compile_fail(path);
        }
    });
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
