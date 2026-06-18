#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Concord examples are available as library modules and integration tests: minimal, docs_dsl, docs_advanced_dsl, auth_session, pagination, custom_pagination, custom_codec, policy_stack, explicit_endpoint, riot, and ddragon."
    );

    concord_examples::riot::riot_test().await?;

    concord_examples::ddragon::ddragon_test().await?;

    Ok(())
}
