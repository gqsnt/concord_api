#![allow(dead_code)]

use crate::error::{ApiClientError, ErrorContext};
use crate::policy::ResolvedPolicy;
use crate::transport::RequestMeta;
use bytes::Bytes;
use http::Method;
use std::any::Any;
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
    pub body: BodyPlan,
    pub response: ResponsePlan,
    pub pagination: Option<PaginationPlan>,
}

#[derive(Clone, Debug, Default)]
pub struct RequestArgs {
    pub body: Option<Bytes>,
}

#[derive(Clone, Debug, Default)]
pub struct RequestOverrides {
    pub timeout: Option<std::time::Duration>,
    pub attempt: u32,
    pub page_index: u32,
    pub cache_mode: crate::cache::CacheRequestMode,
}

#[derive(Clone, Debug)]
pub struct RequestPlan {
    pub endpoint: EndpointPlan,
    pub args: RequestArgs,
    pub overrides: RequestOverrides,
}

#[derive(Clone, Debug, Default)]
pub struct AttemptState {
    pub attempt: u32,
    pub page_index: u32,
    pub auth_attempt: crate::auth::AuthAttemptSummary,
    pub cache_revalidation: Option<crate::cache::CacheRevalidation>,
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum BodyPlan {
    #[default]
    None,
    Encoded {
        content_type: &'static str,
        format: crate::codec::Format,
    },
}

pub type PlanDecodeFn = fn(
    crate::transport::BuiltResponse,
    ErrorContext,
) -> Result<Box<dyn Any + Send>, ApiClientError>;

#[derive(Clone)]
pub struct ResponsePlan {
    pub accept: &'static str,
    pub no_content: bool,
    pub format: crate::codec::Format,
    pub decode: PlanDecodeFn,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaginationPlan {
    OffsetLimit {
        offset_key: String,
        limit_key: String,
        offset: u64,
        limit: u64,
        stop_on_short_page: bool,
        stop: crate::pagination::Stop,
    },
    Cursor {
        cursor_key: String,
        per_page_key: String,
        cursor: Option<String>,
        per_page: u64,
        send_cursor_on_first: bool,
        stop_when_cursor_missing: bool,
        stop: crate::pagination::Stop,
    },
    Paged {
        page_key: String,
        per_page_key: String,
        page: u64,
        per_page: u64,
        stop_on_short_page: bool,
        stop: crate::pagination::Stop,
    },
}
