#![allow(dead_code)]

use crate::policy::ResolvedPolicy;
use crate::transport::RequestMeta;
use http::Method;
use std::borrow::Cow;

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
    pub path: Vec<(Cow<'static, str>, String)>,
    pub query: Vec<(Cow<'static, str>, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct RequestOverrides {
    pub timeout: Option<std::time::Duration>,
}

#[derive(Clone, Debug)]
pub struct RequestPlan {
    pub endpoint: &'static EndpointPlan,
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResolvedRoute {
    pub host: Vec<String>,
    pub path: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum BodyPlan {
    #[default]
    None,
    Encoded {
        content_type: &'static str,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResponsePlan {
    pub accept: &'static str,
    pub no_content: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PaginationPlan {
    pub kind: &'static str,
}
