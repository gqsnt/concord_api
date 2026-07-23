use concord::api;

api! {
    client Api {
        base "https://example.test";
        rate_limit { cost 2; 1 / 1s; }
    }

    GET read() -> Bytes;
}

fn main() {}
