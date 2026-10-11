use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::time::Duration;

use crate::hooks::{DebugLevel, OperationObserver, default_operation_observer};
use crate::pagination::DEFAULT_TRAVERSAL_MAX_HANDOFFS;
use crate::rate_limit::{LimiterCapability, MAX_RATE_LIMIT_KEY_BYTES, RateLimiter};
use crate::retry::RetryPolicy;
use crate::runtime::defaults::{
    DEFAULT_AUTH_FAILURE_COOLDOWN_SECONDS, DEFAULT_OPERATION_TIMEOUT_SECONDS,
    DEFAULT_PAGINATION_IDENTITY_BYTES, DEFAULT_PAGINATION_MAX_ITEMS, DEFAULT_PAGINATION_MAX_PAGES,
    DEFAULT_PAGINATION_TOTAL_RESPONSE_BYTES, DEFAULT_PROVIDER_MAX_ORIGINS,
    DEFAULT_PROVIDER_MAX_SENDS, DEFAULT_PROVIDER_REQUEST_BYTES, DEFAULT_PROVIDER_RESPONSE_BYTES,
    DEFAULT_PROVIDER_TIMEOUT_SECONDS, DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS,
    DEFAULT_REQUEST_BODY_BYTES, DEFAULT_RESPONSE_BODY_BYTES, DEFAULT_STATUS_BODY_BYTES,
    DEFAULT_STREAM_FRAME_BYTES, DEFAULT_STREAM_IDLE_SECONDS, DEFAULT_STREAM_LIFETIME_SECONDS,
    DEFAULT_STREAM_MAX_RECORDS, DEFAULT_STREAM_RESPONSE_BYTES,
};
use crate::runtime::{WallClock, default_wall_clock};

/// Private immutable finite runtime configuration.
pub(crate) struct RuntimeConfig {
    pub(crate) runtime_endpoint_timeout: Option<Duration>,
    pub(crate) request_body_limit: NonZeroUsize,
    pub(crate) response_body_limit: NonZeroUsize,
    pub(crate) stream_response_limit: NonZeroUsize,
    pub(crate) stream_lifetime: Duration,
    pub(crate) stream_idle_timeout: Duration,
    pub(crate) stream_max_records: NonZeroUsize,
    pub(crate) stream_frame_limit: NonZeroUsize,
    pub(crate) status_body_limit: Option<NonZeroUsize>,
    pub(crate) provider_request_limit: NonZeroUsize,
    pub(crate) provider_response_limit: NonZeroUsize,
    pub(crate) provider_max_sends: u32,
    pub(crate) provider_max_origins: usize,
    pub(crate) provider_timeout: Duration,
    pub(crate) pagination_max_pages: NonZeroU32,
    pub(crate) pagination_max_items: NonZeroUsize,
    pub(crate) pagination_total_response_body_limit: NonZeroUsize,
    pub(crate) pagination_identity_limit: NonZeroUsize,
    pub(crate) aggregate_max_sends: NonZeroU32,
    pub(crate) auth_failure_cooldown_cap: Duration,
    pub(crate) rate_limit_cooldown_cap: Duration,
    pub(crate) rate_limit_key_limit: usize,
    pub(crate) rate_limiter: Arc<dyn RateLimiter>,
    /// Capability snapshotted when the limiter was selected.
    pub(crate) rate_limiter_capability: LimiterCapability,
    /// Whether the application explicitly selected a limiter.
    pub(crate) rate_limiter_selected: bool,
    pub(crate) retry_policy: Option<RetryPolicy>,
    pub(crate) operation_observer: Arc<dyn OperationObserver>,
    pub(crate) wall_clock: Arc<dyn WallClock>,
    pub(crate) debug: DebugLevel,
}

/// A consuming builder whose ordinary limits remain positive and finite.
pub struct RuntimeConfigBuilder {
    config: RuntimeConfig,
    limiter_selection: LimiterSelection,
    provider_selection: ProviderSelection,
    stream_selection: StreamSelection,
    limit_selection: LimitSelection,
}

/// Records which body/status/pagination count setters last received an
/// invalid (zero) value.
///
/// These setters take primitive counts (§20.4); like [`StreamSelection`], an
/// invalid latest trusted selection is never silently replaced by an earlier
/// valid value and is reported by the root build.
#[derive(Clone, Copy, Default)]
struct LimitSelection {
    request_body_invalid: bool,
    response_body_invalid: bool,
    status_body_invalid: bool,
    pagination_pages_invalid: bool,
    pagination_items_invalid: bool,
    pagination_total_bytes_invalid: bool,
    pagination_identity_invalid: bool,
    aggregate_sends_invalid: bool,
}

/// Records which managed-stream setters last received an invalid value.
///
/// Like [`ProviderSelection`], an invalid latest trusted selection is never
/// silently replaced by an earlier valid value; the root build reports it.
#[derive(Clone, Copy, Default)]
struct StreamSelection {
    response_limit_invalid: bool,
    lifetime_invalid: bool,
    idle_timeout_invalid: bool,
    max_records_invalid: bool,
    frame_limit_invalid: bool,
}

