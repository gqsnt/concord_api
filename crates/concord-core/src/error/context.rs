use std::error::Error;
use std::fmt;

use super::{ClientBuildErrorKind, ClientInputKind, ConfigurationErrorKind};

/// A safe client-construction failure.
pub struct ClientBuildError {
    kind: ClientBuildErrorKind,
    input_field: Option<&'static str>,
    input_kind: Option<ClientInputKind>,
}

impl ClientBuildError {
    #[allow(dead_code, reason = "used by later managed client construction")]
    pub(crate) const fn new(kind: ClientBuildErrorKind) -> Self {
        Self {
            kind,
            input_field: None,
            input_kind: None,
        }
    }

    pub(crate) const fn missing_input(field: &'static str, input_kind: ClientInputKind) -> Self {
        Self {
            kind: ClientBuildErrorKind::MissingInput,
            input_field: Some(field),
            input_kind: Some(input_kind),
        }
    }

    pub(crate) const fn invalid_credential(field: &'static str) -> Self {
        Self {
            kind: ClientBuildErrorKind::InvalidCredential,
            input_field: Some(field),
            input_kind: Some(ClientInputKind::Secret),
        }
    }

    /// Returns the stable construction failure kind.
    #[must_use]
    pub const fn kind(&self) -> ClientBuildErrorKind {
        self.kind
    }

    /// Returns the named generated builder input associated with this failure.
    ///
    /// This is a static Rust setter identifier selected by the declaration
    /// compiler. It never contains a supplied value or protected secret
    /// material.
    #[must_use]
    pub const fn input_field(&self) -> Option<&'static str> {
        self.input_field
    }

    /// Returns the semantic category of the associated generated input.
    #[must_use]
    pub const fn input_kind(&self) -> Option<ClientInputKind> {
        self.input_kind
    }
}

impl fmt::Display for ClientBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "client construction failed ({:?})", self.kind)?;
        if let (Some(field), Some(input_kind)) = (self.input_field, self.input_kind) {
            let reason = match self.kind {
                ClientBuildErrorKind::MissingInput => "missing",
                ClientBuildErrorKind::InvalidCredential => "invalid",
                _ => "associated",
            };
            write!(formatter, ": {reason} {input_kind:?} field `{field}`")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ClientBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientBuildError")
            .field("kind", &self.kind)
            .field("input_field", &self.input_field)
            .field("input_kind", &self.input_kind)
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

    fn exact_client_build_kind_index(kind: ClientBuildErrorKind) -> usize {
        match kind {
            ClientBuildErrorKind::MissingInput => 0,
            ClientBuildErrorKind::InvalidCredential => 1,
            ClientBuildErrorKind::InvalidBaseUrl => 2,
            ClientBuildErrorKind::HttpsRequired => 3,
            ClientBuildErrorKind::TlsUnavailable => 4,
            ClientBuildErrorKind::InvalidProxy => 5,
            ClientBuildErrorKind::InvalidRootCertificate => 6,
            ClientBuildErrorKind::InvalidClientIdentity => 7,
            ClientBuildErrorKind::HttpClientBuild => 8,
            ClientBuildErrorKind::RuntimeConfiguration => 9,
            ClientBuildErrorKind::LimiterUnavailable => 10,
            ClientBuildErrorKind::Internal => 11,
        }
    }

    #[test]
    fn missing_input_metadata_is_structured_and_never_formats_a_supplied_value() {
        const SENTINEL: &str = "protected-builder-value-sentinel-7f32";
        let error = ClientBuildError::missing_input("api_key", ClientInputKind::Secret);

        assert_eq!(error.kind(), ClientBuildErrorKind::MissingInput);
        assert_eq!(error.input_field(), Some("api_key"));
        assert_eq!(error.input_kind(), Some(ClientInputKind::Secret));
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(rendered.contains("api_key"));
            assert!(!rendered.contains(SENTINEL));
        }
        assert!(Error::source(&error).is_none());
    }

    #[test]
    fn every_client_build_kind_is_distinct_and_source_less() {
        let kinds = [
            ClientBuildErrorKind::MissingInput,
            ClientBuildErrorKind::InvalidCredential,
            ClientBuildErrorKind::InvalidBaseUrl,
            ClientBuildErrorKind::HttpsRequired,
            ClientBuildErrorKind::TlsUnavailable,
            ClientBuildErrorKind::InvalidProxy,
            ClientBuildErrorKind::InvalidRootCertificate,
            ClientBuildErrorKind::InvalidClientIdentity,
            ClientBuildErrorKind::HttpClientBuild,
            ClientBuildErrorKind::RuntimeConfiguration,
            ClientBuildErrorKind::LimiterUnavailable,
            ClientBuildErrorKind::Internal,
        ];

        for (index, kind) in kinds.iter().copied().enumerate() {
            assert_eq!(exact_client_build_kind_index(kind), index);
            let error = ClientBuildError::new(kind);
            assert_eq!(error.kind(), kind);
            assert_eq!(error.input_field(), None);
            assert_eq!(error.input_kind(), None);
            assert!(Error::source(&error).is_none());
            assert!(
                kinds[index + 1..]
                    .iter()
                    .all(|candidate| *candidate != kind),
                "client-build kinds must remain structurally distinct"
            );
        }

        let invalid = ClientBuildError::invalid_credential("api_key");
        assert_eq!(invalid.kind(), ClientBuildErrorKind::InvalidCredential);
        assert_eq!(invalid.input_field(), Some("api_key"));
        assert_eq!(invalid.input_kind(), Some(ClientInputKind::Secret));
        assert!(Error::source(&invalid).is_none());
    }
}
