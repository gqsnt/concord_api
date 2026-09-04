use concord::api;

api! {
    client GeneratedAcceptApi {
        timeout 30s;
        base "https://example.test";

        GET read() -> String via Text;
    }
}

fn main() {
    let client = GeneratedAcceptApi::new().unwrap();
    let _ = client.read().accept("application/json");
}
