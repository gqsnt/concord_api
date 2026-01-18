use core::fmt;

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
