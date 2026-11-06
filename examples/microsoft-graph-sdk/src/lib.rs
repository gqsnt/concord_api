#![doc = include_str!("../README.md")]

mod api;
mod client;
mod error;
mod links;
mod model;
mod paging;

pub use client::{AuthContext, GraphClient};
/// Public traversal controls, completion errors and checkpoints from Concord's paging crate.
pub use concord_pagination as pagination;
pub use error::{
    ConfigurationProblem, ErrorBody, ErrorClass, ErrorDetail, GraphError, LinkProblem,
    ProtocolProblem, ProviderError,
};
pub use links::{DeltaLink, NextLink};
pub use model::{
    DirectoryObject, Field, Group, GroupField, MemberField, ObjectKind, Removed, UserDelta,
    UserField,
};
pub use paging::{GraphPage, GraphPages, Listing, PageMeta};
