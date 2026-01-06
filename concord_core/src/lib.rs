#![allow(clippy::needless_return)]

mod client;
mod codec;
mod debug;
mod endpoint;
mod error;
mod http_transport;
mod pagination;
mod policy;
mod timeout;
mod transport;
mod types;

pub mod internal {
    #[doc(hidden)]
    pub use crate::endpoint::{
        BodyPart, Chain, Decoded, Mapped, NoBody, NoPolicy, NoRoute, PolicyPart, ResponseSpec,
        RoutePart, Transform,
    };
    #[doc(hidden)]
    pub use crate::pagination::{
        Control, Controller, ControllerBuild, ControllerValue, CursorPagination, HasNextCursor,
        NoController, NoPagination, OffsetLimitPagination, PagedPagination, PaginationPart,
        ProgressKey,
    };
}
pub mod prelude {
    pub use crate::client::{ApiClient, ClientContext};
    #[cfg(feature = "json")]
    pub use crate::codec::json::JsonEncoding;
    pub use crate::codec::{NoContentEncoding, text::TextEncoding};
    pub use crate::debug::DebugLevel;
    pub use crate::endpoint::Endpoint;
    pub use crate::error::{ApiClientError, FxError};
    pub use crate::pagination::{
        Caps, CollectAllItems, CollectAllItemsEndpoint, ControllerBuild, ControllerValue,
        CursorPagination, HasNextCursor, OffsetLimitPagination, PageItems, PagedPagination,
        ProgressKey, Stop,
    };
    pub use crate::policy::{Policy, PolicyLayer, PolicyPatch};
    pub use crate::timeout::TimeoutOverride;
    pub use crate::transport::{DecodedResponse, RequestMeta};
    pub use crate::transport::{ReqwestTransport, Transport};
    pub use crate::types::{HostLabelSource, HostParts as HostMap, HostSpec, RouteParts, UrlPath};
}
