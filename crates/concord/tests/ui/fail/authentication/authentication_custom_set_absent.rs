// §11.3: the credential facade offers exactly `clear_<name>()` and
// `replace_<name>(material)`; there is no ambiguous `set` alias, and a
// custom provider is supplied to the builder under the credential's name.
use concord::auth::{
    AuthError, BoxFuture, CredentialMaterial, CredentialProvider, CredentialRequest,
    SecretCredential,
};
use concord::{Binary, Bytes, SecretString, api};

pub struct SessionToken(SecretString);

impl CredentialMaterial for SessionToken {}

impl SecretCredential for SessionToken {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

pub struct SessionProvider;

impl CredentialProvider for SessionProvider {
    type Material = SessionToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async { Ok(SessionToken(SecretString::try_new("provided").expect("secret"))) })
    }
}

api! {
    client Api {
        base "https://example.test";
        credentials {
            credential session = custom crate::SessionProvider;
        }

        GET read() -> Bytes via Binary {
            auth { bearer session; }
        }
    }
}

fn main() {
    let client = Api::builder()
        .session(SessionProvider)
        .build()
        .expect("client");
    let token = SessionToken(SecretString::try_new("manual").expect("secret"));
    let _ = client.credentials().set_session(token);
}
