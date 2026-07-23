#![forbid(unsafe_code)]

use concord_core::Request;
use concord_core::advanced::{HeaderName, Method, PreparedEndpoint};
use concord_macros::api;

api! {
    client PreparedApi {
        base "https://prepared.example.test/";
    }

    GET Generated() -> NoContent;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = PreparedApi::builder().build()?;
    let endpoint = PreparedEndpoint::builder("PreparedApi::lookup", Method::POST)
        .path_segment("records")
        .path_segment("42")
        .query("view", "summary")
        .header(HeaderName::from_static("x-example"), "canonical")
        .body_codec::<concord_examples::ExampleCodec>("request body".to_owned())
        .response::<concord_examples::ExampleCodec>()
        .build()?;
    let _: Request<_> = client.request(endpoint);
    Ok(())
}
