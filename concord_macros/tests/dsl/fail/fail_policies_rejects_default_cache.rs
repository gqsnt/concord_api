use concord_macros::api;

api! {
    client BadPoliciesDefaultCacheApi {
        base "https://example.com"

        cache standard {
            ttl 60s
        }

        policies {
            cache standard
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
