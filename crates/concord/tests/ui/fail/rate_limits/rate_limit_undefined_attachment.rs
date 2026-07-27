use concord::api;

api! {
    client Api {
        base "https://example.test";
        rate_limit use missing;
    }

    GET read() -> Bytes;
}

fn main() {}
