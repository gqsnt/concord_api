#![forbid(unsafe_code)]

use concord::api;
use concord::dangerous::RawResponseRequest;

api! {
    client RawResponseApi {
        base "https://raw-response.example.test/";
    }

    GET Record() -> Json<Vec<u32>>;
}

fn main() -> Result<(), concord::prelude::ClientBuildError> {
    let client = RawResponseApi::builder().build()?;
    let _: RawResponseRequest<_> = client.record().raw_response();
    Ok(())
}
