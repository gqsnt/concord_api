use concord_macros::api;

api! {
    client DuplicateBehaviorMixedApi {
        base "https://example.com"

        behavior read {
            retry off
        }

        behaviors {
            behavior read {
                retry off
            }
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
