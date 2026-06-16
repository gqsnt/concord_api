use concord_macros::api;

api! {
    client SelfBehaviorApi {
        base "https://example.com"

        behavior read extends read {
            retry off
        }
    }
}

fn main() {}
