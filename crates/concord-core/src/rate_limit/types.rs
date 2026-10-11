use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use crate::error::RateLimitErrorKind;
use crate::limits::{
    MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_KEY_BYTES, MAX_RATE_LIMIT_KEY_PART_NAME_BYTES,
    MAX_RATE_LIMIT_KEY_PARTS, MAX_RATE_LIMIT_POLICIES_PER_PLAN, MAX_RATE_LIMIT_WINDOWS_PER_POLICY,
};

use super::canonical::{EncodedKeyPart, encode_canonical};
use super::schema::{ENDPOINT_LABEL, ORIGIN_LABEL, RateDimensionKind, RatePolicySchema};
use super::{RateKeyPart, RateLimitError};

/// Bytes accounted for one key part's structural framing beyond its label and
/// canonical value (a 32-bit label length and a 64-bit value length).
const KEY_PART_FRAMING_BYTES: usize = 4 + 8;

/// A stable, explicit logical identifier for one rate-limit policy.
///
/// IDs are nonempty UTF-8 of at most [`MAX_RATE_LIMIT_ID_BYTES`] bytes. They
/// are static contract facts rather than protected key values, so they may be
/// displayed in plans and schema descriptors.
#[derive(Clone)]
pub struct RateLimitId {
    /// Static for declarations generated from source (their ID is a string
    /// literal), shared otherwise, so that the copies held by a resolved
    /// policy, its schema descriptor and a plan summary never allocate and,
    /// for generated policies, never touch a reference count (V3-O07
    /// attribution). Equality, ordering, hashing and `Debug` use the text.
    value: RateLimitIdText,
}

