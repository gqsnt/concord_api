use concord_macros::api;

api! {
    client Api {
        base "https://example.test";
        rate_limits { quota { by [host]; 1 / 1s; } }
        GET Read() -> Bytes {
            rate_limit quota;
            timeout 1s;
        }
    }
}

fn main() {}
