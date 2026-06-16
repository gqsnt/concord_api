use concord_macros::api;

api! {
    client BadBehaviorsApi {
        base "https://example.com"

        behaviors {
            retry read
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
