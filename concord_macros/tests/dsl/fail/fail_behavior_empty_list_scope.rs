use concord_macros::api;

api! {
    client EmptyBehaviorListScopeApi {
        base "https://example.com"
    }

    scope users {
        path ["users"]
        behavior []
    }
}

fn main() {}
