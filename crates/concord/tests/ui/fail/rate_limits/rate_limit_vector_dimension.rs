use concord::{Bytes, api};

api! {
    client VectorKeyApi {
        timeout 30s;
        base "https://example.test";
        values {
            tenants: Vec<String>;
        }
        rate_limit { id "concord.rate_limit_vector_dimension.1"; cost 1;
            key [client.tenants];
            allow 1 per 1s;
        }

        GET read() -> Bytes via Binary;
    }
}

fn main() {}
