use concord_macros::api;

api! {
    client Api {
        base "https://example.test";
        rate_limit off;
    }

    GET Read() -> Bytes;
}

fn main() {}
