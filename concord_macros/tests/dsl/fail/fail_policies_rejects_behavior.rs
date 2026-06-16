use concord_macros::api;

api! {
    client BadPoliciesBehaviorApi {
        base "https://example.com"

        policies {
            behavior read {
                retry off
            }
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
