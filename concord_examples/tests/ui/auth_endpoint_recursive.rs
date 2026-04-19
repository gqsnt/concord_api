use concord_macros::api;

api! {
    client RecursiveApi {
        scheme: https,
        host: "example.com",
        auth {
            credential session: Endpoint(LoginForSession)
        }
    }

    POST LoginForSession
    -> Json<()>
    {
        path["login"]
        use_auth BearerAuth(session) // ERROR: recursive dependency
    }

    GET Me
    -> Json<()>
    {
        use_auth BearerAuth(session)
    }
}

fn main() {}
