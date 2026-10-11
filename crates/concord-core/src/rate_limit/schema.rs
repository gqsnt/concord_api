//! Value-free rate-limit schema descriptors and limiter capabilities.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use url::{Host, Url};

use super::canonical::is_bounded_ascii_graphic;
use super::{RateLimitError, RateLimitId, RateLimitKey, RateLimitPlan, RateLimitWindow};
use crate::error::RateLimitErrorKind;
use crate::limits::MAX_RATE_LIMITER_NAMESPACE_BYTES;

/// Version of Concord's canonical rate-key framing and built-in dimension
/// normalization carried in every [`RatePolicySchema`].
///
/// Version 1 is byte-identical to Concord 2.1: each value record is
/// `u8(tag.len) || tag || u64_be(bytes.len) || bytes`; built-in scalar tags
/// and big-endian payloads are unchanged; the `origin` dimension is the UTF-8
/// text `scheme://host:effective_port` (ASCII-lowercase domain, canonical IPv4
/// text, bracketed IPv6, always an explicit port) and the `endpoint`
/// dimension is the UTF-8 operational endpoint ID, both under the `utf8` tag.
pub const RATE_KEY_FRAMING_VERSION: u32 = 1;

/// Reserved label of the built-in normalized logical-origin dimension.
pub const ORIGIN_LABEL: &str = "origin";

/// Reserved label of the built-in operational endpoint-ID dimension.
pub const ENDPOINT_LABEL: &str = "endpoint";

/// Default finite number of active schema registrations retained by a
/// Concord-provided shared or mock shared limiter service.
pub const DEFAULT_RATE_SCHEMA_REGISTRATIONS: usize = 4_096;

/// The source of one rate-key dimension.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RateDimensionKind {
    /// The built-in normalized logical origin, labelled [`ORIGIN_LABEL`].
    Origin,
    /// The built-in operational endpoint ID, labelled [`ENDPOINT_LABEL`].
    Endpoint,
    /// An explicitly labelled declaration value.
    Custom,
}

/// Value-free schema of one ordered rate-key dimension.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RateDimensionSchema {
    label: Box<str>,
    kind: RateDimensionKind,
    encoding_id: &'static str,
}

impl RateDimensionSchema {
    pub(crate) fn new(label: &str, kind: RateDimensionKind, encoding_id: &'static str) -> Self {
        Self {
            label: label.into(),
            kind,
            encoding_id,
        }
    }

    /// Returns the explicit logical label. Labels are state-affecting.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns whether this is a built-in or custom dimension.
    #[must_use]
    pub const fn kind(&self) -> RateDimensionKind {
        self.kind
    }

    /// Returns the value's encoding contract name.
    #[must_use]
    pub const fn encoding_id(&self) -> &'static str {
        self.encoding_id
    }
}

/// One window in canonical schema order.
///
/// Ordering is by duration, then capacity, which is the canonical order used
/// for schema comparison.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RateWindowSchema {
    duration: Duration,
    capacity: NonZeroU32,
}

impl RateWindowSchema {
    /// Returns the positive refill duration.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns the positive capacity.
    #[must_use]
    pub const fn capacity(&self) -> NonZeroU32 {
        self.capacity
    }
}

/// Bounded value-free schema of one resolved rate-limit policy.
///
/// The schema identifies the policy ID, the ordered dimension labels, kinds
/// and encoding contracts, the canonical framing version, and the canonical
/// window schema. It never contains key values and is safe to display,
/// compare and store. Cost is deliberately absent: it is not part of bucket
/// identity. The limiter namespace and algorithm identity of the complete
/// schema contract are carried beside it as [`LimiterCapability`] in
/// [`crate::rate_limit::RateLimitRequest::capability`], because one request
/// is admitted by exactly one limiter.
///
/// Two schemas are equal exactly when a shared persistent adapter must treat
/// their state as the same contract (RATE-05). Source window order is
/// provenance only; see [`crate::rate_limit::RateLimitPolicy::windows`].
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RatePolicySchema {
    id: RateLimitId,
    dimensions: Box<[RateDimensionSchema]>,
    windows: Box<[RateWindowSchema]>,
    framing_version: u32,
}

