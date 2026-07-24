use concord::prelude::SecretString;
use concord::api;

api! {
    client Api {
        base "http://example.test";
        credentials {
            secret token: String;
            credential session = bearer(secret.token);
        }
    }
}

fn main() {
    let client = Api::builder()
        .token(SecretString::try_new("protected").expect("bounded protected input"))
        .build()
        .unwrap();
    let credential = client.credentials().session().credential();
    let _ = credential.clear();
}
