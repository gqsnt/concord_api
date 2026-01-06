mod riot;

use concord_core::prelude::*;
use concord_examples::test_riot;

#[tokio::main]
async fn main() -> Result<(), ApiClientError> {
    //test_api().await?;
    test_riot().await?;
    Ok(())
}
