/// Immutable construction metadata produced for a generated endpoint.
///
/// This type is generated-private. It is inert and cannot execute a request.
#[doc(hidden)]
#[derive(Debug, Eq, PartialEq)]
pub struct RequestHead {
    endpoint: &'static str,
    method: &'static str,
    logical_url: String,
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
    scope_snapshot: Vec<(&'static str, String)>,
}

impl RequestHead {
    /// Constructs generated endpoint metadata from already resolved parts.
    #[doc(hidden)]
    #[must_use]
    pub fn new(
        endpoint: &'static str,
        method: &'static str,
        logical_url: String,
        query: Vec<(String, String)>,
        headers: Vec<(String, String)>,
        scope_snapshot: Vec<(&'static str, String)>,
    ) -> Self {
        Self {
            endpoint,
            method,
            logical_url,
            query,
            headers,
            scope_snapshot,
        }
    }

    /// Returns the stable DSL endpoint discriminator.
    #[doc(hidden)]
    #[must_use]
    pub const fn endpoint(&self) -> &'static str {
        self.endpoint
    }

    /// Returns the exact HTTP method token.
    #[doc(hidden)]
    #[must_use]
    pub const fn method(&self) -> &'static str {
        self.method
    }

    /// Returns the logical URL before any protected request processing.
    #[doc(hidden)]
    #[must_use]
    pub fn logical_url(&self) -> &str {
        &self.logical_url
    }

    /// Returns ordered public query pairs.
    #[doc(hidden)]
    #[must_use]
    pub fn query(&self) -> &[(String, String)] {
        &self.query
    }

    /// Returns ordered public headers.
    #[doc(hidden)]
    #[must_use]
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// Returns required outer scope values in constructor order.
    #[doc(hidden)]
    #[must_use]
    pub fn scope_snapshot(&self) -> &[(&'static str, String)] {
        &self.scope_snapshot
    }
}
