use concord::auth::{
    AuthError, BoxFuture, CredentialMaterial, CredentialProvider, CredentialRequest,
};
use concord::api;

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
        timeout 30s;
        base "http://example.test";
        credentials {
            credential session = custom crate::SessionProvider;
        }
    }
}

fn main() {
    let client = Api::builder()
        .session_provider(SessionProvider)
        .build()
        .unwrap();
    let _ = client.credentials().session().set(SessionToken);
}
