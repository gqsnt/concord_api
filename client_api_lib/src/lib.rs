#![allow(clippy::needless_return)]

mod client;
mod codec;
mod endpoint;
mod error;
mod policy;
mod types;

pub mod internal {
    #[doc(hidden)]
    pub use crate::endpoint::{
        BodyPart, Chain, Decoded, Mapped, NoBody, NoPolicy, NoRoute, PolicyPart, ResponseSpec,
        RoutePart, Transform,
    };
}
pub mod prelude {
    pub use crate::client::{ApiClient, ClientContext};
    #[cfg(feature = "json")]
    pub use crate::codec::json::JsonEncoding;
    pub use crate::codec::{NoContentEncoding, text::TextEncoding};
    pub use crate::endpoint::Endpoint;
    pub use crate::error::{ApiClientError, BuildError, FxError};
    pub use crate::policy::Policy;
    pub use crate::types::{HostMap, RouteParts, UrlPath};
}
