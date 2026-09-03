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


    GET read() -> () using NoContent {
        auth { header "X-Api-Key" = application; }
    }
}}

fn main() {
    let client = ConfiguredApi::builder()
        .set_tenant("tenant".to_owned())
        .api_key(SecretString::try_new("protected").expect("bounded protected input"))
        .build()
        .expect("configured client");
    let _: configured_api::ReadRequest = client.read();
}