impl RatePolicySchema {
    pub(crate) fn from_parts(
        id: &RateLimitId,
        key: &RateLimitKey,
        windows: &[RateLimitWindow],
    ) -> Self {
        let dimensions = key
            .parts()
            .iter()
            .map(|part| RateDimensionSchema::new(part.name(), part.kind(), part.encoding_id()))
            .collect::<Vec<_>>();
        let mut canonical = windows
            .iter()
            .map(|window| RateWindowSchema {
                duration: window.duration(),
                capacity: window.capacity(),
            })
            .collect::<Vec<_>>();
        canonical.sort_unstable();
        Self {
            id: id.clone(),
            dimensions: dimensions.into_boxed_slice(),
            windows: canonical.into_boxed_slice(),
            framing_version: RATE_KEY_FRAMING_VERSION,
        }
    }

    /// Returns the explicit policy identifier.
    #[must_use]
    pub const fn id(&self) -> &RateLimitId {
        &self.id
    }

    /// Returns the ordered dimension schema.
    #[must_use]
    pub const fn dimensions(&self) -> &[RateDimensionSchema] {
        &self.dimensions
    }

    /// Returns the windows in canonical (duration, capacity) order.
    #[must_use]
    pub const fn windows(&self) -> &[RateWindowSchema] {
        &self.windows
    }

    /// Returns the canonical framing version, [`RATE_KEY_FRAMING_VERSION`].
    #[must_use]
    pub const fn framing_version(&self) -> u32 {
        self.framing_version
    }

    /// Returns whether the key contains the built-in origin dimension.
    #[must_use]
    pub fn uses_origin(&self) -> bool {
        self.dimensions
            .iter()
            .any(|dimension| dimension.kind == RateDimensionKind::Origin)
    }
}

/// How an application-selected limiter enforces declared policies.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Enforcement {
    /// State belongs to one limiter object in one process.
    Local,
    /// The application explicitly selected no enforcement (for example
    /// [`crate::rate_limit::NoopRateLimiter`]).
    Unenforced,
    /// State is shared and persistent across processes. The adapter must
    /// atomically register and compare each [`RatePolicySchema`] inside
    /// `acquire` before any state use and fail closed on a mismatch.
    SharedPersistent,
}

impl Enforcement {
    /// Returns the safe plan label for a nonempty set of declared policies.
    #[must_use]
    pub const fn plan_label(self) -> &'static str {
        match self {
            Self::Local => {
                "declared policies; enforced by the application-selected process-local limiter"
            }
            Self::Unenforced => {
                "declared policies; enforcement explicitly disabled by application selection"
            }
            Self::SharedPersistent => {
                "declared policies; enforced by the application-selected shared persistent limiter"
            }
        }
    }
}

/// Structural, value-free descriptor of a limiter's enforcement contract.
///
/// Core snapshots it once when the limiter is selected on the runtime
/// builder and validates it at client build; later changes in what the
/// limiter would report are not observed. It identifies the enforcement
/// kind, a stable adapter/storage namespace and an algorithm version; for
/// [`Enforcement::SharedPersistent`], namespace and algorithm version are part
/// of the cross-version continuity contract (RATE-05).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LimiterCapability {
    enforcement: Enforcement,
    namespace: &'static str,
    algorithm_version: u32,
}

impl LimiterCapability {
    /// Creates a capability descriptor.
    ///
    /// The namespace must be nonempty ASCII graphic text of at most
    /// [`crate::rate_limit::MAX_RATE_LIMITER_NAMESPACE_BYTES`] bytes; an
    /// invalid descriptor fails client construction with a runtime
    /// configuration error.
    #[must_use]
    pub const fn new(
        enforcement: Enforcement,
        namespace: &'static str,
        algorithm_version: u32,
    ) -> Self {
        Self {
            enforcement,
            namespace,
            algorithm_version,
        }
    }

    /// Returns the enforcement kind.
    #[must_use]
    pub const fn enforcement(&self) -> Enforcement {
        self.enforcement
    }

    /// Returns the stable adapter/storage namespace.
    #[must_use]
    pub const fn namespace(&self) -> &'static str {
        self.namespace
    }

    /// Returns the limiter algorithm interpretation version.
    #[must_use]
    pub const fn algorithm_version(&self) -> u32 {
        self.algorithm_version
    }

    /// Validates the bounded structural namespace.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::InvalidConfiguration`] for an empty,
    /// non-ASCII-graphic or overlong namespace.
    pub fn validate(&self) -> Result<(), RateLimitError> {
        if is_bounded_ascii_graphic(self.namespace, MAX_RATE_LIMITER_NAMESPACE_BYTES) {
            Ok(())
        } else {
            Err(RateLimitError::new(
                RateLimitErrorKind::InvalidConfiguration,
            ))
        }
    }
}

