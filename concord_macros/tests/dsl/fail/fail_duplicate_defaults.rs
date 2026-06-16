use concord_macros::api;

api! {
    client DuplicateDefaultsAliasApi {
        base "https://example.com"

        defaults {
            retry off
        }

        defaults {
            retry off
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
