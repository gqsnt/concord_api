use concord_core::advanced::{
    AuthError, BoxFuture, CredentialMaterial, CredentialProvider, CredentialRequest,
};
use concord_macros::api;

#[derive(Clone)]
struct SessionToken;

impl CredentialMaterial for SessionToken {}

struct SessionProvider;

impl CredentialProvider for SessionProvider {
    type Material = SessionToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async { Ok(SessionToken) })
    }
}

api! {
    client Api {
        base "http://example.test";
        credentials {
            credential session = custom crate::SessionProvider;
        }
    }
}

fn main() {
    let client = Api::new(SessionProvider).unwrap();
    let _ = client.credentials().session().set(SessionToken);
}