/// Records which provider-helper setters last received an invalid value.
///
/// Repeated trusted selections replace earlier ones; an invalid latest
/// selection is never silently replaced by an earlier valid value and is
/// reported by the root build.
#[derive(Clone, Copy, Default)]
struct ProviderSelection {
    request_limit_invalid: bool,
    response_limit_invalid: bool,
    max_sends_invalid: bool,
    max_origins_invalid: bool,
    timeout_invalid: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum LimiterSelection {
    Fallback,
    Explicit,
}

impl RuntimeConfigBuilder {
    pub(crate) fn new() -> Self {
        let rate_limiter = crate::rate_limit::fallback_limiter();
        let rate_limit_key_limit = rate_limiter.preparation_key_limit();
        let rate_limiter_capability = rate_limiter.capability();
        Self {
            config: RuntimeConfig {
                runtime_endpoint_timeout: Some(Duration::from_secs(
                    DEFAULT_OPERATION_TIMEOUT_SECONDS,
                )),
                request_body_limit: nonzero_usize(DEFAULT_REQUEST_BODY_BYTES),
                response_body_limit: nonzero_usize(DEFAULT_RESPONSE_BODY_BYTES),
                stream_response_limit: nonzero_usize(DEFAULT_STREAM_RESPONSE_BYTES),
                stream_lifetime: Duration::from_secs(DEFAULT_STREAM_LIFETIME_SECONDS),
                stream_idle_timeout: Duration::from_secs(DEFAULT_STREAM_IDLE_SECONDS),
                stream_max_records: nonzero_usize(DEFAULT_STREAM_MAX_RECORDS),
                stream_frame_limit: nonzero_usize(DEFAULT_STREAM_FRAME_BYTES),
                status_body_limit: Some(nonzero_usize(DEFAULT_STATUS_BODY_BYTES)),
                provider_request_limit: nonzero_usize(DEFAULT_PROVIDER_REQUEST_BYTES),
                provider_response_limit: nonzero_usize(DEFAULT_PROVIDER_RESPONSE_BYTES),
                provider_max_sends: DEFAULT_PROVIDER_MAX_SENDS,
                provider_max_origins: DEFAULT_PROVIDER_MAX_ORIGINS,
                provider_timeout: Duration::from_secs(DEFAULT_PROVIDER_TIMEOUT_SECONDS),
                pagination_max_pages: NonZeroU32::new(DEFAULT_PAGINATION_MAX_PAGES)
                    .expect("the pagination page default is nonzero"),
                pagination_max_items: nonzero_usize(DEFAULT_PAGINATION_MAX_ITEMS),
                pagination_total_response_body_limit: nonzero_usize(
                    DEFAULT_PAGINATION_TOTAL_RESPONSE_BYTES,
                ),
                pagination_identity_limit: nonzero_usize(DEFAULT_PAGINATION_IDENTITY_BYTES),
                aggregate_max_sends: NonZeroU32::new(DEFAULT_TRAVERSAL_MAX_HANDOFFS)
                    .expect("the aggregate handoff default is nonzero"),
                auth_failure_cooldown_cap: Duration::from_secs(
                    DEFAULT_AUTH_FAILURE_COOLDOWN_SECONDS,
                ),
                rate_limit_cooldown_cap: Duration::from_secs(DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS),
                rate_limit_key_limit,
                rate_limiter,
                rate_limiter_capability,
                rate_limiter_selected: false,
                retry_policy: None,
                operation_observer: default_operation_observer(),
                wall_clock: default_wall_clock(),
                debug: DebugLevel::None,
            },
            limiter_selection: LimiterSelection::Fallback,
            provider_selection: ProviderSelection::default(),
            stream_selection: StreamSelection::default(),
            limit_selection: LimitSelection::default(),
        }
    }

    /// Sets the default Concord operation timeout inherited by runtime endpoints.
    ///
    /// Generated endpoints retain their effective declaration timeout; request-local
    /// timeout selection can replace and reset to that retained effective value.
    #[must_use]
    pub fn runtime_endpoint_timeout(mut self, duration: Duration) -> Self {
        self.config.runtime_endpoint_timeout = Some(duration);
        self
    }

    /// Sets the positive finite application request-body limit in bytes
    /// (16 MiB by default).
    ///
    /// Takes a primitive count (§20.4). Zero is recorded as invalid
    /// configuration and reported by the root build; it is never replaced
    /// silently by an earlier value.
    #[must_use]
    pub fn request_body_limit(mut self, bytes: usize) -> Self {
        match NonZeroUsize::new(bytes) {
            Some(bytes) => {
                self.config.request_body_limit = bytes;
                self.limit_selection.request_body_invalid = false;
            }
            None => self.limit_selection.request_body_invalid = true,
        }
        self
    }

    /// Sets the positive finite buffered response-body limit in bytes
    /// (16 MiB by default).
    ///
    /// Takes a primitive count (§20.4). A request's
    /// `CallOptions::cap_response_bytes` can only narrow it. Zero is recorded
    /// as invalid configuration and reported by the root build.
    #[must_use]
    pub fn response_body_limit(mut self, bytes: usize) -> Self {
        match NonZeroUsize::new(bytes) {
            Some(bytes) => {
                self.config.response_body_limit = bytes;
                self.limit_selection.response_body_invalid = false;
            }
            None => self.limit_selection.response_body_invalid = true,
        }
        self
    }

    /// Sets the positive finite cumulative decoded (post-decompression)
    /// byte limit of one managed stream (16 MiB by default).
    ///
    /// A request's `CallOptions::cap_response_bytes` can only narrow it. Zero
    /// is recorded as invalid configuration and reported by the root build;
    /// it is never replaced silently by an earlier value.
    #[must_use]
    pub fn stream_response_limit(mut self, bytes: usize) -> Self {
        match NonZeroUsize::new(bytes) {
            Some(bytes) => {
                self.config.stream_response_limit = bytes;
                self.stream_selection.response_limit_invalid = false;
            }
            None => self.stream_selection.response_limit_invalid = true,
        }
        self
    }

    /// Sets the positive, representable total elapsed lease of one managed
    /// stream after response-head acquisition (5 minutes by default).
    ///
    /// The lease includes consumer pauses between polls and is intersected
    /// with `CallOptions::cap_stream_lifetime` and any whole-operation
    /// timeout cap. A long-lived feed requires an explicitly larger finite
    /// lease; there is no forever mode. Zero or unrepresentable durations are
    /// recorded as invalid configuration and reported by the root build.
    #[must_use]
    pub fn stream_lifetime(mut self, duration: Duration) -> Self {
        match crate::controls::validate_positive_duration(duration) {
            Ok(duration) => {
                self.config.stream_lifetime = duration;
                self.stream_selection.lifetime_invalid = false;
            }
            Err(_) => self.stream_selection.lifetime_invalid = true,
        }
        self
    }

    /// Sets the positive, representable cumulative idle limit of one managed
    /// stream (30 seconds by default).
    ///
    /// Idle counts only time while an underlying read is pending; nonempty
    /// data resets it, empty chunks do not, and application processing after
    /// a delivered item is never charged. It is intersected with
    /// `CallOptions::cap_stream_idle`. Zero or unrepresentable durations are
    /// recorded as invalid configuration and reported by the root build.
    #[must_use]
    pub fn stream_idle_timeout(mut self, duration: Duration) -> Self {
        match crate::controls::validate_positive_duration(duration) {
            Ok(duration) => {
                self.config.stream_idle_timeout = duration;
                self.stream_selection.idle_timeout_invalid = false;
            }
            Err(_) => self.stream_selection.idle_timeout_invalid = true,
        }
        self
    }

