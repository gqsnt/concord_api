use concord_macros::api;

api! {
    client BadPoliciesApi {
        base "https://example.com"

        policies {
            secret token: String
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
