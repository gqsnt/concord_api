use concord_macros::api;

api! {
    client DuplicateBehaviorScopeApi {
        base "https://example.com"

        behavior read {
            retry off
        }
    }

    scope users {
        path ["users"]
        behavior read
        behavior read

        GET Me
        path ["me"]
        -> Json<()>
    }
}

fn main() {}
