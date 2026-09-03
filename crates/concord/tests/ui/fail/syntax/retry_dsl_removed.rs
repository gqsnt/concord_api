use concord::api;

api! {
    client RemovedRetryApi {
        timeout off;
        base "https://example.test/";
        retry reqwest_default;
    }
}

fn main() {}
