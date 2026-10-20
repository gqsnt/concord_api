// §7.3: `Client::new()` is generated only when no required ordinary value,
// secret/provider or limiter choice remains; configured clients use the
// builder (see pass/construction/configured_client_builder.rs).
use concord::{Empty, SecretString, api};

api! {
    client ConfiguredApi {
        timeout 30s;
        base "https://configured.example.test/";
        values {
            tenant: String;
        }
        credentials {
            secret api_key;
            credential application = api_key(secret.api_key);
        }
        query { "tenant" = tenant; }
        auth { header "X-Api-Key" = application; }

        GET read() -> () via Empty;
    }
}

api! {
    client SecretOnlyApi {
        base "https://secret-only.example.test/";
        credentials {
            secret api_key;
            credential application = api_key(secret.api_key);
        }
        auth { header "X-Api-Key" = application; }

        GET read() -> () via Empty;
    }
}

fn main() {
    let _ = ConfiguredApi::new(
        "tenant".to_owned(),
        SecretString::try_new("protected").expect("bounded protected input"),
    );
    let _ = SecretOnlyApi::new();
}
