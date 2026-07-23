use concord_macros::api;

api! {
    client Api {
        base "https://example.test";
        credentials {
            secret token: String;
            credential bearer = bearer(secret.token);
        }
        rate_limit { by [secret.token]; 1 / 1s; }
    }

    GET read() -> Bytes;
}

fn main() {}
