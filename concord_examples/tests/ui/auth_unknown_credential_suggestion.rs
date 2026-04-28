use concord_macros::api;

api! {
    client UiAuthUnknownCredentialSuggestion {
        base https "example.com"
        secret token: String
        credential session = bearer(secret.token)
    }

    GET Me
        auth bearer sessoin
        -> Json<()>;
}

fn main() {}

