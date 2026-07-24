use concord::{SecretString, api};

api! {
    client ConfiguredApi {
        base "http://configured.example.test/";
        vars {
            tenant: String;
        }
        credentials {
            secret api_key: String;
            credential application = api_key(secret.api_key);
        }
    }

    GET read() -> NoContent {
        auth header "X-Api-Key" = application challenge never;
    }
}

fn main() {
    let client = ConfiguredApi::builder()
        .tenant("tenant".to_owned())
        .api_key(SecretString::try_new("protected").expect("bounded protected input"))
        .build()
        .expect("configured client");
    let _: configured_api::ReadRequest = client.read();
}
