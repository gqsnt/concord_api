concord::api! {
pub client Vault(origin: Base, key: Secret) {
        base origin;
        middleware Proof::new(origin.clone(), key.clone());
        GET balance() -> String { path "balance"; read concord::Text; }
    }
}