    /// Sets the positive maximum number of records one managed item stream
    /// yields (100,000 by default); the next record is withheld with a
    /// correlated error.
    ///
    /// Zero is recorded as invalid configuration and reported by the root
    /// build.
    #[must_use]
    pub fn stream_max_records(mut self, records: usize) -> Self {
        match NonZeroUsize::new(records) {
            Some(records) => {
                self.config.stream_max_records = records;
                self.stream_selection.max_records_invalid = false;
            }
            None => self.stream_selection.max_records_invalid = true,
        }
        self
    }

    /// Sets the positive maximum staged bytes of one framed stream record
    /// (1 MiB by default).
    ///
    /// A decoder's own stricter `max_buffered_bytes` is preserved: the
    /// effective per-record ceiling is the smaller of the two. Zero is
    /// recorded as invalid configuration and reported by the root build.
    #[must_use]
    pub fn stream_frame_limit(mut self, bytes: usize) -> Self {
        match NonZeroUsize::new(bytes) {
            Some(bytes) => {
                self.config.stream_frame_limit = bytes;
                self.stream_selection.frame_limit_invalid = false;
            }
            None => self.stream_selection.frame_limit_invalid = true,
        }
        self
    }

    /// Sets and enables the positive finite terminal status-body limit in
    /// bytes (64 KiB by default).
    ///
    /// Takes a primitive count (§20.4). Use
    /// [`Self::disable_status_body_capture`] rather than a zero limit; zero is
    /// recorded as invalid configuration and reported by the root build.
    #[must_use]
    pub fn status_body_limit(mut self, bytes: usize) -> Self {
        match NonZeroUsize::new(bytes) {
            Some(bytes) => {
                self.config.status_body_limit = Some(bytes);
                self.limit_selection.status_body_invalid = false;
            }
            None => self.limit_selection.status_body_invalid = true,
        }
        self
    }

    /// Disables terminal status-body capture without creating an unbounded
    /// mode. Being the explicit latest selection, it also supersedes an
    /// earlier invalid [`Self::status_body_limit`] selection.
    #[must_use]
    pub fn disable_status_body_capture(mut self) -> Self {
        self.config.status_body_limit = None;
        self.limit_selection.status_body_invalid = false;
        self
    }

    /// Sets the positive finite pagination page ceiling (1,000 by default),
    /// intersected with every requested page bound.
    ///
    /// Takes a primitive count (§20.4). Zero is recorded as invalid
    /// configuration and reported by the root build.
    #[must_use]
    pub fn pagination_max_pages(mut self, pages: u32) -> Self {
        match NonZeroU32::new(pages) {
            Some(pages) => {
                self.config.pagination_max_pages = pages;
                self.limit_selection.pagination_pages_invalid = false;
            }
            None => self.limit_selection.pagination_pages_invalid = true,
        }
        self
    }

    /// Sets the positive finite pagination item ceiling (100,000 by
    /// default).
    ///
    /// Takes a primitive count (§20.4). Zero is recorded as invalid
    /// configuration and reported by the root build.
    #[must_use]
    pub fn pagination_max_items(mut self, items: usize) -> Self {
        match NonZeroUsize::new(items) {
            Some(items) => {
                self.config.pagination_max_items = items;
                self.limit_selection.pagination_items_invalid = false;
            }
            None => self.limit_selection.pagination_items_invalid = true,
        }
        self
    }

    /// Sets the positive finite aggregate accepted buffered-response byte
    /// ceiling for one pagination operation.
    ///
    /// Concord charges the exact post-decompression body length of each
    /// successfully decoded page before retaining that page's items. This
    /// aggregate limit is independent of the per-page [`Self::response_body_limit`]
    /// and item-count limits.
    ///
    /// Takes a primitive count (§20.4). Zero is recorded as invalid
    /// configuration and reported by the root build.
    #[must_use]
    pub fn pagination_total_response_body_limit(mut self, bytes: usize) -> Self {
        match NonZeroUsize::new(bytes) {
            Some(bytes) => {
                self.config.pagination_total_response_body_limit = bytes;
                self.limit_selection.pagination_total_bytes_invalid = false;
            }
            None => self.limit_selection.pagination_total_bytes_invalid = true,
        }
        self
    }

    /// Sets the positive finite byte ceiling for complete canonical encoded
    /// pagination-identity vectors retained by cycle detection.
    ///
    /// The page ceiling separately bounds the number of retained identities;
    /// allocator and container overhead is not included in this byte total.
    ///
    /// Takes a primitive count (§20.4). Zero is recorded as invalid
    /// configuration and reported by the root build.
    #[must_use]
    pub fn pagination_identity_limit(mut self, bytes: usize) -> Self {
        match NonZeroUsize::new(bytes) {
            Some(bytes) => {
                self.config.pagination_identity_limit = bytes;
                self.limit_selection.pagination_identity_invalid = false;
            }
            None => self.limit_selection.pagination_identity_invalid = true,
        }
        self
    }

    /// Sets the positive finite application-handoff ceiling of one aggregate
    /// operation (2,000 by default).
    ///
    /// Every page attempt of a pagination traversal that is handed to the
    /// transport counts once; the ceiling is intersected with a per-call
    /// [`CallOptions::cap_sends`](crate::controls::CallOptions::cap_sends), which can
    /// only narrow it. Each direct request or page remains separately bounded
    /// by its own attempt ceiling (initial + 16 policy + 1 authentication).
    ///
    /// Takes a primitive count (§20.4). Zero is recorded as invalid
    /// configuration and reported by the root build.
    #[must_use]
    pub fn aggregate_max_sends(mut self, sends: u32) -> Self {
        match NonZeroU32::new(sends) {
            Some(sends) => {
                self.config.aggregate_max_sends = sends;
                self.limit_selection.aggregate_sends_invalid = false;
            }
            None => self.limit_selection.aggregate_sends_invalid = true,
        }
        self
    }

    /// Replaces the synchronous read-only observer used by the built client.
    #[must_use]
    pub fn operation_observer(mut self, observer: Arc<dyn OperationObserver>) -> Self {
        self.config.operation_observer = observer;
        self
    }

    #[cfg(test)]
    pub(crate) fn wall_clock(mut self, wall_clock: Arc<dyn WallClock>) -> Self {
        self.config.wall_clock = wall_clock;
        self
    }

    /// Replaces the built-in body-free debug presentation level.
    #[must_use]
    pub fn debug(mut self, level: DebugLevel) -> Self {
        self.config.debug = level;
        self
    }

