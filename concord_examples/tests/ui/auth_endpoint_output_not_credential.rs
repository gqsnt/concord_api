use concord_core::prelude::*;
use concord_macros::api;

struct NotCredential;

api! {
    client BadEndpointOutputApi {
        base https "example.com"
        credential session = endpoint auth::LoginForSession
    }

    scope auth {
        POST LoginForSession
        -> Json<()>
                map NotCredential {
        NotCredential
        }
            {
            path ["login"]
        }
    }

    GET Health
    -> Json<()>
    {
    }
}

fn main() {}
