//! Canonical hard limits for finite public request construction.

/// Maximum UTF-8 bytes Concord retains from one formatted public value.
pub const MAX_FORMATTED_PUBLIC_VALUE_BYTES: usize = 65_536;

/// Maximum retained components in one final managed request head.
pub const MAX_REQUEST_HEAD_COMPONENTS: usize = 256;

/// Maximum serialized bytes in each logical or credential-sanitized request URL.
pub const MAX_REQUEST_URL_BYTES: usize = 131_072;

/// Maximum public headers retained before private authentication.
pub const MAX_REQUEST_HEADER_COUNT: usize = 256;

/// Maximum aggregate normalized-name and value bytes in public request headers.
pub const MAX_REQUEST_HEADER_BYTES: usize = 65_536;

/// Maximum top-level components retained by one runtime endpoint.
pub const MAX_RUNTIME_ENDPOINT_COMPONENTS: usize = 256;

/// Maximum byte length of one runtime endpoint HTTP method token.
pub const MAX_RUNTIME_ENDPOINT_METHOD_BYTES: usize = 128;

/// Maximum aggregate public UTF-8 bytes retained by one runtime endpoint.
pub const MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES: usize = 65_536;

/// Maximum authentication placements retained by one effective plan.
pub const MAX_AUTH_PLACEMENTS: usize = 64;

/// Maximum UTF-8 bytes in one rate-limit policy identifier.
pub const MAX_RATE_LIMIT_ID_BYTES: usize = 256;

/// Maximum ordered name/value parts in one resolved rate-limit key.
pub const MAX_RATE_LIMIT_KEY_PARTS: usize = 32;

/// Maximum UTF-8 bytes in one rate-limit key-part name.
pub const MAX_RATE_LIMIT_KEY_PART_NAME_BYTES: usize = 128;

/// Maximum aggregate UTF-8 bytes in one resolved rate-limit key.
pub const MAX_RATE_LIMIT_KEY_BYTES: usize = 65_536;

/// Maximum empty transport chunks accepted while acquiring one response body.
pub const MAX_EMPTY_RESPONSE_CHUNKS: usize = 1_024;

/// Maximum immediately ready empty response chunks handled in one task turn.
pub(crate) const MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN: usize = 32;

/// Maximum empty chunks accepted from one managed request body before transport.
pub const MAX_EMPTY_REQUEST_CHUNKS: usize = 1_024;

/// Maximum immediately ready empty request chunks handled by one task poll.
pub(crate) const MAX_EMPTY_REQUEST_CHUNKS_PER_POLL: usize = 32;

/// Maximum windows retained by one rate-limit policy.
pub const MAX_RATE_LIMIT_WINDOWS_PER_POLICY: usize = 16;

/// Maximum policies retained by one effective rate-limit plan.
pub const MAX_RATE_LIMIT_POLICIES_PER_PLAN: usize = 64;
