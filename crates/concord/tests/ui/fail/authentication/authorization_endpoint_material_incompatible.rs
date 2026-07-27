use concord::api;
use concord::auth::CredentialMaterial;
use concord::codec::{CodecError, DecodeBody, DecodeContext};
use concord::Bytes;

struct NonSecretMaterial;

impl CredentialMaterial for NonSecretMaterial {}

struct NonSecretCodec;

impl DecodeBody for NonSecretCodec {
    type Output = NonSecretMaterial;

    const ACCEPT: Option<&'static str> = None;

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Ok(())
    }

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
    }

    GET acquire() -> crate::NonSecretCodec;
    GET protected() -> Bytes {
        auth authorization "Session" = session;
    }
}

fn main() {}
