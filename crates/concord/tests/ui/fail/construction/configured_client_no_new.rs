use concord::{SecretString, api};

api! {
    client ConfiguredApi {
        timeout off;
        base "http://configured.example.test/";
        values {
            tenant: String;
        }
        credentials {
            secret api_key;
            credential application = api_key(secret.api_key);
        }


    GET read() -> () using NoContent;
}}

fn main() {
    let _ = ConfiguredApi::new(
        "tenant".to_owned(),
        SecretString::try_new("protected").expect("bounded protected input"),
    );
}
