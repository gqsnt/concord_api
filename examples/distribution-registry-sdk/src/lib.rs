//! A CNCF Distribution registry HTTP API v2 slice written with `concord::api!`: discover
//! repositories and tags, pull a manifest and verify its blobs, push blobs in resumable chunks and
//! publish a manifest — with the registry's Bearer token challenges answered.
//!
//! **Scope.** `GET /v2/`, `GET /v2/_catalog`, `GET /v2/<name>/tags/list` (with `Link`
//! pagination), manifest `GET`/`PUT` for two image manifest media types, blob `HEAD`/`GET` with
//! digest and size verification, blob uploads (`POST`, ordered `PATCH` chunks, status `GET`,
//! completing `PUT ?digest=`), the documented error document, and the token authentication
//! spec's challenge → token → retry. Not covered: deletion and garbage collection, cross-repository
//! mounts, monolithic `POST ?digest=`, manifest lists / indexes and other manifest schemas,
//! foreign-layer URLs and blob redirects to other origins, credential helpers, refresh tokens and
//! OAuth2 `POST` token requests. `README.md` lists every limit.
//!
//! ```no_run
//! use std::num::NonZeroUsize;
//!
//! use concord_sdk_distribution_registry::concord::bytes::Bytes;
//! use concord_sdk_distribution_registry::{
//!     Descriptor, ImageManifest, ListOptions, ManifestMediaType, Reference, Registry,
//!     RepositoryName, TokenAuth,
//! };
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let registry = Registry::new(concord::reqwest::Client::new(), "https://registry.example.test")?
//!     .token_auth(TokenAuth::anonymous().trust_realm("https://auth.example.test")?);
//! let repository = registry.repository(&RepositoryName::new("acme/app")?)?;
//!
//! // Workflow A: tags, a manifest by tag with an explicit Accept, its blobs verified.
//! let tags = repository.tags(ListOptions::default()).items().collect_complete().await?;
//! let manifest = repository
//!     .manifest(&Reference::tag("v1")?, &[ManifestMediaType::OciV1])
//!     .await?;
//! for descriptor in manifest.image.blobs() {
//!     let bytes = repository.blob_for(descriptor).await?.read_to_end(descriptor.size).await?;
//!     assert_eq!(bytes.len() as u64, descriptor.size);
//! }
//!
//! // Workflow B: push a layer in chunks, then a manifest that refers to it.
//! let layer = Bytes::from_static(b"synthetic layer bytes");
//! let chunk = NonZeroUsize::new(8).unwrap();
//! let pushed = repository.push_blob(layer.clone(), chunk).await?;
//! let config = Bytes::from_static(b"{}");
//! repository.push_blob(config.clone(), chunk).await?;
//! let image = ImageManifest::new(
//!     ManifestMediaType::OciV1,
//!     Descriptor::of("application/vnd.oci.image.config.v1+json", &config),
//!     vec![Descriptor::new("application/vnd.oci.image.layer.v1.tar", layer.len() as u64, pushed.digest)],
//! );
//! repository.put_manifest(&Reference::tag("v2")?, &image.to_upload()?).await?;
//! # let _ = tags; Ok(()) }
//! ```
//!
//! **Layers.**
//!
//! - `api` (private): the three declarations (registry root, one repository, the token service).
//! - [`Registry`], [`Repository`]: construction, the Bearer challenge and retry ([`auth`]),
//!   `GET /v2/`.
//! - [`pages`]: catalog and tag listings, driven by `Link: rel="next"` exactly as sent.
//! - [`manifest`], [`blob`], [`upload`]: manifests, verified blob reads, upload sessions.
//! - [`digest`], [`reference`](mod@reference), [`meta`], [`error`]: inputs, response metadata and errors.
//!
//! The SDK performs no retry other than the one answer to a Bearer challenge, no rate-limit
//! waiting and no concurrency.

mod api;
pub mod auth;
pub mod blob;
pub mod client;
pub mod digest;
pub mod error;
pub mod manifest;
pub mod meta;
mod model;
pub mod pages;
pub mod reference;
pub mod upload;

/// The Concord version this SDK is built on, for matching on its error kinds.
pub use concord;

pub use auth::{BasicCredentials, Challenge, TokenAuth};
pub use blob::{BlobInfo, BlobReader};
pub use client::{ApiVersion, Registry, Repository, TOKEN_BODY_LIMIT};
pub use digest::{Algorithm, Digest};
pub use error::{
    ApiError, AuthError, AuthProblem, ERROR_BODY_LIMIT, ErrorBody, ErrorCode, ErrorDocument,
    ErrorEntry, InputError, IntegrityError, IntegrityProblem, ProtocolError, ProtocolProblem,
    RegistryError,
};
pub use manifest::{
    Descriptor, ImageManifest, MANIFEST_LIMIT, Manifest, ManifestMediaType, ManifestPushed,
    ManifestUpload,
};
pub use meta::ResponseMeta;
pub use pages::{CatalogPage, CatalogPages, ListOptions, NextLink, TagPage, TagPages};
pub use reference::{PageSize, Reference, RepositoryName, Tag};
pub use upload::{BlobPushed, BlobUpload, UploadProgress};

/// The README's examples, compiled as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;
