use concord::api;

api! {
    client RemovedRetryApi {
        timeout 30s;
        base "https://example.test/";
        retry reqwest_default;
    }
}

fn main() {}
