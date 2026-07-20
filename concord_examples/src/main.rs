#![forbid(unsafe_code)]

use concord_core::Request;
use concord_macros::api;

api! {
    client MinimalApi {
        base "https://minimal.example.test/v1/";
        GET Health() -> NoContent;
    }
}

fn main() -> Result<(), concord_core::prelude::ClientBuildError> {
    let client = MinimalApi::builder().build()?;
    let _: Request<_> = client.health();
    Ok(())
}
