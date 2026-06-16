use concord_macros::api;

api! {
    client DuplicateBehaviorCacheApi {
        base "https://example.com"

        cache standard {
            ttl 60s
        }

        behavior bad {
            cache standard
            cache off
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
