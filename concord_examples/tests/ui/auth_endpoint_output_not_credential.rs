use concord_core::prelude::*;
use concord_macros::api;

struct NotCredential;

api! {
    client BadEndpointOutputApi {
        scheme: https,
        host: "example.com",
        auth {
            credential session: Endpoint(LoginForSession)
        }
    }

    POST LoginForSession {
        path["login"]
        -> Json<()> | NotCredential => {
            NotCredential
        };
    }

    GET Health {
        -> Json<()>;
    }
}

fn main() {}
