use concord_macros::api;

api! {
    client BadPoliciesDefaultRetryApi {
        base "https://example.com"

        retry read {
            max_attempts 2
            methods [GET]
        }

        policies {
            retry read
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