#[derive(Clone)]
enum RateLimitIdText {
    Static(&'static str),
    Shared(Arc<str>),
}

impl RateLimitId {
    /// Creates a nonempty policy identifier no larger than
    /// [`MAX_RATE_LIMIT_ID_BYTES`].
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::InvalidConfiguration`] for an empty or
    /// overlong identifier; the error never contains the rejected text.
    pub fn new(value: impl AsRef<str>) -> Result<Self, RateLimitError> {
        let value = value.as_ref();
        validate_id(value)?;
        Ok(Self {
            value: RateLimitIdText::Shared(value.into()),
        })
    }

    /// Creates the identifier of a declaration generated from source, whose
    /// ID is a string literal, without copying it. Same rules as
    /// [`Self::new`].
    pub(crate) fn from_static(value: &'static str) -> Result<Self, RateLimitError> {
        validate_id(value)?;
        Ok(Self {
            value: RateLimitIdText::Static(value),
        })
    }

    /// Returns the policy identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match &self.value {
            RateLimitIdText::Static(value) => value,
            RateLimitIdText::Shared(value) => value,
        }
    }

    #[cfg(test)]
    pub(crate) const fn is_static(&self) -> bool {
        matches!(self.value, RateLimitIdText::Static(_))
    }
}

fn validate_id(value: &str) -> Result<(), RateLimitError> {
    if value.is_empty() || value.len() > MAX_RATE_LIMIT_ID_BYTES {
        return Err(invalid_configuration());
    }
    Ok(())
}

impl std::fmt::Debug for RateLimitId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RateLimitId")
            .field("value", &self.as_str())
            .finish()
    }
}

impl PartialEq for RateLimitId {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for RateLimitId {}

impl PartialOrd for RateLimitId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RateLimitId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl std::hash::Hash for RateLimitId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

/// One ordered, labelled, canonically encoded component of a resolved key.
///
/// A part retains its logical label, its dimension kind, its encoding
/// contract name and its protected canonical bytes. It has no `Debug` or
/// `Display` implementation.
#[derive(Clone)]
pub struct RateLimitKeyPart {
    name: Box<str>,
    kind: RateDimensionKind,
    encoding_id: &'static str,
    value: Arc<[u8]>,
}

impl RateLimitKeyPart {
    pub(crate) fn resolved(
        name: &str,
        kind: RateDimensionKind,
        encoded: &EncodedKeyPart,
    ) -> Result<Self, RateLimitError> {
        let label_matches_kind = match kind {
            RateDimensionKind::Origin => name == ORIGIN_LABEL,
            RateDimensionKind::Endpoint => name == ENDPOINT_LABEL,
            RateDimensionKind::Custom => valid_custom_label(name),
        };
        if !label_matches_kind || encoded.is_empty() {
            return Err(invalid_key());
        }
        Ok(Self {
            name: name.into(),
            kind,
            encoding_id: encoded.encoding_id(),
            value: encoded.shared_bytes(),
        })
    }

    /// Returns the explicit logical dimension label.
    ///
    /// Labels are state-affecting: `"client.tenant"` and `"tenant"` select
    /// different buckets even for identical values.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns whether this is the built-in origin, built-in endpoint, or a
    /// custom dimension.
    #[must_use]
    pub const fn kind(&self) -> RateDimensionKind {
        self.kind
    }

    /// Returns the encoding contract name of this value.
    #[must_use]
    pub const fn encoding_id(&self) -> &'static str {
        self.encoding_id
    }

    /// Returns the potentially sensitive canonical dimension value.
    ///
    /// Custom limiters must protect this value in their storage and logging.
    #[must_use]
    pub fn value(&self) -> &[u8] {
        &self.value
    }

    fn accounted_bytes(&self) -> Option<usize> {
        KEY_PART_FRAMING_BYTES
            .checked_add(self.name.len())?
            .checked_add(self.value.len())
    }
}

/// An ordered resolved rate-limit key of 1 to [`MAX_RATE_LIMIT_KEY_PARTS`]
/// uniquely labelled dimensions.
///
/// This type intentionally has no automatic `Debug` or `Display`
/// implementation because values may be operationally sensitive.
#[derive(Clone)]
pub struct RateLimitKey {
    parts: Box<[RateLimitKeyPart]>,
    total_bytes: usize,
}

impl RateLimitKey {
    /// Builds an ordered key of custom dimensions by canonically encoding
    /// each value through [`RateKeyPart`].
    ///
    /// Labels are explicit, nonempty, unique, at most
    /// [`MAX_RATE_LIMIT_KEY_PART_NAME_BYTES`] bytes, and may not be the
    /// reserved built-in labels `origin` or `endpoint`. Each value is encoded
    /// into the remaining aggregate [`MAX_RATE_LIMIT_KEY_BYTES`] budget; the
    /// first invalid, duplicate or excess part stops iteration.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::InvalidKey`] for label, count or size
    /// violations and an empty key, and
    /// [`RateLimitErrorKind::InvalidConfiguration`] for an invalid
    /// `ENCODING_ID`.
    pub fn from_canonical_values<I, N, V>(parts: I) -> Result<Self, RateLimitError>
    where
        I: IntoIterator<Item = (N, V)>,
        N: AsRef<str>,
        V: RateKeyPart,
    {
        let mut builder = KeyAccumulator::default();
        for (name, value) in parts {
            let name = name.as_ref();
            let remaining = builder.remaining_value_bytes(name, RateDimensionKind::Custom)?;
            let encoded = encode_canonical(&value, remaining)?;
            builder.push(name, RateDimensionKind::Custom, &encoded)?;
        }
        builder.finish()
    }

    /// Builds an ordered key of custom dimensions from already encoded parts.
    ///
    /// This is the heterogeneous form of [`Self::from_canonical_values`]; use
    /// [`crate::rate_limit::encode_canonical`] or
    /// [`crate::rate_limit::encode_canonical_with`] to produce each part.
    ///
    /// # Errors
    ///
    /// As for [`Self::from_canonical_values`].
    pub fn from_encoded_parts<I, N>(parts: I) -> Result<Self, RateLimitError>
    where
        I: IntoIterator<Item = (N, EncodedKeyPart)>,
        N: AsRef<str>,
    {
        let mut builder = KeyAccumulator::default();
        for (name, encoded) in parts {
            let name = name.as_ref();
            let remaining = builder.remaining_value_bytes(name, RateDimensionKind::Custom)?;
            if encoded.len() > remaining {
                return Err(invalid_key());
            }
            builder.push(name, RateDimensionKind::Custom, &encoded)?;
        }
        builder.finish()
    }

