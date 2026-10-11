// AUTH-01: an endpoint-backed credential's material is the source endpoint's
// declared output; a scheme-prefixed Authorization placement requires that
// material to be a `SecretCredential`.
use concord::auth::CredentialMaterial;
use concord::codec::{CodecError, DecodeBody, DecodeContext};
use concord::{Binary, Bytes, api};

pub struct NonSecretMaterial;

impl CredentialMaterial for NonSecretMaterial {}

pub struct NonSecretCodec;

impl DecodeBody for NonSecretCodec {
    type Output = NonSecretMaterial;

    const ACCEPT: Option<&'static str> = None;

    fn decode(_body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        Ok(NonSecretMaterial)
    }
}

api! {
    client Api {
        base "https://example.test";
        credentials {
            credential session = endpoint acquire;
        }

        POST acquire() -> crate::NonSecretMaterial via crate::NonSecretCodec;
        GET protected() -> Bytes via Binary {
            auth { authorization "Session" = session; }
        }
    }
}

fn main() {}
