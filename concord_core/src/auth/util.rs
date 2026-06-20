pub(crate) fn hash_secret(value: &str) -> String {
    crate::redaction::secret_fingerprint(value).to_string()
}

pub(crate) fn hash_secret_parts(domain: &str, parts: &[&str]) -> String {
    let mut material = String::new();
    material.push_str(domain);
    material.push('\0');
    for part in parts {
        material.push_str(&part.len().to_string());
        material.push(':');
        material.push_str(part);
        material.push('\0');
    }
    hash_secret(&material)
}

pub(crate) fn hash_basic_credential(username: &str, password: &str) -> String {
    hash_secret_parts("concord.basic.v1", &[username, password])
}
