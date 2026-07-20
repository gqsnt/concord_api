use std::error::Error;
use std::fmt;

use super::{ClientBuildErrorKind, ConfigurationErrorKind};

/// A safe client-construction failure.
pub struct ClientBuildError {
    kind: ClientBuildErrorKind,
}

impl ClientBuildError {
    #[allow(dead_code, reason = "used by later managed client construction")]
    pub(crate) const fn new(kind: ClientBuildErrorKind) -> Self {
        Self { kind }
    }

    /// Returns the stable construction failure kind.
    #[must_use]
    pub const fn kind(&self) -> ClientBuildErrorKind {
        self.kind
    }
}

impl fmt::Display for ClientBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "client construction failed ({:?})", self.kind)
    }
}

impl fmt::Debug for ClientBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientBuildError")
            .field("kind", &self.kind)
            .finish()
    }
}

impl Error for ClientBuildError {}

/// A safe reusable configuration-validation failure.
pub struct ConfigurationError {
    kind: ConfigurationErrorKind,
}

impl ConfigurationError {
    #[allow(dead_code, reason = "used by the managed configuration builders")]
    pub(crate) const fn new(kind: ConfigurationErrorKind) -> Self {
        Self { kind }
    }

    /// Returns the stable configuration failure kind.
    #[must_use]
    pub const fn kind(&self) -> ConfigurationErrorKind {
        self.kind
    }
}

impl fmt::Display for ConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "configuration is invalid ({:?})", self.kind)
    }
}

impl fmt::Debug for ConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigurationError")
            .field("kind", &self.kind)
            .finish()
    }
}

impl Error for ConfigurationError {}
