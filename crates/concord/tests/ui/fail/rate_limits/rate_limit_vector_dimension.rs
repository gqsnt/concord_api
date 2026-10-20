// §12.3: repetition is not a dimension; `Vec<String>` has no canonical
// `RateKeyPart` encoding.
use concord::{Binary, Bytes, api};

api! {
    client VectorKeyApi {
        base "https://example.test";
        values { tenants: Vec<String>; }
        rate_limit {
            id "concord.rate_limit_vector_dimension.1";
            key ["tenants" = tenants];
            allow 1 per 1s;
        }

        GET read() -> Bytes via Binary;
    }
}

fn main() {}
