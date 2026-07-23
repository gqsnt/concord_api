use concord_macros::api;

api! {
    client Api {
        base "https://example.test";
        rate_limit off;
    }

    GET read() -> Bytes;
}

fn main() {}
