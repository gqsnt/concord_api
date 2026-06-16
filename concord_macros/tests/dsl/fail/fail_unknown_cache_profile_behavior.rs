use concord_macros::api;

api! {
    client UnknownCacheProfileBehaviorApi {
        base "https://example.com"

        behavior bad {
            cache missing
        }

        defaults {
            behavior bad
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
