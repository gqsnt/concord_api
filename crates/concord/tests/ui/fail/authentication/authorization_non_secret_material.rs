use concord::api;
use concord::auth::{
    AuthError, BoxFuture, CredentialMaterial, CredentialProvider, CredentialRequest,
};

struct NonSecretMaterial;

impl CredentialMaterial for NonSecretMaterial {}

struct NonSecretProvider;

impl CredentialProvider for NonSecretProvider {
    type Material = NonSecretMaterial;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async { Ok(NonSecretMaterial) })
    }
}

api! {
    client Api {
        timeout off;
        base "https://example.test";
        credentials {
            credential session = custom crate::NonSecretProvider;
        }


    GET protected() -> Bytes using Bytes {
        auth { authorization = session; }
    }
}}

fn main() {}
