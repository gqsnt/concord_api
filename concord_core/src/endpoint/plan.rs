#![allow(dead_code)]

use crate::policy::ResolvedPolicy;
use crate::transport::RequestMeta;
use http::HeaderValue;
use http::Method;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointMeta {
    pub name: &'static str,
    pub method: Method,
    pub idempotent: bool,
    pub facade_path: &'static [&'static str],
}

impl EndpointMeta {
    #[inline]
    pub fn request_meta(&self, attempt: u32, page_index: u32) -> RequestMeta {
        RequestMeta {
            endpoint: self.name,
            method: self.method.clone(),
            idempotent: self.idempotent,
            attempt,
            page_index,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EndpointPlan {
    pub meta: EndpointMeta,
    pub route: ResolvedRoute,
    pub policy: ResolvedPolicy,
    pub response: ResponsePlan,
    pub pagination: Option<PaginationMarker>,
}

#[derive(Clone, Debug, Default)]
pub struct RequestOverrides {
    pub debug_level: Option<crate::debug::DebugLevel>,
    pub timeout: Option<std::time::Duration>,
    pub attempt: u32,
    pub page_index: u32,
}

#[derive(Debug)]
pub struct RequestPlan {
    pub endpoint: EndpointPlan,
    pub body: crate::io::PreparedBody,
    pub overrides: RequestOverrides,
}

#[derive(Clone, Debug)]
pub struct RequestPlanView {
    pub endpoint: EndpointPlan,
    pub overrides: RequestOverrides,
}

#[derive(Clone, Debug, Default)]
pub struct AttemptState {
    pub attempt: u32,
    pub page_index: u32,
    pub auth_attempt: crate::auth::AuthAttemptSummary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedRoute {
    pub scheme: http::uri::Scheme,
    pub host: String,
    pub path: String,
}

impl Default for ResolvedRoute {
    fn default() -> Self {
        Self {
            scheme: http::uri::Scheme::HTTPS,
            host: String::new(),
            path: "/".to_string(),
        }
    }
}

impl ResolvedRoute {
    pub fn new(
        scheme: http::uri::Scheme,
        host: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            scheme,
            host: host.into(),
            path: path.into(),
        }
    }
}

#[derive(Clone)]
pub struct ResponsePlan {
    pub accept: Option<HeaderValue>,
    pub no_content: bool,
    pub format: crate::codec::Format,
}

impl fmt::Debug for ResponsePlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponsePlan")
            .field("accept", &self.accept)
            .field("no_content", &self.no_content)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PaginationMarker;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_marker_is_presence_only() {
        let marker = PaginationMarker;
        let debug = format!("{marker:?}");
        assert_eq!(marker, PaginationMarker);
        assert_eq!(PaginationMarker, PaginationMarker);
        assert!(debug.contains("PaginationMarker"));
    }
}
