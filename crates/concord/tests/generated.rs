mod support;

mod generated {
    pub(crate) mod authentication;
    pub(crate) mod pagination;
    pub(crate) mod rate_limits;
    #[cfg(feature = "dangerous-raw-response")]
    pub(crate) mod raw_response;
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
#[cfg(feature = "dangerous-raw-response")]
pub(crate) use generated::raw_response::ObservedCodec;
#[cfg(feature = "multipart")]
pub(crate) use generated::shaping::{
    CLIENT_DEFAULT, DropProbe, ENDPOINT_DEFAULT, OPTIONAL_CLIENT_DEFAULT,
    OPTIONAL_ENDPOINT_DEFAULT, OPTIONAL_SCOPE_DEFAULT, OPTIONAL_VALUES_DEFAULT, SCOPE_DEFAULT,
    ShapeProbe,
};