    /// Sets the finite authentication failure cooldown cap, including zero.
    #[must_use]
    pub fn auth_failure_cooldown_cap(mut self, duration: Duration) -> Self {
        self.config.auth_failure_cooldown_cap = duration;
        self
    }

    /// Sets the finite server-directed rate-limit cooldown cap, including zero.
    #[must_use]
    pub fn rate_limit_cooldown_cap(mut self, duration: Duration) -> Self {
        self.config.rate_limit_cooldown_cap = duration;
        self
    }

    /// Replaces the immutable limiter shared by the built client and its clones.
    ///
    /// The limiter's [`RateLimiter::preparation_key_limit`] and
    /// [`RateLimiter::capability`] are snapshotted here, once; client build
    /// validates both.
    #[must_use]
    pub fn rate_limiter(mut self, limiter: Arc<dyn RateLimiter>) -> Self {
        self.config.rate_limit_key_limit = limiter.preparation_key_limit();
        self.config.rate_limiter_capability = limiter.capability();
        self.config.rate_limiter_selected = true;
        self.config.rate_limiter = limiter;
        self.limiter_selection = LimiterSelection::Explicit;
        self
    }

    /// Installs the explicit Concord-managed application retry policy.
    #[must_use]
    pub fn retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.config.retry_policy = Some(policy);
        self
    }

    /// Disables Concord-managed application retry.
    #[must_use]
    pub fn disable_retry_policy(mut self) -> Self {
        self.config.retry_policy = None;
        self
    }

    /// Sets the positive finite provider-helper request-body limit in bytes
    /// (1 MiB by default).
    ///
    /// Zero is recorded as invalid configuration and reported by the root
    /// build; it is never replaced silently by an earlier value.
    #[must_use]
    pub fn provider_request_limit(mut self, bytes: usize) -> Self {
        match NonZeroUsize::new(bytes) {
            Some(bytes) => {
                self.config.provider_request_limit = bytes;
                self.provider_selection.request_limit_invalid = false;
            }
            None => self.provider_selection.request_limit_invalid = true,
        }
        self
    }

    /// Sets the positive finite provider-helper response-body limit in bytes
    /// (1 MiB by default).
    ///
    /// Zero is recorded as invalid configuration and reported by the root
    /// build.
    #[must_use]
    pub fn provider_response_limit(mut self, bytes: usize) -> Self {
        match NonZeroUsize::new(bytes) {
            Some(bytes) => {
                self.config.provider_response_limit = bytes;
                self.provider_selection.response_limit_invalid = false;
            }
            None => self.provider_selection.response_limit_invalid = true,
        }
        self
    }

    /// Sets the provider-helper sends allowed per credential acquisition or
    /// refresh (4 by default).
    ///
    /// Accepted range is `1..=`[`MAX_PROVIDER_SENDS`](crate::contracts::MAX_PROVIDER_SENDS);
    /// other values are recorded as invalid configuration and reported by the
    /// root build.
    #[must_use]
    pub fn provider_max_sends(mut self, sends: u32) -> Self {
        if (1..=crate::limits::MAX_PROVIDER_SENDS).contains(&sends) {
            self.config.provider_max_sends = sends;
            self.provider_selection.max_sends_invalid = false;
        } else {
            self.provider_selection.max_sends_invalid = true;
        }
        self
    }

    /// Sets the capacity of each provider's declared helper origin set
    /// (8 by default).
    ///
    /// Accepted range is `1..=`[`MAX_PROVIDER_ORIGINS`](crate::contracts::MAX_PROVIDER_ORIGINS);
    /// other values are recorded as invalid configuration. Declared origins
    /// are validated against this capacity when the client is built.
    #[must_use]
    pub fn provider_max_origins(mut self, origins: usize) -> Self {
        if (1..=crate::limits::MAX_PROVIDER_ORIGINS).contains(&origins) {
            self.config.provider_max_origins = origins;
            self.provider_selection.max_origins_invalid = false;
        } else {
            self.provider_selection.max_origins_invalid = true;
        }
        self
    }

    /// Sets the positive, representable provider-helper ceiling per
    /// acquisition (30 seconds by default). The effective helper deadline is
    /// `min(remaining operation deadline, this ceiling)`.
    ///
    /// Zero or unrepresentable durations are recorded as invalid
    /// configuration and reported by the root build.
    #[must_use]
    pub fn provider_timeout(mut self, duration: Duration) -> Self {
        match crate::controls::validate_positive_duration(duration) {
            Ok(duration) => {
                self.config.provider_timeout = duration;
                self.provider_selection.timeout_invalid = false;
            }
            Err(_) => self.provider_selection.timeout_invalid = true,
        }
        self
    }

    /// Reports the first provider-helper setter whose latest selection was
    /// invalid.
    pub(crate) fn validate_provider_configuration(
        &self,
    ) -> Result<(), crate::error::ClientBuildError> {
        let selection = self.provider_selection;
        let invalid = [
            (selection.request_limit_invalid, "provider_request_limit"),
            (selection.response_limit_invalid, "provider_response_limit"),
            (selection.max_sends_invalid, "provider_max_sends"),
            (selection.max_origins_invalid, "provider_max_origins"),
            (selection.timeout_invalid, "provider_timeout"),
        ]
        .into_iter()
        .find_map(|(invalid, setting)| invalid.then_some(setting));
        match invalid {
            Some(setting) => Err(crate::error::ClientBuildError::invalid_runtime_setting(
                setting,
            )),
            None => Ok(()),
        }
    }

    /// Reports the first managed-stream setter whose latest selection was
    /// invalid.
    pub(crate) fn validate_stream_configuration(
        &self,
    ) -> Result<(), crate::error::ClientBuildError> {
        let selection = self.stream_selection;
        let invalid = [
            (selection.response_limit_invalid, "stream_response_limit"),
            (selection.lifetime_invalid, "stream_lifetime"),
            (selection.idle_timeout_invalid, "stream_idle_timeout"),
            (selection.max_records_invalid, "stream_max_records"),
            (selection.frame_limit_invalid, "stream_frame_limit"),
        ]
        .into_iter()
        .find_map(|(invalid, setting)| invalid.then_some(setting));
        match invalid {
            Some(setting) => Err(crate::error::ClientBuildError::invalid_runtime_setting(
                setting,
            )),
            None => Ok(()),
        }
    }

