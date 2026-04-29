use concord_macros::api;

api! {
    client RecursiveApi {
        base https "example.com"
        credential session = endpoint auth_api::LoginForSession
    }

    scope auth_api {
        POST LoginForSession
            path ["login"]
            auth bearer session // ERROR: recursive dependency
        -> Json<()>
    }

    GET Me
        auth bearer session
    -> Json<()>
}

fn main() {}
