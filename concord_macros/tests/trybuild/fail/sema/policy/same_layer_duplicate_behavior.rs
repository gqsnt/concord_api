use concord_macros::api;

api! {
    client DuplicateBehaviorApi {
        base "https://example.com"

        behaviors {
            behavior read {
                retry off
            }
        }

        defaults {
            behavior read
            behavior read
        }
    }
}

fn main() {}
