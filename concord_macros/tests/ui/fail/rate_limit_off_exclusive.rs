use concord_macros::api;

api! {
    client Api {
        base "https://example.test";
        rate_limits { quota { by [host]; 1 / 1s; } }
        rate_limit off;
        rate_limit quota;
        GET Read() -> Bytes;
    }
}

fn main() {}