    /// Returns the ordered key components.
    #[must_use]
    pub fn parts(&self) -> &[RateLimitKeyPart] {
        &self.parts
    }

    /// Returns the checked aggregate accounted byte length: for every part,
    /// 12 framing bytes plus its label and canonical value bytes.
    #[must_use]
    pub const fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Returns whether the key contains the built-in origin dimension.
    #[must_use]
    pub fn uses_origin(&self) -> bool {
        self.parts
            .iter()
            .any(|part| part.kind == RateDimensionKind::Origin)
    }

    fn same_values(&self, other: &Self) -> bool {
        self.parts.len() == other.parts.len()
            && self
                .parts
                .iter()
                .zip(other.parts.iter())
                .all(|(left, right)| {
                    left.name == right.name
                        && left.kind == right.kind
                        && left.encoding_id == right.encoding_id
                        && left.value == right.value
                })
    }
}

/// Incremental checked key construction shared by public and resolved keys.
#[derive(Default)]
pub(crate) struct KeyAccumulator {
    parts: Vec<RateLimitKeyPart>,
    total_bytes: usize,
}

impl KeyAccumulator {
    /// Validates the next label and returns the remaining canonical value
    /// budget under the hard aggregate key limit.
    pub(crate) fn remaining_value_bytes(
        &self,
        name: &str,
        kind: RateDimensionKind,
    ) -> Result<usize, RateLimitError> {
        if self.parts.len() >= MAX_RATE_LIMIT_KEY_PARTS {
            return Err(invalid_key());
        }
        let label_valid = match kind {
            RateDimensionKind::Origin => name == ORIGIN_LABEL,
            RateDimensionKind::Endpoint => name == ENDPOINT_LABEL,
            RateDimensionKind::Custom => valid_custom_label(name),
        };
        if !label_valid || self.parts.iter().any(|part| part.name.as_ref() == name) {
            return Err(invalid_key());
        }
        let before_value = self
            .total_bytes
            .checked_add(KEY_PART_FRAMING_BYTES)
            .and_then(|bytes| bytes.checked_add(name.len()))
            .ok_or_else(invalid_key)?;
        MAX_RATE_LIMIT_KEY_BYTES
            .checked_sub(before_value)
            .ok_or_else(invalid_key)
    }

    pub(crate) fn push(
        &mut self,
        name: &str,
        kind: RateDimensionKind,
        encoded: &EncodedKeyPart,
    ) -> Result<(), RateLimitError> {
        let remaining = self.remaining_value_bytes(name, kind)?;
        if encoded.len() > remaining {
            return Err(invalid_key());
        }
        let part = RateLimitKeyPart::resolved(name, kind, encoded)?;
        self.total_bytes = self
            .total_bytes
            .checked_add(part.accounted_bytes().ok_or_else(invalid_key)?)
            .ok_or_else(invalid_key)?;
        self.parts.push(part);
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<RateLimitKey, RateLimitError> {
        if self.parts.is_empty() || self.total_bytes > MAX_RATE_LIMIT_KEY_BYTES {
            return Err(invalid_key());
        }
        Ok(RateLimitKey {
            parts: self.parts.into_boxed_slice(),
            total_bytes: self.total_bytes,
        })
    }
}

/// One positive capacity over one positive refill duration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RateLimitWindow {
    capacity: NonZeroU32,
    duration: Duration,
}

impl RateLimitWindow {
    /// Creates one finite positive rate-limit window.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::InvalidConfiguration`] for a zero
    /// duration.
    pub fn new(capacity: NonZeroU32, duration: Duration) -> Result<Self, RateLimitError> {
        if duration.is_zero() {
            return Err(invalid_configuration());
        }
        Ok(Self { capacity, duration })
    }

    /// Returns the window capacity.
    #[must_use]
    pub const fn capacity(&self) -> NonZeroU32 {
        self.capacity
    }

