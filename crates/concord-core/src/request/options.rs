use std::time::Duration;

use crate::controls::{
    CallOptions, CancellationToken, ControlError, ControlSummary, RequestControlState,
};
use crate::hooks::DebugLevel;

/// Generated-private request-local operation-deadline selection.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeoutOverride {
    Inherit,
    Explicit(Duration),
}

/// Generated-private request-local generic options.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct RequestOptions {
    timeout: TimeoutOverride,
    debug: Option<DebugLevel>,
    controls: RequestControlState,
}

impl RequestOptions {
    #[doc(hidden)]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            timeout: TimeoutOverride::Inherit,
            debug: None,
            controls: RequestControlState::new(),
        }
    }

    /// Returns the caller control state (selected timeout, caps, cancellation).
    #[must_use]
    pub const fn controls(&self) -> &RequestControlState {
        &self.controls
    }

    /// Replaces the selected finite timeout preference; caps are retained.
    pub fn select_timeout(&mut self, duration: Duration) -> Result<(), ControlError> {
        self.controls.set_timeout(duration)?;
        self.timeout = TimeoutOverride::Explicit(duration);
        Ok(())
    }

    /// Restores the declared timeout; caps are retained.
    pub fn reset_selected_timeout(&mut self) {
        self.controls.reset_timeout();
        self.timeout = TimeoutOverride::Inherit;
    }

    /// Intersects additional caps.
    pub fn apply_call_options(&mut self, options: &CallOptions) {
        self.controls.apply_options(options);
    }

    /// Attaches a cancellation signal.
    pub fn add_cancellation(&mut self, token: CancellationToken) -> Result<(), ControlError> {
        self.controls.add_cancellation(token)
    }

    /// Returns the value-free control summary for a declared timeout.
    #[must_use]
    pub fn control_summary(&self, declared: Option<Duration>) -> ControlSummary {
        self.controls.summary(declared)
    }

    pub(crate) fn with_debug(mut self, debug: DebugLevel) -> Self {
        self.debug = Some(debug);
        self
    }

    /// Resolves the enforced timeout: the selected (or inherited) timeout
    /// narrowed by any whole-operation cap.
    pub(crate) fn resolve_timeout(&self, inherited: Option<Duration>) -> Option<Duration> {
        let selected = match self.timeout {
            TimeoutOverride::Inherit => inherited,
            TimeoutOverride::Explicit(duration) => Some(duration),
        };
        match (selected, self.controls.caps().timeout_cap()) {
            (Some(selected), Some(cap)) => Some(selected.min(cap)),
            (None, Some(cap)) => Some(cap),
            (selected, None) => selected,
        }
    }

    pub(crate) const fn resolve_debug(&self, inherited: DebugLevel) -> DebugLevel {
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
