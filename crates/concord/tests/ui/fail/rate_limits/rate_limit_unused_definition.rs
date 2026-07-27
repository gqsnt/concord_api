use concord::api;

api! {
    client Api {
        base "https://example.test";
        rate_limits {
            rule unused_rule {
                1 / 1s;
            }
        }
    }

    GET read() -> Bytes;
}

fn main() {}
