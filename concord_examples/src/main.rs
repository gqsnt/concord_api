#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Concord examples are available as library modules and integration tests: minimal, docs_dsl, docs_advanced_dsl, auth_session, pagination, custom_pagination, custom_codec, policy_stack, explicit_endpoint, riot, and ddragon."
    );

    if std::env::var_os("CONCORD_RUN_RIOT_TEST").is_some() {
        concord_examples::riot::riot_test().await?;
    }

    if std::env::var_os("CONCORD_RUN_DDRAGON_TEST").is_some() {
        concord_examples::ddragon::ddragon_test().await?;
    }

    Ok(())
}
