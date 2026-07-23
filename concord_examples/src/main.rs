#![forbid(unsafe_code)]

use concord::api;

api! {
    client MinimalApi {
        base "https://minimal.example.test/v1/";
    }

    GET health() -> NoContent;
}

fn main() -> Result<(), concord::prelude::ClientBuildError> {
    let client = MinimalApi::builder().build()?;
    let _: minimal_api::HealthRequest = client.health();
    Ok(())
}
