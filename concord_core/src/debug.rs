use http::header::HeaderName;
use http::{HeaderMap, HeaderValue as HttpHeaderValue, Method, StatusCode};
use std::borrow::Cow;
use std::fmt;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
#[derive(Default)]
pub enum DebugLevel {
    #[default]
    None = 0,
    V = 1,
    VV = 2,
}

impl DebugLevel {
    #[inline]
    pub fn is_enabled(self) -> bool {
        self != DebugLevel::None
    }

    #[inline]
    pub fn is_verbose(self) -> bool {
        self >= DebugLevel::V
    }

    #[inline]
    pub fn is_very_verbose(self) -> bool {
        self >= DebugLevel::VV
    }
}

impl core::fmt::Display for DebugLevel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DebugLevel::None => f.write_str("none"),
            DebugLevel::V => f.write_str("v"),
            DebugLevel::VV => f.write_str("vv"),
        }
    }
}

pub trait DebugSink: Send + Sync + 'static {
    fn request_start(
        &self,
        dbg: DebugLevel,
        method: &Method,
        url: &str,
        endpoint: &'static str,
        page_index: u32,
    );
    fn request_headers(&self, dbg: DebugLevel, headers: SanitizedHeaders<'_>);

    fn response_status(&self, dbg: DebugLevel, status: StatusCode, url: &str, ok: bool);
    fn response_headers(&self, dbg: DebugLevel, headers: SanitizedHeaders<'_>);
}

#[derive(Default)]
pub struct NoopDebugSink;
impl DebugSink for NoopDebugSink {
    #[inline]
    fn request_start(&self, _: DebugLevel, _: &Method, _: &str, _: &'static str, _: u32) {}
    #[inline]
    fn request_headers(&self, _: DebugLevel, _: SanitizedHeaders<'_>) {}
    #[inline]
    fn response_status(&self, _: DebugLevel, _: StatusCode, _: &str, _: bool) {}
    #[inline]
    fn response_headers(&self, _: DebugLevel, _: SanitizedHeaders<'_>) {}
}

/// Reproduit le comportement actuel (stderr).
pub struct StderrDebugSink;
impl DebugSink for StderrDebugSink {
    fn request_start(
        &self,
        dbg: DebugLevel,
        method: &Method,
        url: &str,
        endpoint: &'static str,
        page_index: u32,
    ) {
        if page_index == 0 {
            eprintln!("[client_api:{}] -> {} {} ({})", dbg, method, url, endpoint);
        } else {
            eprintln!(
                "[client_api:{}] -> {} {} ({}) page={}",
                dbg, method, url, endpoint, page_index
            );
        }
    }
    fn request_headers(&self, dbg: DebugLevel, headers: SanitizedHeaders<'_>) {
        eprintln!("[client_api:{}] request headers:", dbg);
        for (k, v) in headers.iter() {
            let vs = v.as_str();
            eprintln!("  {}: {}", k, vs);
        }
    }
    fn response_status(&self, dbg: DebugLevel, status: StatusCode, url: &str, ok: bool) {
        if ok {
            eprintln!("[client_api:{}] <- {} {} (ok)", dbg, status.as_u16(), url);
        } else {
            eprintln!(
                "[client_api:{}] <- {} {} (error)",
                dbg,
                status.as_u16(),
                url
            );
        }
    }
    fn response_headers(&self, dbg: DebugLevel, headers: SanitizedHeaders<'_>) {
        eprintln!("[client_api:{}] response headers:", dbg);
        for (k, v) in headers.iter() {
            let vs = v.as_str();
            eprintln!("  {}: {}", k, vs);
        }
    }
}

#[allow(dead_code)]
fn is_sensitive_header_name(name: &HeaderName) -> bool {
    crate::redaction::should_redact_header_name(name)
}

fn sanitized_header_value(name: &str, value: &HttpHeaderValue) -> SanitizedHeaderValue {
    if crate::redaction::is_sensitive_name(name) {
        SanitizedHeaderValue::redacted()
    } else {
        SanitizedHeaderValue::from_header_value(value)
    }
}

#[derive(Clone)]
pub struct SanitizedHeaderValue {
    value: Cow<'static, str>,
    redacted: bool,
}

impl SanitizedHeaderValue {
    fn from_header_value(value: &HttpHeaderValue) -> Self {
        Self {
            value: match value.to_str() {
                Ok(value) => Cow::Owned(value.to_owned()),
                Err(_) => Cow::Borrowed("<non-utf8>"),
            },
            redacted: false,
        }
    }

    fn redacted() -> Self {
        Self {
            value: Cow::Borrowed("<redacted>"),
            redacted: true,
        }
    }

    pub fn is_redacted(&self) -> bool {
        self.redacted
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for SanitizedHeaderValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for SanitizedHeaderValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone)]
pub struct SanitizedHeaders<'a> {
    headers: &'a HeaderMap,
}

impl<'a> SanitizedHeaders<'a> {
    pub fn new(headers: &'a HeaderMap) -> Self {
        Self { headers }
    }

    pub fn len(&self) -> usize {
        self.headers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }

    pub fn get<N>(&self, name: N) -> Option<SanitizedHeaderValue>
    where
        N: http::header::AsHeaderName + Clone,
    {
        let name_str = name.as_str();
        let value = self.headers.get(name.clone())?;
        Some(sanitized_header_value(name_str, value))
    }

    pub fn contains_key<N>(&self, name: N) -> bool
    where
        N: http::header::AsHeaderName,
    {
        self.headers.contains_key(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (HeaderName, SanitizedHeaderValue)> + '_ {
        self.headers
            .iter()
            .map(|(name, value)| (name.clone(), sanitized_header_value(name.as_str(), value)))
    }
}

impl fmt::Debug for SanitizedHeaders<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut map = f.debug_map();
        for (name, value) in self.headers {
            map.entry(name, &sanitized_header_value(name.as_str(), value));
        }
        map.finish()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use http::HeaderValue;
    use http::header::{ACCEPT, AUTHORIZATION, COOKIE};

    #[test]
    fn redaction_redacts_sensitive_headers_by_name() {
        assert!(is_sensitive_header_name(&AUTHORIZATION));
        assert!(is_sensitive_header_name(&COOKIE));
        assert!(is_sensitive_header_name(&HeaderName::from_static(
            "x-riot-token"
        )));
        assert!(is_sensitive_header_name(&HeaderName::from_static(
            "x-api-key"
        )));
        assert!(!is_sensitive_header_name(&ACCEPT));

        let secret = HeaderValue::from_static("s3cr3t");
        assert_eq!(
            sanitized_header_value(AUTHORIZATION.as_str(), &secret).as_str(),
            "<redacted>"
        );
        assert_eq!(
            sanitized_header_value(
                ACCEPT.as_str(),
                &HeaderValue::from_static("application/json")
            )
            .as_str(),
            "application/json"
        );
    }

    #[test]
    fn redaction_redacts_bearer_api_key_and_basic_headers_for_debug_output() {
        for (name, value) in [
            (
                AUTHORIZATION,
                HeaderValue::from_static("Bearer LEAK_SENTINEL_BEARER_456"),
            ),
            (
                AUTHORIZATION,
                HeaderValue::from_static("Basic dXNlcjpMRUFLX1NFTlRJTkVMX1BBU1NXT1JEXzc4OQ=="),
            ),
            (
                HeaderName::from_static("x-api-key"),
                HeaderValue::from_static("LEAK_SENTINEL_API_KEY_123"),
            ),
        ] {
            let rendered = sanitized_header_value(name.as_str(), &value);
            assert_eq!(rendered.as_str(), "<redacted>");
            assert!(!rendered.as_str().contains("LEAK_SENTINEL"));
        }
    }
}
