// §11.2: a secret builder input accepts only the explicit protected
// `SecretString`; no implicit conversion from plain text exists.
use concord::{Empty, api};

api! {
    client ConfiguredApi {
        base "https://configured.example.test/";
        credentials {
            secret api_key;
            credential application = api_key(secret.api_key);
        }

        GET read() -> () via Empty {
            auth { header "X-Api-Key" = application; }
        }
    }
}

fn main() {
    let _ = ConfiguredApi::builder().api_key("protected");
}
