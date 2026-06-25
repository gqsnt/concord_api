#[test]
fn examples_binary_runs_without_live_smoke_environment() {
    let binary = env!("CARGO_BIN_EXE_concord_examples");
    let output = std::process::Command::new(binary)
        .env_remove("CONCORD_RUN_RIOT_TEST")
        .env_remove("CONCORD_RUN_DDRAGON_TEST")
        .env_remove("RIOT_API_KEY")
        .output()
        .expect("run concord_examples binary");

    assert!(
        output.status.success(),
        "examples binary should run without live smoke env vars; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
