use concord_macros::api;

api! {
    client Api {
        base "https://example.test";
        rate_limits {
            quota { 1 / 1s; }
        }
    }

    GET Read() -> Bytes;
}

fn main() {}
