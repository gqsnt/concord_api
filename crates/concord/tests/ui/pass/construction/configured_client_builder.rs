// §4.3 / §20.4: a client with a required root value and a secret has no
// `new()`; the builder takes `set_<value>` and the secret-named protected
// setter. The required value is live through the client query program
// (VALUE-06).
use concord::{Empty, SecretString, api};

api! {
    client ConfiguredApi {
        timeout 30s;
        base "http://configured.example.test/";
        values {
            tenant: String;
        }
        credentials {
            secret api_key;
            credential application = api_key(secret.api_key);
        }
        query { "tenant" = tenant; }

        GET read() -> () via Empty {
            auth { header "X-Api-Key" = application; }
        }
    }
}

fn main() {
    let client = ConfiguredApi::builder()
        .set_tenant("tenant".to_owned())
        .api_key(SecretString::try_new("protected").expect("bounded protected input"))
        .build()
        .expect("configured client");
    let _: configured_api::ReadRequest = client.read();
}
