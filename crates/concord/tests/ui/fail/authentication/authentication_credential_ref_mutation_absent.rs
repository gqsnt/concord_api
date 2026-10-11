// §11.3: credential state changes only through the explicit local facade
// operations `clear_<name>()` / `replace_<name>(material)` (and
// `store_as_<name>()` on the source endpoint). A placement's read-only
// `CredentialRef` exposes no mutation, and the generated facade exposes no
// per-credential handle that could.
use concord::auth::{AccessToken, CredentialRef};
use concord::{Binary, Bytes, SecretString, api};

api! {
    client Api {
        base "https://example.test";
        credentials {
            secret token;
            credential session = bearer(secret.token);
        }

        GET read() -> Bytes via Binary {
            auth { bearer session; }
        }
    }
}

fn clear_through_reference(credential: CredentialRef<AccessToken>) {
    let _ = credential.clear();
}

fn main() {
    let client = Api::builder()
        .token(SecretString::try_new("protected").expect("bounded protected input"))
        .build()
        .expect("client");
    let _ = client.credentials().session();
    let _ = clear_through_reference;
}
