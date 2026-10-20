// §11.1: one secret may feed several credential sources, each placed at its
// own exclusive target (AUTH-01).
use concord::{Empty, api};

api! {
    client SharedSecretApi {
        timeout 30s;
        base "https://example.test/";
        credentials {
            secret shared;
            credential header_key = api_key(secret.shared);
            credential query_key = api_key(secret.shared);
        }
        auth {
            header "x-api-key" = header_key;
            query "api_key" = query_key;
        }

        GET read() -> () via Empty;
    }
}

fn main() {}
