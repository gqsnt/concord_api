#![forbid(unsafe_code)]

use concord_core::dangerous::RawResponseRequest;
use concord_macros::api;

api! {
    client RawResponseApi {
        base "https://raw-response.example.test/";
        GET Record() -> Json<Vec<u32>>;
    }
}

fn main() -> Result<(), concord_core::prelude::ClientBuildError> {
    let client = RawResponseApi::builder().build()?;
    let _: RawResponseRequest<_> = client.record().raw_response();
    Ok(())
}
