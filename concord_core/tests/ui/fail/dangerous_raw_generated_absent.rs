use concord_macros::api;

api! {
    client GeneratedApi {
        base "https://example.test";
    }

    GET Read() -> Bytes;
}

fn main() {
    let client = GeneratedApi::new().unwrap();
    let _ = client.read().raw_response();
}
