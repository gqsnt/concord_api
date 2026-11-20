use std::io;
use std::process::Command;

pub(super) fn compile_fail(tests: &trybuild::TestCases, group: &str) -> io::Result<()> {
    if std::env::var("CONCORD_MSRV").as_deref() != Ok("1") {
        tests.compile_fail(format!("tests/compiler-errors/{group}/*.rs"));
        return Ok(());
    }

    let output = Command::new("python3")
        .args(["-B", "tests/diagnostic_spans.py", group])
        .output()?;
    assert!(
        output.status.success(),
        "{group}:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
