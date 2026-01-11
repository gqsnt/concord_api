use concord_examples::riot::test_riot;
use concord_examples::test_api::test_api;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    test_api().await?;
    test_riot().await?;
    Ok(())
}
