use concord::{Bytes, api};

api! {
    client VectorKeyApi {
        timeout off;
        base "https://example.test";
        values {
            tenants: Vec<String>;
        }
        rate_limit {
            key [client.tenants];
            allow 1 per 1s;
        }

        GET read() -> Bytes using Bytes;
    }
}

fn main() {}
