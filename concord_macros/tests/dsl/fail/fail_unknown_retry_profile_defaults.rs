use concord_macros::api;

api! {
    client UnknownRetryProfileDefaultsApi {
        base "https://example.com"

        defaults {
            retry missing
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
