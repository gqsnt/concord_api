//! Canonical hard limits for finite public request construction.

/// Maximum UTF-8 bytes Concord retains from one formatted public value.
pub const MAX_FORMATTED_PUBLIC_VALUE_BYTES: usize = 65_536;

/// Maximum top-level components retained by one runtime endpoint.
pub const MAX_RUNTIME_ENDPOINT_COMPONENTS: usize = 256;

/// Maximum aggregate public UTF-8 bytes retained by one runtime endpoint.
pub const MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES: usize = 65_536;

/// Maximum authentication placements retained by one effective plan.
pub const MAX_AUTH_PLACEMENTS: usize = 64;

/// Maximum UTF-8 bytes in one rate-limit rule identifier.
pub const MAX_RATE_LIMIT_ID_BYTES: usize = 256;

/// Maximum ordered name/value parts in one resolved rate-limit key.
pub const MAX_RATE_LIMIT_KEY_PARTS: usize = 32;

/// Maximum UTF-8 bytes in one rate-limit key-part name.
pub const MAX_RATE_LIMIT_KEY_PART_NAME_BYTES: usize = 128;

/// Maximum aggregate UTF-8 bytes in one resolved rate-limit key.
pub const MAX_RATE_LIMIT_KEY_BYTES: usize = 65_536;

/// Maximum windows retained by one rate-limit rule.
pub const MAX_RATE_LIMIT_WINDOWS_PER_RULE: usize = 16;

/// Maximum rules retained by one effective rate-limit plan.
pub const MAX_RATE_LIMIT_RULES_PER_PLAN: usize = 64;
