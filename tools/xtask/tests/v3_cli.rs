//! Retired version-conversion commands are outside the V3 product surface.

use std::process::Command;

#[test]
fn retired_migration_command_is_rejected_and_absent_from_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_concord-xtask"))
        .args(["migrate-v3", "--help"])
        .output()
        .expect("run concord-xtask");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).expect("UTF-8 usage");
    assert!(error.contains("usage:"));
    assert!(error.contains("contract <export"));
    assert!(!error.contains("migrate-v3"));
}
