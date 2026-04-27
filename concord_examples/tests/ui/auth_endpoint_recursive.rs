use concord_macros::api;

api! {
    client RecursiveApi {
        base https "example.com"
        credential session = endpoint auth_api::LoginForSession
    }

    scope auth_api {
        POST LoginForSession
        -> Json<()>
        {
            path ["login"]
            auth bearer session // ERROR: recursive dependency
        }
    }

    GET Me
    -> Json<()>
    {
        auth bearer session
    }
}

fn main() {}