    /// Reports the first body/status/pagination count setter whose latest
    /// selection was invalid (§20.4 primitive count setters).
    pub(crate) fn validate_limit_configuration(
        &self,
    ) -> Result<(), crate::error::ClientBuildError> {
        let selection = self.limit_selection;
        let invalid = [
            (selection.request_body_invalid, "request_body_limit"),
            (selection.response_body_invalid, "response_body_limit"),
            (selection.status_body_invalid, "status_body_limit"),
            (selection.pagination_pages_invalid, "pagination_max_pages"),
            (selection.pagination_items_invalid, "pagination_max_items"),
            (
                selection.pagination_total_bytes_invalid,
                "pagination_total_response_body_limit",
            ),
            (
                selection.pagination_identity_invalid,
                "pagination_identity_limit",
            ),
            (selection.aggregate_sends_invalid, "aggregate_max_sends"),
        ]
        .into_iter()
        .find_map(|(invalid, setting)| invalid.then_some(setting));
        match invalid {
            Some(setting) => Err(crate::error::ClientBuildError::invalid_runtime_setting(
                setting,
            )),
            None => Ok(()),
        }
    }

    pub(crate) fn resolve(self) -> RuntimeConfig {
        self.config
    }

    /// Validates the selected limiter's snapshotted structural metadata: its
    /// preparation key limit and its capability descriptor.
    pub(crate) fn validate_rate_limit_key_limit(
        &self,
    ) -> Result<(), crate::error::ClientBuildError> {
        if self.config.rate_limit_key_limit > MAX_RATE_LIMIT_KEY_BYTES
            || self.config.rate_limiter_capability.validate().is_err()
        {
            Err(crate::error::ClientBuildError::new(
                crate::error::ClientBuildErrorKind::RuntimeConfiguration,
            ))
        } else {
            Ok(())
        }
    }

    pub(crate) fn prepare_generated_rate_limits(
        self,
        requires_limiter: bool,
    ) -> Result<Self, crate::error::ClientBuildError> {
        self.validate_rate_limit_key_limit()?;
        // Every v3 key has at least one dimension, so a limiter that admits
        // zero key bytes is explicitly incompatible with any declared policy.
        if requires_limiter
            && (self.limiter_selection == LimiterSelection::Fallback
                || self.config.rate_limit_key_limit == 0)
        {
            return Err(crate::error::ClientBuildError::new(
                crate::error::ClientBuildErrorKind::LimiterUnavailable,
            ));
        }
        Ok(self)
    }
}

