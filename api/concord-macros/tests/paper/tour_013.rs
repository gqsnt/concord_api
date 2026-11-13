pub client Vault(key: Secret) {
    middleware Signed::new(key.clone());      // every request is signed as it leaves
    // base, read, endpoints …
}