    /// Returns the positive refill duration.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }
}

/// One resolved rate-limit policy and all of its conjunctive windows.
///
/// A limiter combines the policy ID and ordered key with each window to
/// select independent quota state (RATE-03). Cost is charged against every
/// window and is not part of bucket identity. The value-free
/// [`RatePolicySchema`] is computed once at construction.
#[derive(Clone)]
pub struct RateLimitPolicy {
    id: RateLimitId,
    key: RateLimitKey,
    cost: NonZeroU32,
    windows: Box<[RateLimitWindow]>,
    schema: Arc<RatePolicySchema>,
}

impl RateLimitPolicy {
    /// Creates one policy after validating its window and cost invariants.
    ///
    /// A policy has 1 to [`MAX_RATE_LIMIT_WINDOWS_PER_POLICY`] windows with
    /// pairwise distinct durations, and a cost no greater than any window
    /// capacity (which could never be admitted). The first excess, repeated
    /// or unadmittable window stops iteration immediately. Windows keep their
    /// source order as provenance; the schema orders them canonically.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::InvalidConfiguration`] for any window or
    /// cost violation.
    pub fn new(
        id: RateLimitId,
        key: RateLimitKey,
        cost: NonZeroU32,
        windows: impl IntoIterator<Item = RateLimitWindow>,
    ) -> Result<Self, RateLimitError> {
        let mut retained: Vec<RateLimitWindow> = Vec::new();
        for window in windows {
            validate_window_append(&retained, cost, &window)?;
            retained.push(window);
        }
        if retained.is_empty() || key.parts.is_empty() {
            return Err(invalid_configuration());
        }
        let schema = Arc::new(RatePolicySchema::from_parts(&id, &key, &retained));
        Ok(Self {
            id,
            key,
            cost,
            windows: retained.into_boxed_slice(),
            schema,
        })
    }

    /// Returns the policy identifier.
    #[must_use]
    pub const fn id(&self) -> &RateLimitId {
        &self.id
    }

    /// Returns the resolved ordered key.
    #[must_use]
    pub const fn key(&self) -> &RateLimitKey {
        &self.key
    }

    /// Returns the positive per-attempt cost charged against every window.
    #[must_use]
    pub const fn cost(&self) -> NonZeroU32 {
        self.cost
    }

    /// Returns the policy windows in source order.
    #[must_use]
    pub const fn windows(&self) -> &[RateLimitWindow] {
        &self.windows
    }

    /// Returns the value-free schema descriptor of this policy.
    #[must_use]
    pub fn schema(&self) -> &RatePolicySchema {
        &self.schema
    }

    /// Returns the shared schema descriptor (plan summaries share it instead
    /// of deep-copying it for every prepared head).
    pub(crate) const fn shared_schema(&self) -> &Arc<RatePolicySchema> {
        &self.schema
    }

    /// Returns whether a repeated policy is an exact duplicate: identical
    /// schema (ID, labels, kinds, encodings, canonical windows), cost and
    /// canonical key values.
    fn is_exact_duplicate_of(&self, other: &Self) -> bool {
        self.schema == other.schema && self.cost == other.cost && self.key.same_values(&other.key)
    }
}

pub(crate) fn validate_window_append(
    retained: &[RateLimitWindow],
    cost: NonZeroU32,
    window: &RateLimitWindow,
) -> Result<(), RateLimitError> {
    if retained.len() >= MAX_RATE_LIMIT_WINDOWS_PER_POLICY
        || window.duration.is_zero()
        || cost.get() > window.capacity.get()
        || retained
            .iter()
            .any(|earlier| earlier.duration == window.duration)
    {
        return Err(invalid_configuration());
    }
    Ok(())
}

/// The canonical immutable effective plan for one Concord attempt.
///
/// Plan values intentionally have no automatic `Debug` or `Display`
/// implementation.
#[derive(Clone)]
pub struct RateLimitPlan {
    policies: Box<[RateLimitPolicy]>,
}

