use concord_macros::api;

api! {
    client DuplicateBehaviorApi {
        base "https://example.com"

        profiles {
            profile read {
                retry off
            }
        }

        default {
            profile read
            profile read
        }
    }
}

fn main() {}
