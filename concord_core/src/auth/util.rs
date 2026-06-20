pub(crate) fn hash_secret(value: &str) -> String {
    crate::redaction::secret_fingerprint(value).to_string()
}
