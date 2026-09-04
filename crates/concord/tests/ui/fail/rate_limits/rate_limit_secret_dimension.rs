use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        credentials {
            secret token;
            credential bearer = bearer(secret.token);
        }
        rate_limit { id "concord.rate_limit_secret_dimension.1"; cost 1;  key [secret.token]; allow 1 per 1s; }


    GET read() -> Bytes via Binary;
}}

fn main() {}
