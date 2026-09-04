use concord::api;

api! {
    client RemovedAcceptApi {
        timeout 30s;
        base "https://example.test";

        GET read() -> String via Text {
            accept "application/json";
        }
    }
}

fn main() {}
