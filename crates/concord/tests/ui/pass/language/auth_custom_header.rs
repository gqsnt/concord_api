// AUTH-01: an API-key credential placed in a custom header; the credential
// name may equal the secret name (separate source namespace, §3.4).
use concord::{Empty, api};

api! {
    client CustomAuthHeaderApi {
        timeout 30s;
        base "https://example.test/";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }
        auth { header "X-API-Key" = key; }

        GET read() -> () via Empty;
    }
}

fn main() {}
