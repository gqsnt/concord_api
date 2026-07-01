use crate::error::FxError;
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{self, Display};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RateLimitErrorKind {
    InvalidConfiguration,
    InvalidKey,
    AcquireFailed,
    ResponseActionFailed,
    Internal,
}

#[derive(Debug)]
pub struct RateLimitError {
    pub kind: RateLimitErrorKind,
    pub message: Cow<'static, str>,
    source: Option<FxError>,
}

impl RateLimitError {
    #[inline]
    pub fn new(kind: RateLimitErrorKind, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            kind,
            message: message.into(),
            source: None,
        }
    }

    #[inline]
    pub fn with_source(mut self, source: impl Into<FxError>) -> Self {
        self.source = Some(source.into());
        self
    }

    #[inline]
    pub fn kind(&self) -> RateLimitErrorKind {
        self.kind
    }

    #[inline]
    pub fn message(&self) -> &str {
        self.message.as_ref()
    }
}

impl Display for RateLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.source {
            Some(source) => write!(f, "{:?}: {}: {}", self.kind, self.message, source),
            None => write!(f, "{:?}: {}", self.kind, self.message),
        }
    }
}

impl Error for RateLimitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}
