use std::num::NonZeroU32;
use std::time::Duration;

use crate::error::RateLimitErrorKind;

use super::RateLimitError;

/// A stable logical identifier for one rate-limit rule.
pub struct RateLimitId {
    value: Box<str>,
}

impl RateLimitId {
    /// Creates a nonempty rule identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, RateLimitError> {
        let value = value.into();
        if value.is_empty() {
            return Err(invalid_configuration());
        }
        Ok(Self {
            value: value.into_boxed_str(),
        })
    }

    /// Returns the rule identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

/// One ordered name/value component of a resolved rate-limit key.
pub struct RateLimitKeyPart {
    name: Box<str>,
    value: Box<str>,
}

impl RateLimitKeyPart {
    /// Creates one key component.
    #[must_use]
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into().into_boxed_str(),
            value: value.into().into_boxed_str(),
        }
    }

    /// Returns the stable dimension name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the resolved dimension value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// An ordered resolved rate-limit key.
///
/// This type intentionally has no automatic `Debug` or `Display`
/// implementation because values may be operationally sensitive.
pub struct RateLimitKey {
    parts: Box<[RateLimitKeyPart]>,
    total_bytes: usize,
}

impl RateLimitKey {
    /// Creates the single global key with no components.
    #[must_use]
    pub fn global() -> Self {
        Self {
            parts: Box::new([]),
            total_bytes: 0,
        }
    }

    /// Creates an ordered key after validating its structural invariants.
    pub fn from_parts<I>(parts: I) -> Result<Self, RateLimitError>
    where
        I: IntoIterator<Item = RateLimitKeyPart>,
    {
        let parts: Vec<_> = parts.into_iter().collect();
        let mut total_bytes = 0_usize;
        for (index, part) in parts.iter().enumerate() {
            if part.name.is_empty()
                || parts[..index]
                    .iter()
                    .any(|earlier| earlier.name == part.name)
            {
                return Err(RateLimitError::new(RateLimitErrorKind::InvalidKey));
            }
            total_bytes = total_bytes
                .checked_add(part.name.len())
                .and_then(|bytes| bytes.checked_add(part.value.len()))
                .ok_or_else(|| RateLimitError::new(RateLimitErrorKind::InvalidKey))?;
        }
        Ok(Self {
            parts: parts.into_boxed_slice(),
            total_bytes,
        })
    }

    /// Returns the ordered key components.
    #[must_use]
    pub fn parts(&self) -> &[RateLimitKeyPart] {
        &self.parts
    }

    /// Returns whether this is the global key.
    #[must_use]
    pub fn is_global(&self) -> bool {
        self.parts.is_empty()
    }

    /// Returns the checked aggregate byte length of component names and values.
    #[must_use]
    pub const fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

/// One positive capacity over one positive refill duration.
pub struct RateLimitWindow {
    capacity: NonZeroU32,
    duration: Duration,
}

impl RateLimitWindow {
    /// Creates one finite positive rate-limit window.
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

/// One resolved rate-limit rule and all of its windows.
pub struct RateLimitRule {
    id: RateLimitId,
    key: RateLimitKey,
    cost: NonZeroU32,
    windows: Box<[RateLimitWindow]>,
}

impl RateLimitRule {
    /// Creates one rule after validating its window and cost invariants.
    pub fn new(
        id: RateLimitId,
        key: RateLimitKey,
        cost: NonZeroU32,
        windows: impl IntoIterator<Item = RateLimitWindow>,
    ) -> Result<Self, RateLimitError> {
        let windows: Vec<_> = windows.into_iter().collect();
        if windows.is_empty()
            || windows
                .iter()
                .any(|window| cost.get() > window.capacity.get())
        {
            return Err(invalid_configuration());
        }
        Ok(Self {
            id,
            key,
            cost,
            windows: windows.into_boxed_slice(),
        })
    }

    /// Returns the rule identifier.
    #[must_use]
    pub const fn id(&self) -> &RateLimitId {
        &self.id
    }

    /// Returns the resolved ordered key.
    #[must_use]
    pub const fn key(&self) -> &RateLimitKey {
        &self.key
    }

    /// Returns the positive reservation cost.
    #[must_use]
    pub const fn cost(&self) -> NonZeroU32 {
        self.cost
    }

    /// Returns the rule windows in declaration order.
    #[must_use]
    pub const fn windows(&self) -> &[RateLimitWindow] {
        &self.windows
    }
}

/// The canonical immutable effective plan for one Concord attempt.
///
/// Plan values intentionally have no automatic `Debug` or `Display`
/// implementation.
pub struct RateLimitPlan {
    rules: Box<[RateLimitRule]>,
}

impl RateLimitPlan {
    /// Creates an empty plan that requires no limiter authority.
    #[must_use]
    pub fn empty() -> Self {
        Self::from_rules(Vec::new())
    }

    /// Creates a plan from already validated semantic rules in their effective
    /// declaration order.
    #[must_use]
    pub fn from_rules(rules: Vec<RateLimitRule>) -> Self {
        Self {
            rules: rules.into_boxed_slice(),
        }
    }

    /// Returns effective rules in normalized declaration order.
    #[must_use]
    pub const fn rules(&self) -> &[RateLimitRule] {
        &self.rules
    }

