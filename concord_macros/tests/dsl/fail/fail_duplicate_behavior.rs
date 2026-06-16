use concord_macros::api;

api! {
    client DupBehaviorApi {
        base "https://example.com"

        behavior read { retry off }
        behavior read { retry off }
    }
}

fn main() {}
