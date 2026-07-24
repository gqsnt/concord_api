use std::num::NonZeroU32;
use std::time::Duration;

use crate::error::RateLimitErrorKind;
use crate::limits::{
    MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_KEY_BYTES, MAX_RATE_LIMIT_KEY_PART_NAME_BYTES,
    MAX_RATE_LIMIT_KEY_PARTS, MAX_RATE_LIMIT_RULES_PER_PLAN, MAX_RATE_LIMIT_WINDOWS_PER_RULE,
};

use super::RateLimitError;

/// A stable logical identifier for one rate-limit rule.
pub struct RateLimitId {
    value: Box<str>,
}

impl RateLimitId {
    /// Creates a nonempty rule identifier no larger than
    /// [`MAX_RATE_LIMIT_ID_BYTES`].
    pub fn new(value: impl AsRef<str>) -> Result<Self, RateLimitError> {
        let value = value.as_ref();
        if value.is_empty() || value.len() > MAX_RATE_LIMIT_ID_BYTES {
            return Err(invalid_configuration());
        }
        Ok(Self {
            value: value.into(),
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
    /// Creates one bounded key component.
    ///
    /// The name is nonempty and no larger than
    /// [`MAX_RATE_LIMIT_KEY_PART_NAME_BYTES`]. The combined name and value are
    /// no larger than [`MAX_RATE_LIMIT_KEY_BYTES`].
    pub fn new(name: impl AsRef<str>, value: impl AsRef<str>) -> Result<Self, RateLimitError> {
        let name = name.as_ref();
        let value = value.as_ref();
        let total_bytes = name
            .len()
            .checked_add(value.len())
            .ok_or_else(invalid_key)?;
        if name.is_empty()
            || name.len() > MAX_RATE_LIMIT_KEY_PART_NAME_BYTES
            || value.len() > MAX_RATE_LIMIT_KEY_BYTES
            || total_bytes > MAX_RATE_LIMIT_KEY_BYTES
        {
            return Err(invalid_key());
        }
        Ok(Self {
            name: name.into(),
            value: value.into(),
        })
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
    ///
    /// At most [`MAX_RATE_LIMIT_KEY_PARTS`] parts and
    /// [`MAX_RATE_LIMIT_KEY_BYTES`] aggregate name/value bytes are retained.
    /// Duplicate names and the first excess item stop iteration immediately.
    pub fn from_parts<I>(parts: I) -> Result<Self, RateLimitError>
    where
        I: IntoIterator<Item = RateLimitKeyPart>,
    {
        Self::from_mapped_parts(parts, Ok)
    }

    pub(crate) fn from_values<I, N, V>(parts: I) -> Result<Self, RateLimitError>
    where
        I: IntoIterator<Item = (N, V)>,
        N: AsRef<str>,
        V: AsRef<str>,
    {
        Self::from_mapped_parts(parts, |(name, value)| RateLimitKeyPart::new(name, value))
    }

    fn from_mapped_parts<I, T, F>(parts: I, mut map: F) -> Result<Self, RateLimitError>
    where
        I: IntoIterator<Item = T>,
        F: FnMut(T) -> Result<RateLimitKeyPart, RateLimitError>,
    {
        let mut retained = Vec::new();
        let mut total_bytes = 0_usize;
        for item in parts {
            if retained.len() >= MAX_RATE_LIMIT_KEY_PARTS {
                return Err(invalid_key());
            }
            let part = map(item)?;
            if retained
                .iter()
                .any(|earlier: &RateLimitKeyPart| earlier.name == part.name)
            {
                return Err(invalid_key());
            }
            total_bytes = total_bytes
                .checked_add(part.name.len())
                .and_then(|bytes| bytes.checked_add(part.value.len()))
                .ok_or_else(invalid_key)?;
            if total_bytes > MAX_RATE_LIMIT_KEY_BYTES {
                return Err(invalid_key());
            }
            retained.push(part);
        }
        Ok(Self {
            parts: retained.into_boxed_slice(),
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
    ///
    /// At most [`MAX_RATE_LIMIT_WINDOWS_PER_RULE`] windows are retained, and
    /// the first excess window stops iteration immediately.
    pub fn new(
        id: RateLimitId,
        key: RateLimitKey,
        cost: NonZeroU32,
        windows: impl IntoIterator<Item = RateLimitWindow>,
    ) -> Result<Self, RateLimitError> {
        Self::from_mapped_windows(id, key, cost, windows, Ok)
    }

    pub(crate) fn from_window_values<I>(
        id: RateLimitId,
        key: RateLimitKey,
        cost: NonZeroU32,
        windows: I,
    ) -> Result<Self, RateLimitError>
    where
        I: IntoIterator<Item = (u32, Duration)>,
    {
        Self::from_mapped_windows(id, key, cost, windows, |(capacity, duration)| {
            let capacity = NonZeroU32::new(capacity).ok_or_else(invalid_configuration)?;
            RateLimitWindow::new(capacity, duration)
        })
    }

    fn from_mapped_windows<I, T, F>(
        id: RateLimitId,
        key: RateLimitKey,
        cost: NonZeroU32,
        windows: I,
        mut map: F,
    ) -> Result<Self, RateLimitError>
    where
        I: IntoIterator<Item = T>,
        F: FnMut(T) -> Result<RateLimitWindow, RateLimitError>,
    {
        let mut retained = Vec::new();
        for item in windows {
            if retained.len() >= MAX_RATE_LIMIT_WINDOWS_PER_RULE {
                return Err(invalid_configuration());
            }
            let window = map(item)?;
            if cost.get() > window.capacity.get() {
                return Err(invalid_configuration());
            }
            retained.push(window);
        }
        if retained.is_empty() {
            return Err(invalid_configuration());
        }
        Ok(Self {
            id,
            key,
            cost,
            windows: retained.into_boxed_slice(),
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

    pub(crate) fn retained_structure_bytes(&self) -> Option<usize> {
        self.id.value.len().checked_add(self.key.total_bytes)
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
        Self::from_validated_rules(Vec::new())
    }

    /// Creates one bounded plan in effective declaration order.
    ///
    /// At most [`MAX_RATE_LIMIT_RULES_PER_PLAN`] rules are retained. Duplicate
    /// IDs and the first excess rule stop iteration immediately.
    pub fn try_from_rules<I>(rules: I) -> Result<Self, RateLimitError>
    where
        I: IntoIterator<Item = RateLimitRule>,
    {
        let mut retained = Vec::new();
        for rule in rules {
            Self::push_validated_rule(&mut retained, rule)?;
        }
        Ok(Self::from_validated_rules(retained))
    }

    pub(crate) fn validate_rule_id_append(
        rules: &[RateLimitRule],
        id: &str,
    ) -> Result<(), RateLimitError> {
        if rules.len() >= MAX_RATE_LIMIT_RULES_PER_PLAN
            || rules.iter().any(|rule| rule.id.as_str() == id)
        {
            Err(invalid_configuration())
        } else {
            Ok(())
        }
    }

    pub(crate) fn validate_rule_append(
        rules: &[RateLimitRule],
        rule: &RateLimitRule,
    ) -> Result<(), RateLimitError> {
        Self::validate_rule_id_append(rules, rule.id.as_str())
    }

    pub(crate) fn push_validated_rule(
        rules: &mut Vec<RateLimitRule>,
        rule: RateLimitRule,
    ) -> Result<(), RateLimitError> {
        Self::validate_rule_append(rules, &rule)?;
        rules.push(rule);
        Ok(())
    }

    pub(crate) fn from_validated_rules(rules: Vec<RateLimitRule>) -> Self {
        debug_assert!(rules.len() <= MAX_RATE_LIMIT_RULES_PER_PLAN);
        debug_assert!(rules.iter().enumerate().all(|(index, rule)| {
            !rules[..index]
                .iter()
                .any(|earlier| earlier.id.as_str() == rule.id.as_str())
        }));
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
        if limit > MAX_RATE_LIMIT_KEY_BYTES {
            return Err(invalid_configuration());
        }
        if self.rules.iter().any(|rule| rule.key.total_bytes > limit) {
            return Err(invalid_key());
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

fn invalid_key() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::InvalidKey)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    const ID_SENTINEL: &str = "rate-id-private-sentinel-0f3a";
    const KEY_SENTINEL: &str = "rate-key-private-sentinel-7b2c";

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

    fn part(name: impl AsRef<str>, value: impl AsRef<str>) -> RateLimitKeyPart {
        RateLimitKeyPart::new(name, value).expect("valid key part")
    }

    fn rule(id: impl AsRef<str>) -> RateLimitRule {
        RateLimitRule::new(
            RateLimitId::new(id).expect("valid rule ID"),
            RateLimitKey::global(),
            NonZeroU32::MIN,
            [window(1, 1)],
        )
        .expect("valid rule")
    }

    fn assert_redacted(error: &RateLimitError, sentinels: &[&str]) {
        let display = error.to_string();
        let debug = format!("{error:?}");
        for sentinel in sentinels {
            assert!(!display.contains(sentinel));
            assert!(!debug.contains(sentinel));
        }
    }

    struct PartLimitIterator {
        calls: Arc<AtomicUsize>,
    }

    impl Iterator for PartLimitIterator {
        type Item = RateLimitKeyPart;

        fn next(&mut self) -> Option<Self::Item> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            assert!(
                call <= MAX_RATE_LIMIT_KEY_PARTS,
                "key iterator consumed beyond the first excess"
            );
            Some(part(format!("part-{call}"), "value"))
        }
    }

    struct DuplicatePartIterator {
        calls: Arc<AtomicUsize>,
    }

    impl Iterator for DuplicatePartIterator {
        type Item = RateLimitKeyPart;

        fn next(&mut self) -> Option<Self::Item> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            assert!(call < 2, "duplicate-key iterator tail was consumed");
            Some(part("duplicate", KEY_SENTINEL))
        }
    }

    struct WindowLimitIterator {
        calls: Arc<AtomicUsize>,
    }

    impl Iterator for WindowLimitIterator {
        type Item = RateLimitWindow;

        fn next(&mut self) -> Option<Self::Item> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            assert!(
                call <= MAX_RATE_LIMIT_WINDOWS_PER_RULE,
                "window iterator consumed beyond the first excess"
            );
            Some(window(100, u64::try_from(call + 1).expect("test index")))
        }
    }

    struct RuleLimitIterator {
        calls: Arc<AtomicUsize>,
    }

    impl Iterator for RuleLimitIterator {
        type Item = RateLimitRule;

        fn next(&mut self) -> Option<Self::Item> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            assert!(
                call <= MAX_RATE_LIMIT_RULES_PER_PLAN,
                "plan iterator consumed beyond the first excess"
            );
            Some(rule(format!("rule-{call}")))
        }
    }

    struct DuplicateRuleIterator {
        calls: Arc<AtomicUsize>,
    }

    impl Iterator for DuplicateRuleIterator {
        type Item = RateLimitRule;

        fn next(&mut self) -> Option<Self::Item> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            assert!(call < 2, "duplicate-rule iterator tail was consumed");
            Some(rule(ID_SENTINEL))
        }
    }

    #[test]
    fn constructors_and_accessors_preserve_order_and_checked_bytes() {
        let key = RateLimitKey::from_parts([
            part("host", "api.example.test"),
            part("vars.tenant", "north"),
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

        let plan = RateLimitPlan::try_from_rules([rule]).expect("valid plan");
        assert!(!plan.is_empty());
        assert_eq!(plan.rules()[0].id().as_str(), "tenant");
    }

    #[test]
    fn rate_limit_id_max_and_first_excess() {
        let boundary = "i".repeat(MAX_RATE_LIMIT_ID_BYTES);
        let id = RateLimitId::new(&boundary).expect("ID byte boundary");
        assert_eq!(id.as_str(), boundary);

        let excessive = format!(
            "{}{ID_SENTINEL}",
            "i".repeat(MAX_RATE_LIMIT_ID_BYTES + 1 - ID_SENTINEL.len())
        );
        let error = rate_error(
            RateLimitId::new(&excessive),
            "first excess ID byte unexpectedly succeeded",
        );
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
        assert_redacted(&error, &[ID_SENTINEL]);

        let error = rate_error(RateLimitId::new(""), "empty ID unexpectedly succeeded");
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
    }

    #[test]
    fn rate_limit_key_part_max_and_first_excess() {
        let name = "n".repeat(MAX_RATE_LIMIT_KEY_PART_NAME_BYTES);
        let value = "v".repeat(MAX_RATE_LIMIT_KEY_BYTES - MAX_RATE_LIMIT_KEY_PART_NAME_BYTES);
        let boundary = RateLimitKeyPart::new(&name, &value).expect("key-part byte boundary");
        assert_eq!(boundary.name().len(), MAX_RATE_LIMIT_KEY_PART_NAME_BYTES);
        assert_eq!(
            boundary.name().len() + boundary.value().len(),
            MAX_RATE_LIMIT_KEY_BYTES
        );

        let excessive_name = format!(
            "{}{KEY_SENTINEL}",
            "n".repeat(MAX_RATE_LIMIT_KEY_PART_NAME_BYTES + 1 - KEY_SENTINEL.len())
        );
        let name_error = rate_error(
            RateLimitKeyPart::new(&excessive_name, ""),
            "first excess name byte unexpectedly succeeded",
        );
        assert_eq!(name_error.kind(), RateLimitErrorKind::InvalidKey);
        assert_redacted(&name_error, &[KEY_SENTINEL]);

        let excessive_value = format!(
            "{}{KEY_SENTINEL}",
            "v".repeat(MAX_RATE_LIMIT_KEY_BYTES - KEY_SENTINEL.len())
        );
        let value_error = rate_error(
            RateLimitKeyPart::new("n", &excessive_value),
            "first excess aggregate byte unexpectedly succeeded",
        );
        assert_eq!(value_error.kind(), RateLimitErrorKind::InvalidKey);
        assert_redacted(&value_error, &[KEY_SENTINEL]);

        let empty_error = rate_error(
            RateLimitKeyPart::new("", KEY_SENTINEL),
            "empty key-part name unexpectedly succeeded",
        );
        assert_eq!(empty_error.kind(), RateLimitErrorKind::InvalidKey);
        assert_redacted(&empty_error, &[KEY_SENTINEL]);
    }

    #[test]
    fn rate_limit_key_iterator_stops_at_first_excess() {
        let boundary = RateLimitKey::from_parts(
            (0..MAX_RATE_LIMIT_KEY_PARTS).map(|index| part(format!("part-{index}"), "value")),
        )
        .expect("key-part count boundary");
        assert_eq!(boundary.parts().len(), MAX_RATE_LIMIT_KEY_PARTS);

        let calls = Arc::new(AtomicUsize::new(0));
        let error = rate_error(
            RateLimitKey::from_parts(PartLimitIterator {
                calls: Arc::clone(&calls),
            }),
            "first excess key part unexpectedly succeeded",
        );
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
        assert_eq!(calls.load(Ordering::SeqCst), MAX_RATE_LIMIT_KEY_PARTS + 1);

        let aggregate_error = rate_error(
            RateLimitKey::from_parts([
                part("a", "v".repeat(MAX_RATE_LIMIT_KEY_BYTES - 1)),
                part("b", KEY_SENTINEL),
            ]),
            "first excess aggregate key byte unexpectedly succeeded",
        );
        assert_eq!(aggregate_error.kind(), RateLimitErrorKind::InvalidKey);
        assert_redacted(&aggregate_error, &[KEY_SENTINEL]);
    }

    #[test]
    fn rate_limit_key_duplicate_stops_without_tail_consumption() {
        let calls = Arc::new(AtomicUsize::new(0));
        let error = rate_error(
            RateLimitKey::from_parts(DuplicatePartIterator {
                calls: Arc::clone(&calls),
            }),
            "duplicate key-part name unexpectedly succeeded",
        );
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_redacted(&error, &[KEY_SENTINEL, "duplicate"]);
    }

    #[test]
    fn rate_limit_rule_window_iterator_stops_at_first_excess() {
        let boundary = RateLimitRule::new(
            RateLimitId::new("window-boundary").expect("ID"),
            RateLimitKey::global(),
            NonZeroU32::MIN,
            (0..MAX_RATE_LIMIT_WINDOWS_PER_RULE)
                .map(|index| window(100, u64::try_from(index + 1).expect("test index"))),
        )
        .expect("window count boundary");
        assert_eq!(boundary.windows().len(), MAX_RATE_LIMIT_WINDOWS_PER_RULE);

        let calls = Arc::new(AtomicUsize::new(0));
        let error = rate_error(
            RateLimitRule::new(
                RateLimitId::new(ID_SENTINEL).expect("ID"),
                RateLimitKey::global(),
                NonZeroU32::MIN,
                WindowLimitIterator {
                    calls: Arc::clone(&calls),
                },
            ),
            "first excess window unexpectedly succeeded",
        );
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            MAX_RATE_LIMIT_WINDOWS_PER_RULE + 1
        );
        assert_redacted(&error, &[ID_SENTINEL]);
    }

    #[test]
    fn rate_limit_plan_rule_iterator_stops_at_first_excess() {
        let boundary = RateLimitPlan::try_from_rules(
            (0..MAX_RATE_LIMIT_RULES_PER_PLAN).map(|index| rule(format!("rule-{index}"))),
        )
        .expect("rule count boundary");
        assert_eq!(boundary.rules().len(), MAX_RATE_LIMIT_RULES_PER_PLAN);

        let calls = Arc::new(AtomicUsize::new(0));
        let error = rate_error(
            RateLimitPlan::try_from_rules(RuleLimitIterator {
                calls: Arc::clone(&calls),
            }),
            "first excess plan rule unexpectedly succeeded",
        );
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            MAX_RATE_LIMIT_RULES_PER_PLAN + 1
        );
    }

    #[test]
    fn rate_limit_plan_rejects_duplicate_ids() {
        let calls = Arc::new(AtomicUsize::new(0));
        let error = rate_error(
            RateLimitPlan::try_from_rules(DuplicateRuleIterator {
                calls: Arc::clone(&calls),
            }),
            "duplicate rule ID unexpectedly succeeded",
        );
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_redacted(&error, &[ID_SENTINEL]);
    }

    #[test]
    fn global_and_structural_failures_are_exact() {
        let global = RateLimitKey::global();
        assert!(global.is_global());
        assert!(global.parts().is_empty());
        assert_eq!(global.total_bytes(), 0);
        assert!(
            RateLimitKey::from_parts(std::iter::empty())
                .expect("empty part iterator is global")
                .is_global()
        );
        assert!(RateLimitPlan::empty().is_empty());

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
        let global = RateLimitRule::new(
            RateLimitId::new("global").expect("ID"),
            RateLimitKey::global(),
            NonZeroU32::MIN,
            [window(1, 1)],
        )
        .expect("global rule");
        RateLimitPlan::try_from_rules([global])
            .expect("global plan")
            .validate_key_limit(0)
            .expect("global key fits zero");

        let keyed = RateLimitRule::new(
            RateLimitId::new("keyed").expect("ID"),
            RateLimitKey::from_parts([part("tenant", KEY_SENTINEL)]).expect("key"),
            NonZeroU32::MIN,
            [window(1, 1)],
        )
        .expect("keyed rule");
        let error = RateLimitPlan::try_from_rules([keyed])
            .expect("keyed plan")
            .validate_key_limit(0)
            .expect_err("nonempty key exceeds zero");
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
        assert_redacted(&error, &[KEY_SENTINEL]);

        let hard_error = RateLimitPlan::empty()
            .validate_key_limit(MAX_RATE_LIMIT_KEY_BYTES + 1)
            .expect_err("operational cap above hard limit");
        assert_eq!(hard_error.kind(), RateLimitErrorKind::InvalidConfiguration);
    }
}
