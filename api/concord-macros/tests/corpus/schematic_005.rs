pub client Vault {
    base "https://api.vault.example";
    read Json;
    problem VaultProblem;
    status { 2xx { body } => Reply::into_result(body)? }
    …
}
