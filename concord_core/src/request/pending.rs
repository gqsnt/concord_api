use std::time::Duration;

use super::{RequestOptions, TimeoutOverride};
use crate::client::ClientCore;
use crate::hooks::DebugLevel;

/// An opaque typed pending request owned independently of its generated facade.
pub struct Request<E> {
    #[allow(dead_code, reason = "consumed by the PR 2.3 executor")]
    pub(crate) core: ClientCore,
    #[allow(dead_code, reason = "consumed by the PR 2.3 executor")]
    pub(crate) endpoint: E,
    pub(crate) options: RequestOptions,
}

impl<E> Request<E> {
    pub(crate) fn new(core: ClientCore, endpoint: E) -> Self {
        Self {
            core,
            endpoint,
            options: RequestOptions::new(),
        }
    }

    #[must_use]
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.options = self
            .options
            .with_timeout(TimeoutOverride::Explicit(duration));
        self
    }

    #[must_use]
    pub fn disable_timeout(mut self) -> Self {
        self.options = self.options.with_timeout(TimeoutOverride::Disabled);
        self
    }

    #[must_use]
    pub fn inherit_timeout(mut self) -> Self {
        self.options = self.options.with_timeout(TimeoutOverride::Inherit);
        self
    }

    #[must_use]
    pub fn debug(mut self, level: DebugLevel) -> Self {
        self.options = self.options.with_debug(level);
        self
    }

    #[must_use]
    pub fn response(self) -> ResponseRequest<E> {
        ResponseRequest { request: self }
    }
}

/// An opaque pending request selecting successful response metadata.
pub struct ResponseRequest<E> {
    #[allow(dead_code, reason = "consumed by the PR 2.3 executor")]
    pub(crate) request: Request<E>,
}
