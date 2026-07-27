use concord::api;

api! {
    client RemovedRetryApi {
        base "https://example.test/";
        retry reqwest_default;
    }
}

fn main() {}
