// AUTH-01: bearer/authorization/API-key placements type-check against the
// relevant credential capability; a custom provider whose material is not a
// `SecretCredential` cannot back a bearer placement.
use concord::auth::{
    AuthError, BoxFuture, CredentialMaterial, CredentialProvider, CredentialRequest,
};
use concord::{Binary, Bytes, api};

pub struct NonSecretMaterial;

impl CredentialMaterial for NonSecretMaterial {}

pub struct NonSecretProvider;

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
        base "https://example.test";
        credentials {
            credential session = custom crate::NonSecretProvider;
        }

        GET protected() -> Bytes via Binary {
            auth { bearer session; }
        }
    }
}

fn main() {}
