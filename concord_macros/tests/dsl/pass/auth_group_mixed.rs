use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
}

api! {
    client AuthGroupMixedApi {
        base "https://example.com"

        secret api_key: String

        auth {
            secret bearer_token: String
            credential api_key_credential = api_key(secret.api_key)
            credential bearer_session = bearer(secret.bearer_token)
        }

        behavior protected {
            auth bearer bearer_session
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
