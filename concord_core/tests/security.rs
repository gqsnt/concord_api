#[path = "security/error_redaction.rs"]
mod error_redaction;
#[path = "security/hook_redaction.rs"]
mod hook_redaction;
#[path = "security/sanitized_metadata.rs"]
mod sanitized_metadata;

// The exhaustive name matrix remains colocated under `security/`, but is
// included by the private redaction module so it can exercise normalization
// helpers without widening the public API.
