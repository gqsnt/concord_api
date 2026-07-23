use std::time::Duration;

use crate::hooks::DebugLevel;

/// Generated-private request-local operation-deadline selection.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeoutOverride {
    Inherit,
    Explicit(Duration),
    Disabled,
}

/// Generated-private request-local generic options.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestOptions {
    timeout: TimeoutOverride,
    debug: Option<DebugLevel>,
}

impl RequestOptions {
    #[doc(hidden)]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            timeout: TimeoutOverride::Inherit,
            debug: None,
        }
    }

    pub(crate) const fn with_timeout(mut self, timeout: TimeoutOverride) -> Self {
        self.timeout = timeout;
        self
    }

    pub(crate) const fn with_debug(mut self, debug: DebugLevel) -> Self {
        self.debug = Some(debug);
        self
    }

    pub(crate) const fn resolve_timeout(self, inherited: Option<Duration>) -> Option<Duration> {
        match self.timeout {
            TimeoutOverride::Inherit => inherited,
            TimeoutOverride::Explicit(duration) => Some(duration),
            TimeoutOverride::Disabled => None,
        }
    }

    pub(crate) const fn resolve_debug(self, inherited: DebugLevel) -> DebugLevel {
        match self.debug {
            Some(debug) => debug,
            None => inherited,
        }
    }
}

impl Default for RequestOptions {
    fn default() -> Self {
        Self::new()
    }
}
