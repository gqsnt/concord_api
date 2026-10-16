//! Stable rate-limit contracts and private execution support.
//!
//! One local bucket is identified by a selected limiter object, policy ID,
//! ordered `(label, canonical value bytes)` key, and window (RATE-03). Cost is
//! charged against every window and is not part of bucket identity. Keys are
//! explicitly labelled; values are canonically encoded through
//! [`RateKeyPart`] / [`RateKeyEncoder`] using canonical byte framing, while
//! each contract's `ENCODING_ID` is carried separately as
//! value-free schema metadata ([`RatePolicySchema`]).
//!
//! Resolved key values may be operationally sensitive: plans, keys and
//! declarations intentionally have no `Debug` or `Display`, and core does not
//! copy their values into lifecycle errors, user hook events, plans, or
//! completion events. Bound request plans expose only
//! [`RatePlanSummary`]: policy IDs, label/encoding schemas, windows, cost and
//! the actual enforcement choice.
//!
//! Generated code and runtime endpoints attach policies through
//! [`RateLimitDeclaration`]; core resolves the built-in `origin` and
//! `endpoint` dimensions from the request head.

mod canonical;
mod declaration;
mod error;
mod limiter;
mod schema;
mod types;

pub use crate::limits::{
    MAX_RATE_KEY_ENCODING_ID_BYTES, MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_KEY_BYTES,
    MAX_RATE_LIMIT_KEY_PART_NAME_BYTES, MAX_RATE_LIMIT_KEY_PARTS, MAX_RATE_LIMIT_POLICIES_PER_PLAN,
    MAX_RATE_LIMIT_WINDOWS_PER_POLICY, MAX_RATE_LIMITER_NAMESPACE_BYTES,
};
pub use canonical::{
    CanonicalKeyWriter, EncodedKeyPart, RateKeyEncoder, RateKeyPart, UTF8_ENCODING_ID,
    encode_canonical, encode_canonical_with, is_valid_encoding_id,
};
pub use declaration::RateLimitDeclaration;
pub use error::RateLimitError;
pub use limiter::{
    CooldownOrigin, DEFAULT_RATE_LIMIT_KEY_BYTES, NoopRateLimiter, RateLimitRequest,
    RateLimitResponse, RateLimiter,
};
pub use schema::{
    DEFAULT_RATE_SCHEMA_REGISTRATIONS, ENDPOINT_LABEL, Enforcement, LimiterCapability,
    ORIGIN_LABEL, RATE_KEY_FRAMING_VERSION, RateDimensionKind, RateDimensionSchema,
    RatePlanSummary, RatePolicySchema, RatePolicySummary, RateWindowSchema,
};
pub use types::{
    RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitPermit, RateLimitPlan, RateLimitPolicy,
    RateLimitWindow,
};

pub(crate) use declaration::push_declaration;
#[cfg(test)]
pub(crate) use limiter::FallbackRateLimiter;
pub(crate) use limiter::fallback_limiter;
pub(crate) use schema::normalized_origin;

// The captured V3 key bytes use an `https` logical origin, so these
// fixtures need the TLS-capable build.
#[cfg(all(test, feature = "default-tls"))]
mod plan_tests;
#[cfg(test)]
mod tests;
