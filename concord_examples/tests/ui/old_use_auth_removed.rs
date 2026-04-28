use concord_macros::api;

api! {
    client OldUseAuthSyntax {
        base https "example.com"
        secret key: String
        credential key = api_key(secret.key)

        use_auth HeaderAuth("X-Api-Key", key)
    }
}

fn main() {}
