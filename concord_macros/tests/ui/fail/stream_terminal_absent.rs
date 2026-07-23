use concord_macros::api;

api! {
    client StreamingApi {
        base "http://streaming.example.test";
    }

    scope transfers() {
        path ["transfers"];
        GET Download(verbose: Option<bool>) -> Stream<OctetStream>;
    }
}

fn main() {
    use streaming_api::prelude::*;

    let client = StreamingApi::new().unwrap();
    let _ = client.transfers().download().verbose(true).response();
}
