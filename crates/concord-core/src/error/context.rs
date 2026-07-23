use std::error::Error;
use std::fmt;

use super::{ClientBuildErrorKind, ClientInputKind, ConfigurationErrorKind};

/// A safe client-construction failure.
pub struct ClientBuildError {
    kind: ClientBuildErrorKind,
    missing_field: Option<&'static str>,
    missing_input_kind: Option<ClientInputKind>,
}

impl ClientBuildError {
    #[allow(dead_code, reason = "used by later managed client construction")]
    pub(crate) const fn new(kind: ClientBuildErrorKind) -> Self {
        Self {
            kind,
            missing_field: None,
            missing_input_kind: None,
        }
    }

    pub(crate) const fn missing_input(field: &'static str, input_kind: ClientInputKind) -> Self {
        Self {
            kind: ClientBuildErrorKind::MissingInput,
            missing_field: Some(field),
            missing_input_kind: Some(input_kind),
        }
    }

    /// Returns the stable construction failure kind.
    #[must_use]
    pub const fn kind(&self) -> ClientBuildErrorKind {
        self.kind
    }

    /// Returns the named generated builder input that is missing.
    ///
    /// This is a static Rust setter identifier selected by the declaration
    /// compiler. It never contains a supplied value or protected secret
    /// material.
    #[must_use]
    pub const fn missing_field(&self) -> Option<&'static str> {
        self.missing_field
    }

    /// Returns the semantic category of the missing generated input.
    #[must_use]
    pub const fn missing_input_kind(&self) -> Option<ClientInputKind> {
        self.missing_input_kind
    }
}

impl fmt::Display for ClientBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "client construction failed ({:?})", self.kind)?;
        if let (Some(field), Some(input_kind)) = (self.missing_field, self.missing_input_kind) {
            write!(formatter, ": missing {input_kind:?} field `{field}`")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ClientBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientBuildError")
            .field("kind", &self.kind)
            .field("missing_field", &self.missing_field)
            .field("missing_input_kind", &self.missing_input_kind)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_input_metadata_is_structured_and_never_formats_a_supplied_value() {
        const SENTINEL: &str = "protected-builder-value-sentinel-7f32";
        let error = ClientBuildError::missing_input("api_key", ClientInputKind::Secret);

        assert_eq!(error.kind(), ClientBuildErrorKind::MissingInput);
        assert_eq!(error.missing_field(), Some("api_key"));
        assert_eq!(error.missing_input_kind(), Some(ClientInputKind::Secret));
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(rendered.contains("api_key"));
            assert!(!rendered.contains(SENTINEL));
        }
    }
}
