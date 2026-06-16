use concord_macros::api;

api! {
    client UnknownBehaviorParentApi {
        base "https://example.com"

        behavior child extends missing {
            retry off
        }
    }
}

fn main() {}
