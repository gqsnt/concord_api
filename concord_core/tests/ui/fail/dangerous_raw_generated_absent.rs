use concord_macros::api;

api! {
    client FeatureIsolationApi {
        base "http://feature-isolation.example.test/";
        scope health() {
            path ["health"];
            GET Ping() -> Bytes;
        }
    }
}

fn main() {
    let client = FeatureIsolationApi::new().expect("client construction");
    let _ = client.health().ping().raw_response();
}
