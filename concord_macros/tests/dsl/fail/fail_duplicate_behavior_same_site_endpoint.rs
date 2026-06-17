use concord_macros::api;

api! {
    client DuplicateBehaviorEndpointApi {
        base "https://example.com"

        behavior read {
            retry off
        }
    }

    GET Me
    path ["me"]
    behavior read
    behavior read
    -> Json<()>
}

fn main() {}
