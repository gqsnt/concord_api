// AUTH-01: an exact `authorization = credential` placement requires a
// `SecretCredential`; a custom provider's non-secret material is rejected by
// the placement's typed obligation.
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
            auth { authorization = session; }
        }
    }
}

fn main() {}
