mod client;
mod codec;
mod debug;
mod endpoint;
pub mod error;
mod pagination;
mod policy;
mod request;
mod secret;
mod timeout;
pub mod transport;
mod types;
mod auth;

pub mod internal {
    #[doc(hidden)]
    pub use crate::endpoint::{
        BodyPart, Chain, Decoded, Mapped, MappedResp, NoBody, NoPolicy, NoRoute, PolicyPart,
        ResponseSpec, RoutePart, Transform, TransformResp,
    };
    #[doc(hidden)]
    pub use crate::pagination::{
        Control, Controller, CursorPagination, HasNextCursor, NoController, NoPagination,
        OffsetLimitPagination, PagedPagination, PaginationPart, ProgressKey,
    };
}
pub mod prelude {
    pub use crate::client::{ApiClient, ClientContext};
    #[cfg(feature = "json")]
    pub use crate::codec::json::Json;
    pub use crate::codec::{NoContent, text::Text};
    pub use crate::debug::{DebugLevel, DebugSink, NoopDebugSink, StderrDebugSink};
    pub use crate::endpoint::Endpoint;
    pub use crate::error::{ApiClientError, ErrorContext, FxError};
    pub use crate::pagination::PaginatedEndpoint;
    pub use crate::pagination::{
        Caps, CursorPagination, HasNextCursor, OffsetLimitPagination, PageItems, PagedPagination,
        ProgressKey, Stop,
    };
    pub use crate::policy::{Policy, PolicyLayer, PolicyPatch};
    pub use crate::request::{PaginatedRequest, PendingRequest};
    pub use crate::secret::SecretString;
    pub use crate::timeout::TimeoutOverride;
    pub use crate::transport::{DecodedResponse, RequestMeta};
    pub use crate::transport::{ReqwestTransport, Transport};
    pub use crate::types::{HostLabelSource, HostParts as HostMap, HostSpec, RouteParts, UrlPath};
    pub use crate::auth::{
        AuthChain, AuthId, AuthOpResolved, AuthOpTemplate, AuthRuntime, AuthSlot, Authenticator,
        EffectiveAuth, SecretBytes, SecretProvider, TokenBundle, TokenFlow, ValueFmt, ValueTemplate,
    };
}
