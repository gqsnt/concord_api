use concord_macros::api;

api! {
    client Api {
        base "https://example.test";
        rate_limits { quota { by [host]; cost 2; 1 / 1s; } }
        rate_limit quota;
        GET Read() -> Bytes;
    }
}

fn main() {}
