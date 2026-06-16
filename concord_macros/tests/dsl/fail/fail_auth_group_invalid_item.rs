use concord_macros::api;

api! {
    client BadAuthGroupApi {
        base "https://example.com"

        auth {
            retry read
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
