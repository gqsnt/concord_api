#![forbid(unsafe_code)]

use concord::Request;
use concord::api;

api! {
    client MinimalApi {
        base "https://minimal.example.test/v1/";
    }

    GET Health() -> NoContent;
}

fn main() -> Result<(), concord::prelude::ClientBuildError> {
    let client = MinimalApi::builder().build()?;
    let _: Request<_> = client.health();
    Ok(())
}
