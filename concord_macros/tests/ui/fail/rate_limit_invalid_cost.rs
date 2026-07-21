use concord_macros::api;

api! {
    client Api {
        base "https://example.test";
        rate_limit { cost 2; 1 / 1s; }
        GET Read() -> Bytes;
    }
}

fn main() {}
