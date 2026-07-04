use concord_macros::api;

api! {
    client SecretPathAtomApi {
        base "https://example.com"
        secret token: String
    }

    GET Ping
        path [secret.token]
        -> Text<String>
}

fn main() {}
