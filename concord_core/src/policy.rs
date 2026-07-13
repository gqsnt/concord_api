use crate::rate_limit::RateLimitPlan;
use core::time::Duration;
use http::header::{ACCEPT, CONTENT_TYPE, HeaderName};
use http::{HeaderMap, HeaderValue};

pub mod feature;
pub mod resolved;
#[allow(unused_imports)]
pub use feature::FeatureUse;
#[allow(unused_imports)]
pub use resolved::ResolvedPolicy;

pub type PolicySnapshot = (
    HeaderMap,
    Vec<(String, String)>,
    Option<Duration>,
    RateLimitPlan,
);

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
    rate_limit: RateLimitPlan,
    // Current layer used for provenance decisions (not exposed in into_parts()).
    layer: PolicyLayer,

    // If endpoint policy explicitly sets OR removes Accept, runtime decoder injection must not override it.
    accept_explicit_by_endpoint: bool,
    accept_explicit_by_runtime: bool,
}

impl Policy {
    pub fn new() -> Self {
        Self {
            headers: HeaderMap::new(),
            query: Vec::new(),
            timeout: None,
            rate_limit: RateLimitPlan::new(),
            layer: PolicyLayer::Client,
            accept_explicit_by_endpoint: false,
            accept_explicit_by_runtime: false,
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

    #[inline]
    pub fn rate_limit(&self) -> &RateLimitPlan {
        &self.rate_limit
    }

    #[inline]
    pub fn add_rate_limit(&mut self, plan: RateLimitPlan) {
        self.rate_limit.extend(plan);
    }

    #[inline]
    pub fn replace_rate_limit(&mut self, plan: RateLimitPlan) {
        self.rate_limit = plan;
    }

    #[inline]
    pub fn clear_rate_limit(&mut self) {
        self.rate_limit = RateLimitPlan::new();
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
    pub fn ensure_accept(&mut self, value: HeaderValue) {
        if self.accept_explicit_by_endpoint || self.accept_explicit_by_runtime {
            return;
        }
        // Always override whatever was there (base/prefix/path), because decoder owns Accept.
        self.headers.insert(ACCEPT, value);
    }

    // ---------------- Query helpers ----------------

    /// Override-by-key: remove existing entries with same key, then insert.
    pub fn set_query(&mut self, key: &str, value: impl Into<String>) {
        self.replace_query_values(key, std::iter::once(value.into()));
    }

    /// Replace every query value for `key` with the supplied values in order.
    pub fn replace_query_values<I>(&mut self, key: &str, values: I)
    where
        I: IntoIterator<Item = String>,
    {
        self.remove_query(key);
        self.query
            .extend(values.into_iter().map(|value| (key.to_string(), value)));
    }

    /// Remove all entries matching `key`.
    pub fn remove_query(&mut self, key: &str) {
        self.query.retain(|(k, _)| k != key);
    }

    pub fn into_parts(self) -> PolicySnapshot {
        (self.headers, self.query, self.timeout, self.rate_limit)
    }
}

impl From<ResolvedPolicy> for Policy {
    fn from(resolved: ResolvedPolicy) -> Self {
        Self {
            headers: resolved.headers,
            query: resolved.query,
            timeout: resolved.timeout,
            rate_limit: resolved.rate_limit,
            layer: PolicyLayer::Runtime,
            accept_explicit_by_endpoint: true,
            accept_explicit_by_runtime: true,
        }
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
        p.ensure_accept(HeaderValue::from_static("application/json"));
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
        p.ensure_accept(HeaderValue::from_static("application/json"));
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
        p.ensure_accept(HeaderValue::from_static("application/json"));
        assert!(p.headers().get(ACCEPT).is_none());
    }

    #[test]
    fn query_replacement_and_remove_preserve_expected_order() {
        let mut p = Policy::new();
        p.set_query("q", "first");
        p.replace_query_values("tag", ["base".to_string(), "inherited".to_string()]);
        p.set_query("q", "override");
        p.replace_query_values("tag", ["endpoint".to_string(), "final".to_string()]);

        assert_eq!(
            p.query(),
            &[
                ("q".to_string(), "override".to_string()),
                ("tag".to_string(), "endpoint".to_string()),
                ("tag".to_string(), "final".to_string())
            ]
        );
    }

    #[test]
    fn query_remove_semantics_documented_and_tested() {
        let mut p = Policy::new();

        p.replace_query_values("dup", ["first".to_string(), "second".to_string()]);
        p.set_query("keep", "base");

        p.remove_query("missing");
        assert_eq!(
            p.query(),
            &[
                ("dup".to_string(), "first".to_string()),
                ("dup".to_string(), "second".to_string()),
                ("keep".to_string(), "base".to_string())
            ]
        );

        p.remove_query("dup");
        assert_eq!(p.query(), &[("keep".to_string(), "base".to_string())]);

        p.set_query("dup", "after-remove");
        assert_eq!(
            p.query(),
            &[
                ("keep".to_string(), "base".to_string()),
                ("dup".to_string(), "after-remove".to_string())
            ]
        );

        p.set_query("keep", "replace");
        assert_eq!(
            p.query(),
            &[
                ("dup".to_string(), "after-remove".to_string()),
                ("keep".to_string(), "replace".to_string())
            ]
        );

        p.replace_query_values("dup", ["shadow".to_string()]);
        p.remove_query("dup");
        assert_eq!(p.query(), &[("keep".to_string(), "replace".to_string())]);
    }

    #[test]
    fn query_replacement_supports_empty_values_and_empty_strings() {
        let mut p = Policy::new();
        p.replace_query_values("tags", ["a".to_string(), "b".to_string()]);
        p.replace_query_values("tags", std::iter::empty());
        p.set_query("empty", "");
        assert_eq!(p.query(), &[("empty".to_string(), "".to_string())]);
    }

    #[test]
    fn query_replacement_keeps_unrelated_key_order() {
        let mut p = Policy::new();
        p.set_query("first", "1");
        p.replace_query_values("target", ["old".to_string()]);
        p.set_query("last", "3");
        p.replace_query_values("target", ["a".to_string(), "b".to_string()]);
        assert_eq!(
            p.query(),
            &[
                ("first".to_string(), "1".to_string()),
                ("last".to_string(), "3".to_string()),
                ("target".to_string(), "a".to_string()),
                ("target".to_string(), "b".to_string()),
            ]
        );
    }

    #[test]
    fn header_override_and_remove_are_case_insensitive() {
        let mut p = Policy::new();
        p.insert_header(
            HeaderName::from_static("x-trace"),
            HeaderValue::from_static("one"),
        );
        p.insert_header(
            HeaderName::from_static("x-trace"),
            HeaderValue::from_static("two"),
        );

        assert_eq!(
            p.headers()
                .get(HeaderName::from_static("x-trace"))
                .unwrap()
                .to_str()
                .unwrap(),
            "two"
        );

        p.remove_header(HeaderName::from_static("x-trace"));
        assert!(!p.headers().contains_key(HeaderName::from_static("x-trace")));
    }
}
