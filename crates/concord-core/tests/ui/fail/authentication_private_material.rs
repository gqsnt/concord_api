use concord_core::advanced::{CredentialMaterial, CredentialRef};
use concord_core::prelude::{AccessToken, BasicCredential};

#[derive(Clone)]
struct Material;

impl CredentialMaterial for Material {}

fn inspect(reference: CredentialRef<Material>, token: AccessToken, basic: BasicCredential) {
    let _ = reference.selector;
    let _ = token.token;
    let _ = basic.username;
}

fn main() {}
