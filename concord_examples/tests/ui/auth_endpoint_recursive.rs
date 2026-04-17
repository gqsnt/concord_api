use concord_macros::api;

api! {
    client RecursiveApi {
        scheme: https,
        host: "example.com",
        auth {
            credential session: Endpoint(LoginForSession)
        }
    }

    POST LoginForSession {
        path["login"]
        use_auth BearerAuth(session) // ERROR: recursive dependency
        -> Json<()>;
    }

    GET Me {
        use_auth BearerAuth(session)
        -> Json<()>;
    }
}

fn main() {}
