use concord_macros::api;

api! {
    client SecretFmtPathAtomApi {
        base "https://example.com"
        secret token: String
    }

    GET Ping
        path [fmt["prefix-", secret.token]]
        -> Text<String>
}

fn main() {}
