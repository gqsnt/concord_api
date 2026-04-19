use concord_macros::api;

api! {
    client UiAuthEndpointUnknown {
        scheme: https,
        host: "example.com",
        auth {
            credential session: Endpoint(auth::LoginMissing) // ERROR: unknown endpoint reference
        }
    }

    GET Ping
    -> Json<()>
    {
        use_auth BearerAuth(session)
    }
}

fn main() {}
