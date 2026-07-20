use concord_macros::api;

api! {
    client Api {
        base "https://example.test";
        credentials {
            secret token: String;
            credential bearer = bearer(secret.token);
        }
        rate_limits { quota { by [secret.token]; 1 / 1s; } }
        rate_limit quota;
        GET Read() -> Bytes;
    }
}

fn main() {}
