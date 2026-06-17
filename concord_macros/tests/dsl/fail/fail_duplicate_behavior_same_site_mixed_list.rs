use concord_macros::api;

api! {
    client DuplicateBehaviorMixedListApi {
        base "https://example.com"

        behavior read {
            retry off
        }

        defaults {
            behavior [read]
            behavior read
        }
    }

    GET Ping
    path ["ping"]
    -> Json<()>
}

fn main() {}
