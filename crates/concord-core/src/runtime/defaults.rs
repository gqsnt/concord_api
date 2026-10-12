//! Normative finite defaults for one immutable client runtime snapshot.
//!
//! Keep exact numeric defaults here. The documentation checker links against
//! the values reexported through the hidden integration surface.

/// Default finite operation deadline inherited by runtime endpoints, in seconds.
pub const DEFAULT_OPERATION_TIMEOUT_SECONDS: u64 = 30;
/// Default aggregate request-body byte ceiling.
pub const DEFAULT_REQUEST_BODY_BYTES: usize = 16_777_216;
/// Default buffered accepted-response byte ceiling.
pub const DEFAULT_RESPONSE_BODY_BYTES: usize = 16_777_216;
/// Default cumulative streaming-response byte ceiling.
pub const DEFAULT_STREAM_RESPONSE_BYTES: usize = 16_777_216;
/// Default total elapsed lease of one managed stream after response-head
/// acquisition, in seconds. Consumer pauses between polls count.
pub const DEFAULT_STREAM_LIFETIME_SECONDS: u64 = 300;
/// Default cumulative idle time of one managed stream while an underlying
/// read is pending, in seconds.
pub const DEFAULT_STREAM_IDLE_SECONDS: u64 = 30;
/// Default maximum records yielded by one managed item stream.
pub const DEFAULT_STREAM_MAX_RECORDS: usize = 100_000;
/// Default maximum staged bytes of one framed stream record, including delimiters.
/// Allows a 1 MiB NDJSON JSON payload and its two-byte CRLF terminator;
/// each decoder retains its own independent payload/framing bound.
pub const DEFAULT_STREAM_FRAME_BYTES: usize = 1_048_578;
/// Default captured terminal-status-body byte ceiling.
pub const DEFAULT_STATUS_BODY_BYTES: usize = 65_536;
/// Default credential-provider request-body byte ceiling.
pub const DEFAULT_PROVIDER_REQUEST_BYTES: usize = 1_048_576;
/// Default credential-provider response-body byte ceiling.
pub const DEFAULT_PROVIDER_RESPONSE_BYTES: usize = 1_048_576;
/// Default provider-helper sends per credential acquisition or refresh.
pub const DEFAULT_PROVIDER_MAX_SENDS: u32 = 4;
/// Default capacity of one provider's declared helper origin set.
pub const DEFAULT_PROVIDER_MAX_ORIGINS: usize = 8;
/// Default provider-helper ceiling per acquisition, in seconds; the effective
/// helper deadline is `min(remaining operation deadline, this ceiling)`.
pub const DEFAULT_PROVIDER_TIMEOUT_SECONDS: u64 = 30;
/// Default maximum page count for pagination.
pub const DEFAULT_PAGINATION_MAX_PAGES: u32 = 1_000;
/// Default maximum accumulated item count for pagination.
pub const DEFAULT_PAGINATION_MAX_ITEMS: usize = 100_000;
/// Default aggregate accepted buffered-response bytes charged across pagination pages.
pub const DEFAULT_PAGINATION_TOTAL_RESPONSE_BYTES: usize = 67_108_864;
/// Default byte ceiling for retained canonical pagination-identity vectors.
pub const DEFAULT_PAGINATION_IDENTITY_BYTES: usize = 4_194_304;
/// Default authentication-failure cooldown cap in seconds.
pub const DEFAULT_AUTH_FAILURE_COOLDOWN_SECONDS: u64 = 60;
/// Default server-directed rate-limit cooldown cap in seconds.
pub const DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS: u64 = 60;
/// Default maximum aggregate byte length of one resolved rate-limit key.
pub const DEFAULT_RATE_LIMIT_KEY_BYTES: usize = 1_024;
