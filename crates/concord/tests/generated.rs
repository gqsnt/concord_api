mod support;

mod generated {
    pub(crate) mod any_stream;
    pub(crate) mod authentication;
    #[cfg(feature = "records")]
    pub(crate) mod buffered_records;
    pub(crate) mod pagination;
    pub(crate) mod rate_limits;
    #[cfg(any(feature = "csv", feature = "ndjson"))]
    pub(crate) mod record_formats;
    pub(crate) mod response_bytes;
    #[cfg(feature = "multipart")]
    pub(crate) mod shaping;
    mod surface;
}

pub(crate) use generated::authentication::{BearerProvider, SessionCodec, SessionProvider};
pub(crate) use generated::pagination::{
    CountedOffset, CustomPageSpec, NOT_SYNC_MARKER, NonClonePageCodec, OneNonClonePage,
    ScopeBindingController, StablePolicy, VendorController, VendorPageCodec, ZeroController,
};
pub(crate) use generated::rate_limits::{DisplayOnly, NoDisplay, RouteHost};
pub(crate) use generated::response_bytes::{AcceptedMissing, ObservedCodec};
#[cfg(feature = "multipart")]
pub(crate) use generated::shaping::{
    CLIENT_DEFAULT, ClientTraitDefaultProbe, DropProbe, ENDPOINT_DEFAULT,
    EndpointTraitDefaultProbe, OPTIONAL_CLIENT_DEFAULT, OPTIONAL_ENDPOINT_DEFAULT,
    OPTIONAL_SCOPE_DEFAULT, OPTIONAL_VALUES_DEFAULT, SCOPE_DEFAULT, ScopeTraitDefaultProbe,
    ShapeProbe,
};
