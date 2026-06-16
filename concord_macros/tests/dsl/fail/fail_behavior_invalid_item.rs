use concord_macros::api;

api! {
    client InvalidBehaviorApi {
        base "https://example.com"

        behavior bad {
            path ["users"]
        }
    }
}

fn main() {}
