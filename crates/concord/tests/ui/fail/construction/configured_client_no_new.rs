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

    GET read() -> NoContent;
}

fn main() {
    let _ = ConfiguredApi::new(
        "tenant".to_owned(),
        SecretString::try_new("protected").expect("bounded protected input"),
    );
}
