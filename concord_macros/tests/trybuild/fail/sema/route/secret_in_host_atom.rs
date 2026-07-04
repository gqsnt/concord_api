use concord_macros::api;

api! {
    client SecretHostAtomApi {
        base "https://example.com"
        secret token: String
    }

    scope SecretHost {
        host [secret.token]

        GET Ping
            path ["ping"]
            -> Text<String>
    }
}

fn main() {}