    /// Returns whether no rate-limit rule applies.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub(crate) fn validate_key_limit(&self, limit: usize) -> Result<(), RateLimitError> {
        if self.rules.iter().any(|rule| rule.key.total_bytes > limit) {
            return Err(RateLimitError::new(RateLimitErrorKind::InvalidKey));
        }
        Ok(())
    }
}

/// Opaque proof that a limiter granted authority for one attempt.
pub struct RateLimitPermit {
    _private: (),
}

impl RateLimitPermit {
    /// Creates an opaque granted permit for a custom limiter.
    #[must_use]
    pub const fn granted() -> Self {
        Self { _private: () }
    }
}

fn invalid_configuration() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::InvalidConfiguration)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(capacity: u32, seconds: u64) -> RateLimitWindow {
        RateLimitWindow::new(
            NonZeroU32::new(capacity).expect("positive test capacity"),
            Duration::from_secs(seconds),
        )
        .expect("valid test window")
    }

    fn rate_error<T>(result: Result<T, RateLimitError>, message: &str) -> RateLimitError {
        match result {
            Ok(_) => panic!("{message}"),
            Err(error) => error,
        }
    }

    #[test]
    fn constructors_and_accessors_preserve_order_and_checked_bytes() {
        let key = RateLimitKey::from_parts([
            RateLimitKeyPart::new("host", "api.example.test"),
            RateLimitKeyPart::new("vars.tenant", "north"),
        ])
        .expect("valid key");
        assert_eq!(key.parts()[0].name(), "host");
        assert_eq!(key.parts()[0].value(), "api.example.test");
        assert_eq!(key.parts()[1].name(), "vars.tenant");
        assert_eq!(key.parts()[1].value(), "north");
        assert_eq!(key.total_bytes(), 4 + 16 + 11 + 5);
        assert!(!key.is_global());

        let rule = RateLimitRule::new(
            RateLimitId::new("tenant").expect("ID"),
            key,
            NonZeroU32::new(2).expect("cost"),
            [window(100, 60), window(1_000, 600)],
        )
        .expect("rule");
        assert_eq!(rule.id().as_str(), "tenant");
        assert_eq!(rule.cost().get(), 2);
        assert_eq!(rule.windows().len(), 2);
        assert_eq!(rule.windows()[0].capacity().get(), 100);
        assert_eq!(rule.windows()[0].duration(), Duration::from_secs(60));

        let plan = RateLimitPlan::from_rules(vec![rule]);
        assert!(!plan.is_empty());
        assert_eq!(plan.rules()[0].id().as_str(), "tenant");
    }

    #[test]
    fn global_and_structural_failures_are_exact() {
        let global = RateLimitKey::global();
        assert!(global.is_global());
        assert!(global.parts().is_empty());
        assert_eq!(global.total_bytes(), 0);
        assert!(RateLimitPlan::empty().is_empty());

        assert_eq!(
            rate_error(RateLimitId::new(""), "empty ID unexpectedly succeeded").kind(),
            RateLimitErrorKind::InvalidConfiguration
        );
        assert_eq!(
            rate_error(
                RateLimitKey::from_parts([RateLimitKeyPart::new("", "value")]),
                "empty key-part name unexpectedly succeeded",
            )
            .kind(),
            RateLimitErrorKind::InvalidKey
        );
        assert_eq!(
            rate_error(
                RateLimitKey::from_parts([
                    RateLimitKeyPart::new("host", "one"),
                    RateLimitKeyPart::new("host", "two"),
                ]),
                "duplicate key-part name unexpectedly succeeded",
            )
            .kind(),
            RateLimitErrorKind::InvalidKey
        );
        assert_eq!(
            rate_error(
                RateLimitWindow::new(NonZeroU32::MIN, Duration::ZERO),
                "zero duration unexpectedly succeeded",
            )
            .kind(),
            RateLimitErrorKind::InvalidConfiguration
        );
        assert_eq!(
            rate_error(
                RateLimitRule::new(
                    RateLimitId::new("empty").expect("ID"),
                    RateLimitKey::global(),
                    NonZeroU32::MIN,
                    [],
                ),
                "empty windows unexpectedly succeeded",
            )
            .kind(),
            RateLimitErrorKind::InvalidConfiguration
        );
        assert_eq!(
            rate_error(
                RateLimitRule::new(
                    RateLimitId::new("cost").expect("ID"),
                    RateLimitKey::global(),
                    NonZeroU32::new(2).expect("cost"),
                    [window(1, 1)],
                ),
                "cost above capacity unexpectedly succeeded",
            )
            .kind(),
            RateLimitErrorKind::InvalidConfiguration
        );
    }

    #[test]
    fn plan_key_limit_is_key_safe_and_zero_allows_only_global() {
        const KEY_SENTINEL: &str = "tenant-key-5be70c07";
        let global = RateLimitRule::new(
            RateLimitId::new("global").expect("ID"),
            RateLimitKey::global(),
            NonZeroU32::MIN,
            [window(1, 1)],
        )
        .expect("global rule");
        RateLimitPlan::from_rules(vec![global])
            .validate_key_limit(0)
            .expect("global key fits zero");

        let keyed = RateLimitRule::new(
            RateLimitId::new("keyed").expect("ID"),
            RateLimitKey::from_parts([RateLimitKeyPart::new("tenant", KEY_SENTINEL)]).expect("key"),
            NonZeroU32::MIN,
            [window(1, 1)],
        )
        .expect("keyed rule");
        let error = RateLimitPlan::from_rules(vec![keyed])
            .validate_key_limit(0)
            .expect_err("nonempty key exceeds zero");
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
        assert!(!error.to_string().contains(KEY_SENTINEL));
        assert!(!format!("{error:?}").contains(KEY_SENTINEL));
    }
}
