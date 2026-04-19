use concord_core::prelude::*;
use concord_macros::api;

struct NotCredential;

api! {
    client BadEndpointOutputApi {
        scheme: https,
        host: "example.com",
        auth {
            credential session: Endpoint(auth::LoginForSession)
        }
    }

    scope auth {
        POST LoginForSession
        -> Json<()> | NotCredential => {
        NotCredential
        }
        {
            path["login"]
        }
    }

    GET Health
    -> Json<()>
    {
    }
}

fn main() {}
