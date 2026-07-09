use concord_macros::api;

api! {
    client UnknownBehaviorUseApi {
        base "https://example.com"
    }

    scope users {
        path ["users"]
        profile missing
    }
}

fn main() {}
