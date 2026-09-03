use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        credentials {
            secret token;
            credential bearer = bearer(secret.token);
        }
        rate_limit { key [secret.token]; allow 1 per 1s; }


    GET read() -> Bytes using Bytes;
}}

fn main() {}
