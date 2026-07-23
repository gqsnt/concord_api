use concord::api;

api! {
    client Api {
        base "https://example.test";
    }

    GET read() -> Bytes {
        rate_limit { 1 / 1s; }
        timeout 1s;
    }
}

fn main() {}
