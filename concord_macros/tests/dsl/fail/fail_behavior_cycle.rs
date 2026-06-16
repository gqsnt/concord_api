use concord_macros::api;

api! {
    client CycleBehaviorApi {
        base "https://example.com"

        behavior a extends b {
            retry off
        }

        behavior b extends a {
            retry off
        }
    }
}

fn main() {}
