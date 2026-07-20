use concord_macros::api;

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
    let client = Api::new("protected").unwrap();
    let credential = client.credentials().session().credential();
    let _ = credential.clear();
}