impl RateLimitPlan {
    /// Creates an empty plan that requires no limiter authority.
    #[must_use]
    pub fn empty() -> Self {
        Self::from_validated_policies(Vec::new())
    }

    /// Creates one bounded plan in effective declaration order.
    ///
    /// A repeated policy ID is deduplicated only when the repetition is an
    /// exact duplicate (identical schema, cost and canonical key values);
    /// any other repetition of an ID is a conflict. At most
    /// [`MAX_RATE_LIMIT_POLICIES_PER_PLAN`] distinct policies are retained,
    /// and the first conflicting or excess policy stops iteration.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::InvalidConfiguration`] for a conflict or
    /// excess policy.
    pub fn try_from_policies<I>(policies: I) -> Result<Self, RateLimitError>
    where
        I: IntoIterator<Item = RateLimitPolicy>,
    {
        let mut retained = Vec::new();
        for policy in policies {
            Self::push_policy(&mut retained, policy)?;
        }
        Ok(Self::from_validated_policies(retained))
    }

    /// Appends one policy under the plan identity rules.
    pub(crate) fn push_policy(
        policies: &mut Vec<RateLimitPolicy>,
        policy: RateLimitPolicy,
    ) -> Result<(), RateLimitError> {
        if let Some(earlier) = policies
            .iter()
            .find(|earlier| earlier.id.as_str() == policy.id.as_str())
        {
            return if earlier.is_exact_duplicate_of(&policy) {
                Ok(())
            } else {
                Err(invalid_configuration())
            };
        }
        if policies.len() >= MAX_RATE_LIMIT_POLICIES_PER_PLAN {
            return Err(invalid_configuration());
        }
        policies.push(policy);
        Ok(())
    }

    pub(crate) fn from_validated_policies(policies: Vec<RateLimitPolicy>) -> Self {
        debug_assert!(policies.len() <= MAX_RATE_LIMIT_POLICIES_PER_PLAN);
        debug_assert!(policies.iter().enumerate().all(|(index, policy)| {
            !policies[..index]
                .iter()
                .any(|earlier| earlier.id.as_str() == policy.id.as_str())
        }));
        Self {
            policies: policies.into_boxed_slice(),
        }
    }

    /// Returns effective policies in normalized declaration order.
    #[must_use]
    pub const fn policies(&self) -> &[RateLimitPolicy] {
        &self.policies
    }

    /// Returns whether no rate-limit policy applies.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.policies.is_empty()
    }

    /// Returns whether any policy uses the built-in origin dimension.
    #[must_use]
    pub fn uses_origin(&self) -> bool {
        self.policies.iter().any(|policy| policy.key.uses_origin())
    }

    pub(crate) fn validate_key_limit(&self, limit: usize) -> Result<(), RateLimitError> {
        if limit > MAX_RATE_LIMIT_KEY_BYTES {
            return Err(invalid_configuration());
        }
        if self
            .policies
            .iter()
            .any(|policy| policy.key.total_bytes > limit)
        {
            return Err(invalid_key());
        }
        Ok(())
    }
}

/// Opaque proof that a limiter admitted one visible Concord attempt.
///
/// This zero-sized value is not an in-flight concurrency lease. Concord does
/// not retain it through HTTP execution or use its drop to notify the limiter.
/// A committed admission is non-refundable: later cancellation or a
/// credential-freshness restart may consume quota without dispatch. Custom
/// concurrency limiting must own its state and release protocol.
pub struct RateLimitPermit {
    _private: (),
}

impl RateLimitPermit {
    /// Creates an opaque attempt-admission proof for a custom limiter.
    #[must_use]
    pub const fn granted() -> Self {
        Self { _private: () }
    }
}

pub(crate) fn valid_custom_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= MAX_RATE_LIMIT_KEY_PART_NAME_BYTES
        && label != ORIGIN_LABEL
        && label != ENDPOINT_LABEL
}

fn invalid_configuration() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::InvalidConfiguration)
}

fn invalid_key() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::InvalidKey)
}

#[cfg(test)]
mod tests;
