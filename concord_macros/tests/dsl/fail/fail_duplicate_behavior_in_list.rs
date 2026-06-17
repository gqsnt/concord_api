use concord_macros::api;

api! {
    client DuplicateBehaviorListApi {
        base "https://example.com"

        behavior read {
            retry off
        }

        defaults {
            behavior [read, read]
        }
    }

    GET Ping
    path ["ping"]
    -> Json<()>
}

fn main() {}
