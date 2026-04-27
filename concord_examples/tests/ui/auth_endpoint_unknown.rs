use concord_macros::api;

api! {
    client UiAuthEndpointUnknown {
        base https "example.com"
            credential session = endpoint auth_api::LoginMissing // ERROR: unknown endpoint reference
    }

    GET Ping
    -> Json<()>
    {
        auth bearer session
    }
}

fn main() {}
