use concord_macros::api;

api! {
    client BadAuthUseInGroupApi {
        base "https://example.com"

        auth {
            auth bearer session
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