fn nonzero_usize(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("normative runtime defaults are nonzero")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FirstObserver;
    impl OperationObserver for FirstObserver {}

    struct SecondObserver;
    impl OperationObserver for SecondObserver {}

    #[test]
    fn defaults_are_exact_positive_finite_values() {
        let config = RuntimeConfigBuilder::new().resolve();
        assert_eq!(
            config.runtime_endpoint_timeout,
            Some(Duration::from_secs(DEFAULT_OPERATION_TIMEOUT_SECONDS))
        );
        assert_eq!(config.request_body_limit.get(), DEFAULT_REQUEST_BODY_BYTES);
        assert_eq!(
            config.response_body_limit.get(),
            DEFAULT_RESPONSE_BODY_BYTES
        );
        assert_eq!(
            config.stream_response_limit.get(),
            DEFAULT_STREAM_RESPONSE_BYTES
        );
        assert_eq!(
            config.status_body_limit.map(NonZeroUsize::get),
            Some(DEFAULT_STATUS_BODY_BYTES)
        );
        assert_eq!(
            config.provider_request_limit.get(),
            DEFAULT_PROVIDER_REQUEST_BYTES
        );
        assert_eq!(
            config.provider_response_limit.get(),
            DEFAULT_PROVIDER_RESPONSE_BYTES
        );
        assert_eq!(config.provider_max_sends, DEFAULT_PROVIDER_MAX_SENDS);
        assert_eq!(config.provider_max_origins, DEFAULT_PROVIDER_MAX_ORIGINS);
        assert_eq!(
            config.provider_timeout,
            Duration::from_secs(DEFAULT_PROVIDER_TIMEOUT_SECONDS)
        );
        assert_eq!(
            config.pagination_max_pages.get(),
            DEFAULT_PAGINATION_MAX_PAGES
        );
        assert_eq!(
            config.pagination_max_items.get(),
            DEFAULT_PAGINATION_MAX_ITEMS
        );
        assert_eq!(
            config.pagination_total_response_body_limit.get(),
            DEFAULT_PAGINATION_TOTAL_RESPONSE_BYTES
        );
        assert_eq!(
            config.pagination_identity_limit.get(),
            DEFAULT_PAGINATION_IDENTITY_BYTES
        );
        assert_eq!(
            config.aggregate_max_sends.get(),
            DEFAULT_TRAVERSAL_MAX_HANDOFFS
        );
        assert_eq!(config.aggregate_max_sends.get(), 2_000);
        assert_eq!(
            config.auth_failure_cooldown_cap,
            Duration::from_secs(DEFAULT_AUTH_FAILURE_COOLDOWN_SECONDS)
        );
        assert_eq!(
            config.rate_limit_cooldown_cap,
            Duration::from_secs(DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS)
        );
        assert_eq!(
            config.rate_limit_key_limit,
            crate::rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES
        );
        assert_eq!(config.debug, DebugLevel::None);
        assert!(config.retry_policy.is_none());
        assert_eq!(
            config.stream_lifetime,
            Duration::from_secs(DEFAULT_STREAM_LIFETIME_SECONDS)
        );
        assert_eq!(config.stream_lifetime, Duration::from_secs(5 * 60));
        assert_eq!(
            config.stream_idle_timeout,
            Duration::from_secs(DEFAULT_STREAM_IDLE_SECONDS)
        );
        assert_eq!(config.stream_idle_timeout, Duration::from_secs(30));
        assert_eq!(config.stream_max_records.get(), DEFAULT_STREAM_MAX_RECORDS);
        assert_eq!(config.stream_max_records.get(), 100_000);
        assert_eq!(config.stream_frame_limit.get(), DEFAULT_STREAM_FRAME_BYTES);
        assert_eq!(config.stream_frame_limit.get(), 1_048_576);
        assert_eq!(config.stream_response_limit.get(), 16 * 1_048_576);
    }

    /// Regression (§20.4, §23.1): body, status and pagination count setters
    /// take primitive counts; zero is latched as invalid and reported by the
    /// root build instead of being replaced by an earlier valid value.
    #[test]
    fn limit_setters_take_primitives_and_report_invalid_latest_selection() {
        let builder = RuntimeConfigBuilder::new()
            .request_body_limit(11)
            .response_body_limit(8 * 1024 * 1024)
            .status_body_limit(13)
            .pagination_max_pages(100)
            .pagination_max_items(17)
            .pagination_total_response_body_limit(19)
            .pagination_identity_limit(23)
            .aggregate_max_sends(29);
        builder
            .validate_limit_configuration()
            .expect("valid primitive limit settings");
        let config = builder.resolve();
        assert_eq!(config.request_body_limit.get(), 11);
        assert_eq!(config.response_body_limit.get(), 8 * 1024 * 1024);
        assert_eq!(config.status_body_limit.map(NonZeroUsize::get), Some(13));
        assert_eq!(config.pagination_max_pages.get(), 100);
        assert_eq!(config.pagination_max_items.get(), 17);
        assert_eq!(config.pagination_total_response_body_limit.get(), 19);
        assert_eq!(config.pagination_identity_limit.get(), 23);
        assert_eq!(config.aggregate_max_sends.get(), 29);

        for (builder, setting) in [
            (
                RuntimeConfigBuilder::new().request_body_limit(0),
                "request_body_limit",
            ),
            (
                RuntimeConfigBuilder::new().response_body_limit(0),
                "response_body_limit",
            ),
            (
                RuntimeConfigBuilder::new().status_body_limit(0),
                "status_body_limit",
            ),
            (
                RuntimeConfigBuilder::new().pagination_max_pages(0),
                "pagination_max_pages",
            ),
            (
                RuntimeConfigBuilder::new().pagination_max_items(0),
                "pagination_max_items",
            ),
            (
                RuntimeConfigBuilder::new().pagination_total_response_body_limit(0),
                "pagination_total_response_body_limit",
            ),
            (
                RuntimeConfigBuilder::new().pagination_identity_limit(0),
                "pagination_identity_limit",
            ),
            (
                RuntimeConfigBuilder::new().aggregate_max_sends(0),
                "aggregate_max_sends",
            ),
            (
                RuntimeConfigBuilder::new()
                    .aggregate_max_sends(7)
                    .aggregate_max_sends(0),
                "aggregate_max_sends",
            ),
            (
                RuntimeConfigBuilder::new()
                    .response_body_limit(64)
                    .response_body_limit(0),
                "response_body_limit",
            ),
        ] {
            let error = builder.validate_limit_configuration().expect_err(setting);
            assert_eq!(
                error.kind(),
                crate::error::ClientBuildErrorKind::RuntimeConfiguration
            );
            assert_eq!(error.input_field(), Some(setting));
        }

        let corrected = RuntimeConfigBuilder::new()
            .pagination_max_pages(0)
            .pagination_max_pages(5);
        corrected
            .validate_limit_configuration()
            .expect("a later valid selection replaces the invalid one");
        assert_eq!(corrected.resolve().pagination_max_pages.get(), 5);

        let disabled = RuntimeConfigBuilder::new()
            .status_body_limit(0)
            .disable_status_body_capture();
        disabled
            .validate_limit_configuration()
            .expect("explicit disable is the latest status-body selection");
        assert!(disabled.resolve().status_body_limit.is_none());
    }

    #[test]
    fn stream_setters_take_primitives_and_report_invalid_latest_selection() {
        let builder = RuntimeConfigBuilder::new()
            .stream_response_limit(7)
            .stream_lifetime(Duration::from_secs(3_600))
            .stream_idle_timeout(Duration::from_secs(90))
            .stream_max_records(12)
            .stream_frame_limit(64);
        builder
            .validate_stream_configuration()
            .expect("valid stream settings");
        let config = builder.resolve();
        assert_eq!(config.stream_response_limit.get(), 7);
        assert_eq!(config.stream_lifetime, Duration::from_secs(3_600));
        assert_eq!(config.stream_idle_timeout, Duration::from_secs(90));
        assert_eq!(config.stream_max_records.get(), 12);
        assert_eq!(config.stream_frame_limit.get(), 64);

        for (builder, setting) in [
            (
                RuntimeConfigBuilder::new().stream_response_limit(0),
                "stream_response_limit",
            ),
            (
                RuntimeConfigBuilder::new().stream_lifetime(Duration::ZERO),
                "stream_lifetime",
            ),
            (
                RuntimeConfigBuilder::new().stream_lifetime(Duration::MAX),
                "stream_lifetime",
            ),
            (
                RuntimeConfigBuilder::new().stream_idle_timeout(Duration::ZERO),
                "stream_idle_timeout",
            ),
            (
                RuntimeConfigBuilder::new().stream_idle_timeout(Duration::MAX),
                "stream_idle_timeout",
            ),
            (
                RuntimeConfigBuilder::new().stream_max_records(0),
                "stream_max_records",
            ),
            (
                RuntimeConfigBuilder::new().stream_frame_limit(0),
                "stream_frame_limit",
            ),
            (
                RuntimeConfigBuilder::new()
                    .stream_lifetime(Duration::from_secs(9))
                    .stream_lifetime(Duration::ZERO),
                "stream_lifetime",
            ),
        ] {
            let error = builder.validate_stream_configuration().expect_err(setting);
            assert_eq!(
                error.kind(),
                crate::error::ClientBuildErrorKind::RuntimeConfiguration
            );
            assert_eq!(error.input_field(), Some(setting));
        }

        let replaced = RuntimeConfigBuilder::new()
            .stream_max_records(0)
            .stream_max_records(3);
        replaced
            .validate_stream_configuration()
            .expect("a later trusted selection replaces an earlier invalid one");
        let invalid_kept = RuntimeConfigBuilder::new()
            .stream_frame_limit(5)
            .stream_frame_limit(0);
        assert!(invalid_kept.validate_stream_configuration().is_err());
        assert_eq!(invalid_kept.resolve().stream_frame_limit.get(), 5);
        assert_eq!(replaced.resolve().stream_max_records.get(), 3);
    }

    #[test]
    fn selected_limiter_metadata_and_precedence_form_one_runtime_snapshot() {
        struct KeyBoundLimiter(usize);

        impl RateLimiter for KeyBoundLimiter {
            fn capability(&self) -> crate::rate_limit::LimiterCapability {
                crate::rate_limit::LimiterCapability::new(
                    crate::rate_limit::Enforcement::Local,
                    "test.key-bound",
                    u32::try_from(self.0).unwrap_or(u32::MAX),
                )
            }

            fn preparation_key_limit(&self) -> usize {
                self.0
            }

            fn acquire<'a>(
                &'a self,
                _request: crate::rate_limit::RateLimitRequest<'a>,
            ) -> crate::hooks::BoxFuture<
                'a,
                Result<crate::rate_limit::RateLimitPermit, crate::rate_limit::RateLimitError>,
            > {
                Box::pin(async { Ok(crate::rate_limit::RateLimitPermit::granted()) })
            }
        }

        for limit in [
            0,
            16,
            crate::rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES,
            2 * crate::rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES,
        ] {
            let limiter: Arc<dyn RateLimiter> = Arc::new(KeyBoundLimiter(limit));
            let config = RuntimeConfigBuilder::new()
                .rate_limiter(Arc::clone(&limiter))
                .resolve();
            assert_eq!(config.rate_limit_key_limit, limit);
            assert!(config.rate_limiter_selected);
            assert_eq!(
                config.rate_limiter_capability.algorithm_version(),
                u32::try_from(limit).expect("test limit")
            );
            assert!(Arc::ptr_eq(&config.rate_limiter, &limiter));
        }
        let unselected = RuntimeConfigBuilder::new().resolve();
        assert!(!unselected.rate_limiter_selected);
        assert_eq!(
            unselected.rate_limiter_capability,
            crate::rate_limit::FallbackRateLimiter::CAPABILITY
        );

        let explicit: Arc<dyn RateLimiter> = Arc::new(KeyBoundLimiter(23));
        let config = RuntimeConfigBuilder::new()
            .rate_limiter(Arc::clone(&explicit))
            .resolve();
        assert_eq!(config.rate_limit_key_limit, 23);
        assert!(Arc::ptr_eq(&config.rate_limiter, &explicit));
    }

    #[test]
    fn runtime_and_generated_limiter_build_failures_have_exact_kinds() {
        struct ExcessiveKeyLimiter;

        impl RateLimiter for ExcessiveKeyLimiter {
            fn capability(&self) -> crate::rate_limit::LimiterCapability {
                crate::rate_limit::LimiterCapability::new(
                    crate::rate_limit::Enforcement::Local,
                    "test.excessive",
                    1,
                )
            }

            fn preparation_key_limit(&self) -> usize {
                MAX_RATE_LIMIT_KEY_BYTES + 1
            }

            fn acquire<'a>(
                &'a self,
                _request: crate::rate_limit::RateLimitRequest<'a>,
            ) -> crate::hooks::BoxFuture<
                'a,
                Result<crate::rate_limit::RateLimitPermit, crate::rate_limit::RateLimitError>,
            > {
                Box::pin(async { Ok(crate::rate_limit::RateLimitPermit::granted()) })
            }
        }

        let runtime = RuntimeConfigBuilder::new().rate_limiter(Arc::new(ExcessiveKeyLimiter));
        let error = runtime
            .validate_rate_limit_key_limit()
            .expect_err("excessive limiter key bound");
        assert_eq!(
            error.kind(),
            crate::error::ClientBuildErrorKind::RuntimeConfiguration
        );
        assert!(std::error::Error::source(&error).is_none());

        let error = match RuntimeConfigBuilder::new().prepare_generated_rate_limits(true) {
            Ok(_) => panic!("generated rate limits require an installed limiter"),
            Err(error) => error,
        };
        assert_eq!(
            error.kind(),
            crate::error::ClientBuildErrorKind::LimiterUnavailable
        );
        assert!(std::error::Error::source(&error).is_none());

        struct CapabilityLimiter(crate::rate_limit::LimiterCapability, usize);

        impl RateLimiter for CapabilityLimiter {
            fn capability(&self) -> crate::rate_limit::LimiterCapability {
                self.0
            }

            fn preparation_key_limit(&self) -> usize {
                self.1
            }

            fn acquire<'a>(
                &'a self,
                _request: crate::rate_limit::RateLimitRequest<'a>,
            ) -> crate::hooks::BoxFuture<
                'a,
                Result<crate::rate_limit::RateLimitPermit, crate::rate_limit::RateLimitError>,
            > {
                Box::pin(async { Ok(crate::rate_limit::RateLimitPermit::granted()) })
            }
        }

        let overlong = "n".repeat(crate::rate_limit::MAX_RATE_LIMITER_NAMESPACE_BYTES + 1);
        let overlong: &'static str = Box::leak(overlong.into_boxed_str());
        for namespace in ["", "has space", "caf\u{e9}", overlong] {
            let runtime = RuntimeConfigBuilder::new().rate_limiter(Arc::new(CapabilityLimiter(
                crate::rate_limit::LimiterCapability::new(
                    crate::rate_limit::Enforcement::SharedPersistent,
                    namespace,
                    1,
                ),
                crate::rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES,
            )));
            let error = runtime
                .validate_rate_limit_key_limit()
                .expect_err("invalid limiter capability namespace");
            assert_eq!(
                error.kind(),
                crate::error::ClientBuildErrorKind::RuntimeConfiguration
            );
        }

        let zero_key = RuntimeConfigBuilder::new().rate_limiter(Arc::new(CapabilityLimiter(
            crate::rate_limit::LimiterCapability::new(
                crate::rate_limit::Enforcement::Local,
                "test.zero-key",
                1,
            ),
            0,
        )));
        zero_key
            .validate_rate_limit_key_limit()
            .expect("a zero key limit is structurally valid");
        let error = match zero_key.prepare_generated_rate_limits(true) {
            Ok(_) => panic!("declared policies cannot use a zero-key-byte limiter"),
            Err(error) => error,
        };
        assert_eq!(
            error.kind(),
            crate::error::ClientBuildErrorKind::LimiterUnavailable
        );
    }

    #[test]
    fn setters_preserve_positive_finite_types_and_zero_duration_caps() {
        let one = NonZeroUsize::MIN;
        let one_page = NonZeroU32::MIN;
        let config = RuntimeConfigBuilder::new()
            .runtime_endpoint_timeout(Duration::from_secs(59))
            .request_body_limit(1)
            .response_body_limit(1)
            .stream_response_limit(1)
            .status_body_limit(1)
            .pagination_max_pages(1)
            .pagination_max_items(1)
            .pagination_total_response_body_limit(1)
            .pagination_identity_limit(1)
            .operation_observer(Arc::new(crate::hooks::NoopOperationObserver))
            .debug(DebugLevel::Headers)
            .auth_failure_cooldown_cap(Duration::ZERO)
            .rate_limit_cooldown_cap(Duration::ZERO)
            .provider_request_limit(1)
            .provider_response_limit(1)
            .resolve();
        assert_eq!(
            config.runtime_endpoint_timeout,
            Some(Duration::from_secs(59))
        );
        assert_eq!(config.request_body_limit, one);
        assert_eq!(config.response_body_limit, one);
        assert_eq!(config.stream_response_limit, one);
        assert_eq!(config.status_body_limit, Some(one));
        assert_eq!(config.pagination_max_pages, one_page);
        assert_eq!(config.pagination_max_items, one);
        assert_eq!(config.pagination_total_response_body_limit, one);
        assert_eq!(config.pagination_identity_limit, one);
        assert_eq!(config.debug, DebugLevel::Headers);
        assert_eq!(config.auth_failure_cooldown_cap, Duration::ZERO);
        assert_eq!(config.rate_limit_cooldown_cap, Duration::ZERO);
        assert_eq!(config.provider_request_limit, one);
        assert_eq!(config.provider_response_limit, one);
    }

    #[test]
    fn provider_helper_setters_take_primitives_and_report_invalid_latest_selection() {
        let builder = RuntimeConfigBuilder::new()
            .provider_request_limit(2)
            .provider_response_limit(3)
            .provider_max_sends(crate::limits::MAX_PROVIDER_SENDS)
            .provider_max_origins(crate::limits::MAX_PROVIDER_ORIGINS)
            .provider_timeout(Duration::from_secs(7));
        builder
            .validate_provider_configuration()
            .expect("valid provider settings");
        let config = builder.resolve();
        assert_eq!(config.provider_request_limit.get(), 2);
        assert_eq!(config.provider_response_limit.get(), 3);
        assert_eq!(config.provider_max_sends, 64);
        assert_eq!(config.provider_max_origins, 32);
        assert_eq!(config.provider_timeout, Duration::from_secs(7));

        for (builder, setting) in [
            (
                RuntimeConfigBuilder::new().provider_request_limit(0),
                "provider_request_limit",
            ),
            (
                RuntimeConfigBuilder::new().provider_response_limit(0),
                "provider_response_limit",
            ),
            (
                RuntimeConfigBuilder::new().provider_max_sends(0),
                "provider_max_sends",
            ),
            (
                RuntimeConfigBuilder::new().provider_max_sends(65),
                "provider_max_sends",
            ),
            (
                RuntimeConfigBuilder::new().provider_max_origins(0),
                "provider_max_origins",
            ),
            (
                RuntimeConfigBuilder::new().provider_max_origins(33),
                "provider_max_origins",
            ),
            (
                RuntimeConfigBuilder::new().provider_timeout(Duration::ZERO),
                "provider_timeout",
            ),
            (
                RuntimeConfigBuilder::new().provider_timeout(Duration::MAX),
                "provider_timeout",
            ),
            (
                RuntimeConfigBuilder::new()
                    .provider_max_sends(8)
                    .provider_max_sends(0),
                "provider_max_sends",
            ),
        ] {
            let error = builder
                .validate_provider_configuration()
                .expect_err(setting);
            assert_eq!(
                error.kind(),
                crate::error::ClientBuildErrorKind::RuntimeConfiguration
            );
            assert_eq!(error.input_field(), Some(setting));
        }
        let replaced = RuntimeConfigBuilder::new()
            .provider_max_sends(0)
            .provider_max_sends(2);
        replaced
            .validate_provider_configuration()
            .expect("a later trusted selection replaces an earlier invalid one");
        assert_eq!(replaced.resolve().provider_max_sends, 2);
    }

    #[test]
    fn status_capture_disablement_is_none_and_finite_limit_reenables() {
        let disabled = RuntimeConfigBuilder::new()
            .disable_status_body_capture()
            .resolve();
        assert_eq!(disabled.status_body_limit, None);

        let enabled = RuntimeConfigBuilder::new()
            .disable_status_body_capture()
            .status_body_limit(1)
            .resolve();
        assert_eq!(enabled.status_body_limit, Some(NonZeroUsize::MIN));
    }

    #[test]
    fn runtime_endpoint_timeout_replacement_is_ordered() {
        let configured = RuntimeConfigBuilder::new()
            .runtime_endpoint_timeout(Duration::from_secs(11))
            .runtime_endpoint_timeout(Duration::from_secs(13))
            .resolve();
        assert_eq!(
            configured.runtime_endpoint_timeout,
            Some(Duration::from_secs(13))
        );
    }

    #[test]
    fn observer_and_debug_setters_replace_immutable_builder_selection() {
        let first: Arc<dyn OperationObserver> = Arc::new(FirstObserver);
        let second: Arc<dyn OperationObserver> = Arc::new(SecondObserver);
        let config = RuntimeConfigBuilder::new()
            .operation_observer(Arc::clone(&first))
            .debug(DebugLevel::Basic)
            .operation_observer(Arc::clone(&second))
            .debug(DebugLevel::Headers)
            .resolve();

        assert!(!Arc::ptr_eq(&config.operation_observer, &first));
        assert!(Arc::ptr_eq(&config.operation_observer, &second));
        assert_eq!(config.debug, DebugLevel::Headers);
    }

    #[test]
    fn retry_policy_is_opt_in_replaceable_and_disableable() {
        let first = RetryPolicy::builder()
            .max_retries_per_operation(1)
            .without_budget()
            .classify_fn(|_| crate::retry::RetryDecision::Stop)
            .build()
            .expect("first retry policy");
        let second = RetryPolicy::builder()
            .max_retries_per_operation(3)
            .without_budget()
            .classify_fn(|_| crate::retry::RetryDecision::Stop)
            .build()
            .expect("second retry policy");

        let configured = RuntimeConfigBuilder::new()
            .retry_policy(first)
            .retry_policy(second)
            .resolve();
        assert_eq!(
            configured
                .retry_policy
                .as_ref()
                .expect("retry policy")
                .max_retries_per_operation(),
            3
        );

        let disabled = RuntimeConfigBuilder::new()
            .retry_policy(
                RetryPolicy::builder()
                    .without_budget()
                    .classify_fn(|_| crate::retry::RetryDecision::Stop)
                    .build()
                    .expect("retry policy"),
            )
            .disable_retry_policy()
            .resolve();
        assert!(disabled.retry_policy.is_none());
    }
}
