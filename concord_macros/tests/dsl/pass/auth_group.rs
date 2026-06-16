use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
}

api! {
    client AuthGroupApi {
        base "https://example.com"

        auth {
            secret token: String
            credential session = bearer(secret.token)
        }

        behavior protected {
            auth bearer session
        }

        default {
            behavior protected
        }
    }

    GET Me
    as me
    path ["me"]
    -> Json<User>
}

fn main() {}
