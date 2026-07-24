use concord_core::contracts::{
    AccessToken, BasicCredential, CredentialMaterial, CredentialRef,
};

#[derive(Clone)]
struct Material;

impl CredentialMaterial for Material {}

fn inspect(reference: CredentialRef<Material>, token: AccessToken, basic: BasicCredential) {
    let _ = reference.selector;
    let _ = token.token;
    let _ = basic.username;
}

fn main() {}
