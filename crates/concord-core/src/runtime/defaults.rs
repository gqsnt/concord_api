//! Normative finite defaults for one immutable client runtime snapshot.
//!
//! Keep exact numeric defaults here. The documentation checker links against
//! the values reexported through the hidden integration surface.

/// Default aggregate request-body byte ceiling.
pub const DEFAULT_REQUEST_BODY_BYTES: usize = 16_777_216;
/// Default buffered accepted-response byte ceiling.
pub const DEFAULT_RESPONSE_BODY_BYTES: usize = 16_777_216;
/// Default cumulative streaming-response byte ceiling.
pub const DEFAULT_STREAM_RESPONSE_BYTES: usize = 16_777_216;
/// Default captured terminal-status-body byte ceiling.
pub const DEFAULT_STATUS_BODY_BYTES: usize = 65_536;
/// Default credential-provider request-body byte ceiling.
pub const DEFAULT_PROVIDER_REQUEST_BYTES: usize = 1_048_576;
/// Default credential-provider response-body byte ceiling.
pub const DEFAULT_PROVIDER_RESPONSE_BYTES: usize = 1_048_576;
/// Default maximum page count for pagination.
pub const DEFAULT_PAGINATION_MAX_PAGES: u32 = 1_000;
/// Default maximum accumulated item count for pagination.
pub const DEFAULT_PAGINATION_MAX_ITEMS: usize = 100_000;
/// Default byte ceiling for retained canonical pagination-identity vectors.
pub const DEFAULT_PAGINATION_IDENTITY_BYTES: usize = 4_194_304;
/// Default authentication-failure cooldown cap in seconds.
pub const DEFAULT_AUTH_FAILURE_COOLDOWN_SECONDS: u64 = 60;
/// Default server-directed rate-limit cooldown cap in seconds.
pub const DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS: u64 = 60;
/// Default maximum aggregate byte length of one resolved rate-limit key.
pub const DEFAULT_RATE_LIMIT_KEY_BYTES: usize = 1_024;
