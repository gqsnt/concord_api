use concord_macros::api;

api! {
    client Api {
        base "https://example.test";
        rate_limit missing;
        GET Read() -> Bytes;
    }
}

fn main() {}
