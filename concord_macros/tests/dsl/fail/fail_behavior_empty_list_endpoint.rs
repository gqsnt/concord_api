use concord_macros::api;

api! {
    client EmptyBehaviorListEndpointApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    behavior []
    -> Text<String>
}

fn main() {}
