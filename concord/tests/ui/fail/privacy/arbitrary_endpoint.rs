use concord::api;

api! {
    client TypedApi {
        base "https://typed.example.test";
    }

    scope health() {
        path ["health"];
        GET ping() -> NoContent;
    }
}

fn main() {
    let client = TypedApi::new().unwrap();
    let _ = client.request(42_u32);
}
