// AUTH-01 / §11.1: every credential source kind (api_key, bearer, endpoint,
// custom provider) placed through the exact and literal-scheme `authorization`
// placements; literal schemes are validated HTTP tokens (case preserved).
use concord::api;
use concord::auth::{
    AuthError, BoxFuture, CredentialMaterial, CredentialProvider, CredentialRequest,
    SecretCredential, SecretString,
};
use concord::codec::{CodecError, DecodeBody, DecodeContext};
use concord::{Binary, Bytes};

pub struct Session(SecretString);

impl CredentialMaterial for Session {}

impl SecretCredential for Session {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

pub struct SessionCodec;

impl DecodeBody for SessionCodec {
    type Output = Session;

    const ACCEPT: Option<&'static str> = None;

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let value = String::from_utf8(body.to_vec())
            .map_err(|_| CodecError::new(concord::codec::CodecErrorKind::Decode))?;
        Ok(Session(
            SecretString::try_new(value).map_err(|_| {
                CodecError::new(concord::codec::CodecErrorKind::Decode)
            })?,
        ))
    }
}

pub struct Provider;

impl CredentialProvider for Provider {
    type Material = Session;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async {
            Ok(Session(
                SecretString::try_new("provider").expect("bounded test material"),
            ))
        })
    }
}

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        credentials {
            secret key;
            secret token;
            credential api_key = api_key(secret.key);
            credential bearer = bearer(secret.token);
            credential endpoint = endpoint acquire;
            credential provider = custom crate::Provider;
        }

        GET acquire() -> crate::Session via crate::SessionCodec;
        GET exact_key() -> Bytes via Binary {
            auth { authorization = api_key; }
        }
        GET exact_bearer() -> Bytes via Binary {
            auth { authorization = bearer; }
        }
        GET scheme_endpoint() -> Bytes via Binary {
            auth { authorization "Session" = endpoint; }
        }
        GET scheme_provider() -> Bytes via Binary {
            auth { authorization "tOkEn" = provider; }
        }
    }
}

fn main() {}
