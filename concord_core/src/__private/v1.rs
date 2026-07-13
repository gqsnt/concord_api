//! Version 1 of Concord's generated-code descriptor ABI.
//!
//! This surface is public only so macro expansions can refer to it across
//! crate boundaries. It is generated-only, unstable implementation
//! integration. It is intentionally not a transport, middleware, runtime
//! configuration, or general reflection API.

/// Compile-time identity for the generated-code ABI.
///
/// Macro output assigns [`MACRO_ABI`] to `MacroAbi<1>`. A macro expecting a
/// different ABI therefore fails with a type mismatch at the expansion site.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct MacroAbi<const VERSION: u32>;

/// Compatibility value referenced by every generated API module.
#[doc(hidden)]
pub const MACRO_ABI: MacroAbi<1> = MacroAbi;

/// A protocol scheme known without consulting generated client values.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginScheme {
    Http,
    Https,
}

/// Safe static origin metadata. `authority` is produced from validated DSL
/// host literals and contains no user information, credentials, or proxy data.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedOriginDescriptor {
    pub scheme: OriginScheme,
    pub authority: &'static str,
}

/// Static origin classification for an API.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiOriginDescriptor {
    FixedSingleOrigin(FixedOriginDescriptor),
    DynamicOrigin,
    MultiOrigin,
}

/// Static origin relationship for one endpoint.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointOriginDescriptor {
    Fixed(FixedOriginDescriptor),
    Dynamic,
}

/// HTTP method identity stored without a runtime request value.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Head,
    Options,
    Patch,
}

impl HttpMethod {
    /// Adapt descriptor metadata to the current runtime method type.
    #[doc(hidden)]
    pub fn as_http_method(self) -> http::Method {
        match self {
            Self::Get => http::Method::GET,
            Self::Post => http::Method::POST,
            Self::Put => http::Method::PUT,
            Self::Delete => http::Method::DELETE,
            Self::Head => http::Method::HEAD,
            Self::Options => http::Method::OPTIONS,
            Self::Patch => http::Method::PATCH,
        }
    }
}

/// Request body contract resolved by the macro.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestBodyDescriptor {
    None,
    Buffered { codec: &'static str },
    Streaming { media: &'static str },
    Multipart,
}

/// Static request metadata; it never contains a body instance.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestDescriptor {
    pub body: RequestBodyDescriptor,
}

/// Response contract resolved by the macro.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseFormatDescriptor {
    Buffered { codec: &'static str },
    Bytes,
    NoContent,
    Streaming { media: &'static str },
}

/// Static response metadata; response processing remains in the current
/// runtime pipeline.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResponseDescriptor {
    pub format: ResponseFormatDescriptor,
}

/// One secret-free authentication requirement identity.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthRequirementDescriptor {
    pub credential: &'static str,
    pub usage_id: &'static str,
}

/// Static authentication metadata. Providers, credentials, and caches are
/// deliberately absent.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthDescriptor {
    pub requirements: &'static [AuthRequirementDescriptor],
}

/// Pagination facts known before runtime execution.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaginationDescriptor {
    pub can_change_origin: bool,
}

/// Static endpoint descriptor emitted once for every generated endpoint.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EndpointDescriptor {
    pub name: &'static str,
    pub api_name: &'static str,
    pub method: HttpMethod,
    pub origin: EndpointOriginDescriptor,
    pub request: RequestDescriptor,
    pub response: ResponseDescriptor,
    pub auth: AuthDescriptor,
    pub pagination: Option<PaginationDescriptor>,
}

/// Static API descriptor emitted once for every generated API.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApiDescriptor {
    pub name: &'static str,
    pub origin: ApiOriginDescriptor,
    pub endpoints: &'static [&'static EndpointDescriptor],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_method_adapter_is_metadata_only() {
        assert_eq!(HttpMethod::Get.as_http_method(), http::Method::GET);
        let _: MacroAbi<1> = MACRO_ABI;
    }
}
