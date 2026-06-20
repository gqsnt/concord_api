use concord_macros::api;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CertificateResponse {
    identity_id: String,
}

#[derive(Debug, Deserialize)]
pub struct User;

api! {
    client EndpointCertificateAsBasicApi {
        base "https://example.com"
        credential cert = endpoint auth_api::GetCertificate
    }

    scope auth_api {
        GET GetCertificate
            path ["cert"]
            -> Json<CertificateResponse>
            map ClientCertificate {
                ClientCertificate::new(r.identity_id)
            }
    }

    scope protected {
        auth basic cert

        GET Me
            path ["me"]
            -> Json<User>
    }
}

fn main() {}
