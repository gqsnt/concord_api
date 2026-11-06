#![doc = include_str!("../README.md")]

mod api;
mod client;
mod error;
mod model;
mod upload;

pub use client::{ChangesPages, ClientConfig, DriveClient, FilePages};
pub use concord_pagination as pagination;
pub use error::{
    DocumentProblem, Error, ErrorKind, ProviderError, ProviderReason, Response, ResponseMeta,
};
pub use model::{
    Change, ChangesOptions, ChangesPage, ChangesPageMeta, File, FileField, FileFields, FilesPage,
    ListOptions, NewFile, PageToken, StartPageToken,
};
pub use upload::{CHUNK_ALIGNMENT, UploadCheckpoint, UploadSession, UploadStatus};
