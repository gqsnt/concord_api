use core::fmt;
use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer};

/// Minimal secret wrapper that never reveals its contents in Debug/Display.
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    #[inline]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Explicit "escape hatch" used by generated code to materialize the secret.
    #[inline]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<secret>")
    }
}
impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<secret>")
    }
}

// Broad conversion: allows String/&str and any type that can become a String.
impl<T: Into<String>> From<T> for SecretString {
    #[inline]
    fn from(v: T) -> Self {
        Self::new(v)
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SecretStringVisitor;

        impl<'de> Visitor<'de> for SecretStringVisitor {
            type Value = SecretString;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a secret string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                Ok(SecretString::new(value))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                Ok(SecretString::new(value))
            }
        }

        deserializer.deserialize_string(SecretStringVisitor)
    }
}
