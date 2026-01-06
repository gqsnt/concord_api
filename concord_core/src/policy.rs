use crate::error::ApiClientError;
use core::time::Duration;
use http::header::{ACCEPT, CONTENT_TYPE, HeaderName};
use http::{HeaderMap, HeaderValue};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
#[derive(Default)]
pub enum PolicyLayer {
    #[default]
    Client = 0,
    /// Reserved for future “outer → inner prefix/path” policy layers.
    PrefixPath = 1,
    Endpoint = 2,
    Runtime = 3,
}

#[derive(Default)]
pub struct Policy {
    headers: HeaderMap,
    query: Vec<(String, String)>,
    timeout: Option<Duration>,
    // Current layer used for provenance decisions (not exposed in into_parts()).
    layer: PolicyLayer,

    // If endpoint policy explicitly sets OR removes Accept, runtime decoder injection must not override it.
    accept_explicit_by_endpoint: bool,
}

impl Policy {
    pub fn new() -> Self {
        Self {
            headers: HeaderMap::new(),
            query: Vec::new(),
            timeout: None,
            layer: PolicyLayer::Client,
            accept_explicit_by_endpoint: false,
        }
    }

    #[inline]
    pub fn layer(&self) -> PolicyLayer {
        self.layer
    }

    #[inline]
    pub fn set_layer(&mut self, layer: PolicyLayer) {
        self.layer = layer;
    }

    #[inline]
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    #[inline]
    pub fn set_timeout(&mut self, d: Duration) {
        self.timeout = Some(d);
    }

    #[inline]
    pub fn clear_timeout(&mut self) {
        self.timeout = None;
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub fn query(&self) -> &[(String, String)] {
        &self.query
    }

    pub fn insert_header(&mut self, name: HeaderName, value: HeaderValue) {
        if self.layer == PolicyLayer::Endpoint && name == ACCEPT {
            self.accept_explicit_by_endpoint = true;
        }
        self.headers.insert(name, value);
    }

    pub fn remove_header(&mut self, name: HeaderName) {
        if self.layer == PolicyLayer::Endpoint && name == ACCEPT {
            self.accept_explicit_by_endpoint = true;
        }
        let _ = self.headers.remove(name);
    }

    pub fn has_content_type(&self) -> bool {
        self.headers.contains_key(CONTENT_TYPE)
    }

    /// Decoder-driven Accept injection:
    /// - Applied at runtime (after endpoint policy).
    /// - Overrides base/prefix/path Accept.
    /// - Does NOT override if endpoint policy explicitly set OR removed Accept.
    pub fn ensure_accept(&mut self, ct: &'static str) {
        if ct.is_empty() {
            return;
        }
        if self.accept_explicit_by_endpoint {
            return;
        }
        // Always override whatever was there (base/prefix/path), because decoder owns Accept.
        self.headers.insert(ACCEPT, HeaderValue::from_static(ct));
    }

    // ---------------- Query helpers ----------------

    /// Append (allow duplicates): current behavior.
    pub fn push_query(&mut self, key: &str, value: impl Into<String>) {
        self.query.push((key.to_string(), value.into()));
    }

    /// Override-by-key: remove existing entries with same key, then insert.
    pub fn set_query(&mut self, key: &str, value: impl Into<String>) {
        self.remove_query(key);
        self.query.push((key.to_string(), value.into()));
    }

    /// Remove all entries matching `key`.
    pub fn remove_query(&mut self, key: &str) {
        self.query.retain(|(k, _)| k != key);
    }

    pub fn into_parts(self) -> (HeaderMap, Vec<(String, String)>, Option<Duration>) {
        (self.headers, self.query, self.timeout)
    }
}

pub struct PolicyPatch<'a> {
    inner: &'a mut Policy,
}

impl<'a> PolicyPatch<'a> {
    #[inline]
    pub(crate) fn new(inner: &'a mut Policy) -> Self {
        Self { inner }
    }

    #[inline]
    pub fn set_header(
        &mut self,
        name: HeaderName,
        value: HeaderValue,
    ) -> Result<(), ApiClientError> {
        self.guard_accept(&name)?;
        self.inner.insert_header(name, value);
        Ok(())
    }

    #[inline]
    pub fn remove_header(&mut self, name: HeaderName) -> Result<(), ApiClientError> {
        self.guard_accept(&name)?;
        self.inner.remove_header(name);
        Ok(())
    }

    #[inline]
    pub fn push_query(&mut self, key: &str, value: impl Into<String>) {
        self.inner.push_query(key, value);
    }

    #[inline]
    pub fn set_query(&mut self, key: &str, value: impl Into<String>) {
        self.inner.set_query(key, value);
    }

    #[inline]
    pub fn remove_query(&mut self, key: &str) {
        self.inner.remove_query(key);
    }

    #[inline]
    pub fn set_timeout_override(&mut self, t: Option<Duration>) {
        self.inner.timeout = t;
    }

    fn guard_accept(&self, name: &HeaderName) -> Result<(), ApiClientError> {
        if self.inner.layer == PolicyLayer::Runtime
            && *name == ACCEPT
            && !self.inner.accept_explicit_by_endpoint
        {
            return Err(ApiClientError::PolicyViolation(
                "runtime cannot override Accept unless endpoint explicitly set/removed it",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use http::header::ACCEPT;

    #[test]
    fn runtime_accept_overrides_base_unless_endpoint_explicit() {
        // base Accept gets overridden by runtime decoder Accept
        let mut p = Policy::new();
        p.insert_header(ACCEPT, HeaderValue::from_static("text/plain"));
        p.set_layer(PolicyLayer::Runtime);
        p.ensure_accept("application/json");
        assert_eq!(
            p.headers().get(ACCEPT).unwrap().to_str().unwrap(),
            "application/json"
        );

        // endpoint explicit Accept prevents override
        let mut p = Policy::new();
        p.insert_header(ACCEPT, HeaderValue::from_static("text/plain"));
        p.set_layer(PolicyLayer::Endpoint);
        p.insert_header(ACCEPT, HeaderValue::from_static("application/custom"));
        p.set_layer(PolicyLayer::Runtime);
        p.ensure_accept("application/json");
        assert_eq!(
            p.headers().get(ACCEPT).unwrap().to_str().unwrap(),
            "application/custom"
        );

        // endpoint explicit removal prevents runtime injection
        let mut p = Policy::new();
        p.insert_header(ACCEPT, HeaderValue::from_static("text/plain"));
        p.set_layer(PolicyLayer::Endpoint);
        p.remove_header(ACCEPT);
        p.set_layer(PolicyLayer::Runtime);
        p.ensure_accept("application/json");
        assert!(p.headers().get(ACCEPT).is_none());
    }
}
