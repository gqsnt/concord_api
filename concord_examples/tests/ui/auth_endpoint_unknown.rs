use concord_macros::api;

api! {
    client UiAuthEndpointUnknown {
        scheme: https,
        host: "example.com",
        auth {
            credential session: Endpoint(LoginMissing) // ERROR: unknown endpoint reference
        }
    }

    GET Ping {
        use_auth BearerAuth(session)
        -> Json<()>;
    }
}

fn main() {}
