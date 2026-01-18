use crate::codec::{self, Format};
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use http::header::{HeaderName, HeaderValue};

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
    fn request_headers(&self, dbg: DebugLevel, headers: &HeaderMap);
    fn request_body(&self, dbg: DebugLevel, body: &Bytes, format: Format, max_chars: usize);

    fn response_status(&self, dbg: DebugLevel, status: StatusCode, url: &str, ok: bool);
    fn response_headers(&self, dbg: DebugLevel, headers: &HeaderMap);
    fn response_body(&self, dbg: DebugLevel, body: &Bytes, format: Format, max_chars: usize);
    fn response_body_preview(
        &self,
        dbg: DebugLevel,
        headers: &HeaderMap,
        body: &Bytes,
        full_len: Option<usize>,
    );
}

#[derive(Default)]
pub struct NoopDebugSink;
impl DebugSink for NoopDebugSink {
    #[inline]
    fn request_start(&self, _: DebugLevel, _: &Method, _: &str, _: &'static str, _: u32) {}
    #[inline]
    fn request_headers(&self, _: DebugLevel, _: &HeaderMap) {}
    #[inline]
    fn request_body(&self, _: DebugLevel, _: &Bytes, _: Format, _: usize) {}
    #[inline]
    fn response_status(&self, _: DebugLevel, _: StatusCode, _: &str, _: bool) {}
    #[inline]
    fn response_headers(&self, _: DebugLevel, _: &HeaderMap) {}
    #[inline]
    fn response_body(&self, _: DebugLevel, _: &Bytes, _: Format, _: usize) {}
    #[inline]
    fn response_body_preview(&self, _: DebugLevel, _: &HeaderMap, _: &Bytes, _: Option<usize>) {}
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
    fn request_headers(&self, dbg: DebugLevel, headers: &HeaderMap) {
        eprintln!("[client_api:{}] request headers:", dbg);
        for (k, v) in headers.iter() {
            let vs = header_value_for_debug(k, v);
            eprintln!("  {}: {}", k, vs);
        }
    }
    fn request_body(&self, dbg: DebugLevel, body: &Bytes, format: Format, max_chars: usize) {
        let preview = codec::format_bytes_for_debug(format, body.as_ref(), max_chars);
        eprintln!(
            "[client_api:{}] request body ({} bytes): {}",
            dbg,
            body.len(),
            preview
        );
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
    fn response_headers(&self, dbg: DebugLevel, headers: &HeaderMap) {
        eprintln!("[client_api:{}] response headers:", dbg);
        for (k, v) in headers.iter() {
            let vs = header_value_for_debug(k, v);
            eprintln!("  {}: {}", k, vs);
        }
    }
    fn response_body(&self, dbg: DebugLevel, body: &Bytes, format: Format, max_chars: usize) {
        let preview = codec::format_bytes_for_debug(format, body.as_ref(), max_chars);
        eprintln!(
            "[client_api:{}] response body ({} bytes): {}",
            dbg,
            body.len(),
            preview
        );
    }
    fn response_body_preview(
        &self,
        dbg: DebugLevel,
        headers: &HeaderMap,
        body: &Bytes,
        full_len: Option<usize>,
    ) {
        let preview = crate::error::body_as_text(headers, body, full_len);
        eprintln!("[client_api:{}] response body preview: {}", dbg, preview);
    }


}

fn is_sensitive_header_name(name: &HeaderName) -> bool {
    // HeaderName::as_str() is normalized to lowercase.
    let n = name.as_str();
    matches!(n, "authorization" | "proxy-authorization" | "cookie" | "set-cookie")
        // Common vendor patterns
        || n.contains("token")
        || n.contains("secret")
        || n.contains("api-key")
        || n.contains("apikey")
        || n.ends_with("-key")
}

fn header_value_for_debug(name: &HeaderName, value: &HeaderValue) -> String {
    if is_sensitive_header_name(name) {
        "<redacted>".to_string()
    } else {
        value.to_str().unwrap_or("<non-utf8>").to_string()
    }
}



#[cfg(test)]
mod test {
    use super::*;
    use http::header::{ACCEPT, AUTHORIZATION, COOKIE};

    #[test]
    fn redacts_sensitive_headers_by_name() {
        assert!(is_sensitive_header_name(&AUTHORIZATION));
        assert!(is_sensitive_header_name(&COOKIE));
        assert!(is_sensitive_header_name(&HeaderName::from_static("x-riot-token")));
        assert!(is_sensitive_header_name(&HeaderName::from_static("x-api-key")));
        assert!(!is_sensitive_header_name(&ACCEPT));

        let secret = HeaderValue::from_static("s3cr3t");
        assert_eq!(header_value_for_debug(&AUTHORIZATION, &secret), "<redacted>");
        assert_eq!(
            header_value_for_debug(&ACCEPT, &HeaderValue::from_static("application/json")),
            "application/json"
        );
    }
}