/// Value-free summary of one policy in a bound plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatePolicySummary {
    /// Shared with the resolved policy it summarizes (V3-O07 attribution: a
    /// deep copy cost four allocations per rate-limited plan).
    schema: Arc<RatePolicySchema>,
    cost: NonZeroU32,
}

impl RatePolicySummary {
    /// Returns the value-free policy schema.
    #[must_use]
    pub fn schema(&self) -> &RatePolicySchema {
        &self.schema
    }

    /// Returns the per-attempt cost charged against every window.
    #[must_use]
    pub const fn cost(&self) -> NonZeroU32 {
        self.cost
    }
}

/// Safe structural rate-limit facts of one bound request plan.
///
/// The summary contains every effective policy ID, ordered key-label schema,
/// encoding contracts, canonical windows and cost, plus the actual limiter
/// enforcement choice snapshotted at client build. It contains no key value,
/// no origin text and no limiter state; creating it performs no limiter call
/// or schema registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatePlanSummary {
    policies: Box<[RatePolicySummary]>,
    limiter: Option<LimiterCapability>,
}

impl RatePlanSummary {
    pub(crate) fn new(plan: &RateLimitPlan, limiter: Option<LimiterCapability>) -> Self {
        Self {
            policies: plan
                .policies()
                .iter()
                .map(|policy| RatePolicySummary {
                    schema: Arc::clone(policy.shared_schema()),
                    cost: policy.cost(),
                })
                .collect(),
            limiter,
        }
    }

    /// Returns the effective policies in declaration order.
    #[must_use]
    pub const fn policies(&self) -> &[RatePolicySummary] {
        &self.policies
    }

    /// Returns the number of effective policies.
    #[must_use]
    pub const fn policy_count(&self) -> usize {
        self.policies.len()
    }

    /// Returns the selected limiter's capability, or `None` when the
    /// application selected no limiter.
    #[must_use]
    pub const fn limiter(&self) -> Option<LimiterCapability> {
        self.limiter
    }

    /// Returns the actual enforcement choice for these policies.
    ///
    /// `None` means either no policy applies or no limiter was selected; in
    /// the latter case execution fails closed before dispatch.
    #[must_use]
    pub fn enforcement(&self) -> Option<Enforcement> {
        if self.policies.is_empty() {
            None
        } else {
            self.limiter.map(|limiter| limiter.enforcement())
        }
    }

    /// Returns a safe human label for the enforcement choice.
    ///
    /// An explicit no-op limiter is labelled
    /// "declared policies; enforcement explicitly disabled by application
    /// selection", never as rate limited.
    #[must_use]
    pub fn enforcement_label(&self) -> &'static str {
        if self.policies.is_empty() {
            return "no declared rate-limit policies";
        }
        match self.limiter {
            Some(limiter) => limiter.enforcement().plan_label(),
            None => "declared policies; no limiter selected, so admission fails closed",
        }
    }
}

/// Returns the canonical `scheme://host:effective_port` origin payload.
///
/// Domain hosts are ASCII-lowercased, IPv4 uses canonical dotted text, IPv6
/// is bracketed, and the port is always explicit (including 80/443).
pub(crate) fn normalized_origin(url: &Url) -> Option<String> {
    use std::fmt::Write as _;

    let host = url.host()?;
    let port = url.port_or_known_default()?;
    let scheme = url.scheme();
    // One exactly sized buffer instead of separate lowercase copies and a
    // growing `format!` (V3-O07 attribution): scheme, "://", the serialized
    // host (IPv6 already bracketed), ':' and at most five port digits.
    let mut origin =
        String::with_capacity(scheme.len() + 3 + url.host_str().map_or(0, str::len) + 1 + 5);
    origin.push_str(scheme);
    origin.make_ascii_lowercase();
    origin.push_str("://");
    match host {
        Host::Domain(domain) => {
            let start = origin.len();
            origin.push_str(domain);
            origin[start..].make_ascii_lowercase();
        }
        Host::Ipv4(address) => write!(origin, "{address}").ok()?,
        Host::Ipv6(address) => write!(origin, "[{address}]").ok()?,
    }
    write!(origin, ":{port}").ok()?;
    Some(origin)
}
