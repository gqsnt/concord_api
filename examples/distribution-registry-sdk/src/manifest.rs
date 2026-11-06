//! Image manifests: `GET` with an explicit `Accept`, and `PUT`.
//!
//! **Media types.** Two image manifest types, each a `config` descriptor and `layers`
//! descriptors: Docker's image manifest schema 2
//! (`application/vnd.docker.distribution.manifest.v2+json`) and the OCI image manifest
//! (`application/vnd.oci.image.manifest.v1+json`). Manifest lists / image indexes, schema 1 and
//! artifact manifests are out of scope: they are never put in `Accept`, and a response labelled
//! with a type the call did not accept is refused before its body is read
//! ([`ProtocolProblem::UnexpectedMediaType`]).
//!
//! **Verification.** The digest is computed over the exact bytes received. Fetched by digest,
//! the bytes must have that digest; fetched by tag, the `Docker-Content-Digest` header (when it
//! uses the same algorithm) must name them. A header with another algorithm is ignored, as the
//! spec allows. A push sends the exact bytes whose digest the SDK computed, and the registry's
//! `Docker-Content-Digest` must name them.

use std::collections::BTreeMap;

use concord::EncodedBody;
use concord::bytes::Bytes;
use concord::reqwest::StatusCode;
use concord::reqwest::header::{CONTENT_TYPE, HeaderValue};
use serde::{Deserialize, Serialize};

use crate::client::{Repository, drain, verbatim_last_segment};
use crate::digest::{Algorithm, Digest};
use crate::error::{InputError, IntegrityProblem, ProtocolProblem, RegistryError, expect_status};
use crate::meta::ResponseMeta;
use crate::reference::Reference;

/// The most bytes of a manifest the SDK reads (4 MiB, Distribution's default manifest limit).
pub const MANIFEST_LIMIT: usize = 4 * 1024 * 1024;

/// A supported image manifest media type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ManifestMediaType {
    /// `application/vnd.docker.distribution.manifest.v2+json` (image manifest schema 2).
    DockerV2,
    /// `application/vnd.oci.image.manifest.v1+json` (OCI image manifest).
    OciV1,
}

impl ManifestMediaType {
    /// The media type string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DockerV2 => "application/vnd.docker.distribution.manifest.v2+json",
            Self::OciV1 => "application/vnd.oci.image.manifest.v1+json",
        }
    }

    /// The type a media type string names (parameters ignored, case-insensitive).
    pub fn from_media_type(text: &str) -> Option<Self> {
        let essence = text.split(';').next().unwrap_or("").trim();
        [Self::DockerV2, Self::OciV1]
            .into_iter()
            .find(|candidate| candidate.as_str().eq_ignore_ascii_case(essence))
    }
}

/// A content descriptor: media type, size and digest of a blob.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Descriptor {
    /// The blob's media type.
    pub media_type: String,
    /// The blob's size in bytes.
    pub size: u64,
    /// The blob's digest.
    pub digest: Digest,
    /// Alternate download locations (foreign layers). Kept, never followed by this SDK.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<String>,
    /// Annotations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<BTreeMap<String, String>>,
}

impl Descriptor {
    /// A descriptor of `size` bytes with `digest`.
    pub fn new(media_type: impl Into<String>, size: u64, digest: Digest) -> Self {
        Self {
            media_type: media_type.into(),
            size,
            digest,
            urls: Vec::new(),
            annotations: None,
        }
    }

    /// The descriptor of `bytes` (sha256).
    pub fn of(media_type: impl Into<String>, bytes: &[u8]) -> Self {
        Self::new(media_type, bytes.len() as u64, Digest::sha256_of(bytes))
    }
}

/// An image manifest (schema version 2): a configuration and ordered layers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ImageManifest {
    /// Always 2.
    pub schema_version: u32,
    /// The manifest's media type (required by schema 2, recommended by OCI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// The configuration blob.
    pub config: Descriptor,
    /// The layer blobs, base first.
    pub layers: Vec<Descriptor>,
    /// Annotations (OCI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<BTreeMap<String, String>>,
}

impl ImageManifest {
    /// A schema-2 manifest of `media_type` over `config` and `layers`.
    pub fn new(media_type: ManifestMediaType, config: Descriptor, layers: Vec<Descriptor>) -> Self {
        Self {
            schema_version: 2,
            media_type: Some(media_type.as_str().to_owned()),
            config,
            layers,
            annotations: None,
        }
    }

    /// Every blob the manifest references: the configuration, then the layers.
    pub fn blobs(&self) -> impl Iterator<Item = &Descriptor> {
        std::iter::once(&self.config).chain(&self.layers)
    }

    /// The manifest serialized (compact JSON) for a push, with its digest. Its `mediaType` must
    /// name a supported type.
    ///
    /// # Errors
    ///
    /// [`InputError::Manifest`] without a supported `mediaType`.
    pub fn to_upload(&self) -> Result<ManifestUpload, InputError> {
        let media_type = self
            .media_type
            .as_deref()
            .and_then(ManifestMediaType::from_media_type)
            .ok_or(InputError::Manifest)?;
        let bytes = serde_json::to_vec(self).map_err(|_| InputError::Manifest)?;
        ManifestUpload::new(media_type, bytes)
    }
}

/// Manifest bytes ready to push, with their media type and digest.
#[derive(Clone, Debug)]
pub struct ManifestUpload {
    media_type: ManifestMediaType,
    bytes: Bytes,
    digest: Digest,
}

impl ManifestUpload {
    /// Exactly these bytes, which must be an image manifest whose `mediaType` (if present) is
    /// `media_type`. The digest is their sha256.
    ///
    /// # Errors
    ///
    /// [`InputError::Manifest`] otherwise.
    pub fn new(media_type: ManifestMediaType, bytes: impl Into<Bytes>) -> Result<Self, InputError> {
        let bytes = bytes.into();
        let parsed: ImageManifest =
            serde_json::from_slice(&bytes).map_err(|_| InputError::Manifest)?;
        let consistent = parsed.schema_version == 2
            && parsed.media_type.as_deref().is_none_or(|declared| {
                ManifestMediaType::from_media_type(declared) == Some(media_type)
            });
        if !consistent {
            return Err(InputError::Manifest);
        }
        let digest = Digest::sha256_of(&bytes);
        Ok(Self {
            media_type,
            bytes,
            digest,
        })
    }

    /// The media type sent as `Content-Type`.
    pub fn media_type(&self) -> ManifestMediaType {
        self.media_type
    }

    /// The bytes sent.
    pub fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    /// The sha256 digest of the bytes.
    pub fn digest(&self) -> &Digest {
        &self.digest
    }
}

/// A manifest as received: verified bytes, their media type and digest, and the parsed manifest.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Manifest {
    /// The media type of the response (one the call accepted).
    pub media_type: ManifestMediaType,
    /// The digest of [`bytes`](Self::bytes) (computed, then checked).
    pub digest: Digest,
    /// The exact bytes received.
    pub bytes: Bytes,
    /// The parsed manifest.
    pub image: ImageManifest,
    /// The response metadata.
    pub meta: ResponseMeta,
}

/// The answer to a manifest push.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ManifestPushed {
    /// The digest of the pushed bytes (the registry's `Docker-Content-Digest` agreed, if sent).
    pub digest: Digest,
    /// The response metadata (`Location` among the headers).
    pub meta: ResponseMeta,
}

impl Repository {
    /// `GET /v2/<name>/manifests/<reference>` with `Accept` listing `accept` in order.
    ///
    /// # Errors
    ///
    /// [`InputError::NoMediaType`] for an empty `accept`; an
    /// [`UnsupportedAlgorithm`](IntegrityProblem::UnsupportedAlgorithm) digest reference is
    /// refused before sending. Then a [`RegistryError`]: `Api` (`MANIFEST_UNKNOWN`, ...),
    /// `Protocol` (an unaccepted media type, a malformed manifest), `Integrity` (the bytes do
    /// not match the reference or the header digest), or `Request` (a body over
    /// [`MANIFEST_LIMIT`] is Concord's `Limit`).
    pub async fn manifest(
        &self,
        reference: &Reference,
        accept: &[ManifestMediaType],
    ) -> Result<Manifest, RegistryError> {
        if accept.is_empty() {
            return Err(InputError::NoMediaType.into());
        }
        let algorithm = match reference.as_digest() {
            Some(digest) => digest.supported_algorithm().ok_or_else(|| {
                RegistryError::integrity(IntegrityProblem::UnsupportedAlgorithm, None)
            })?,
            None => Algorithm::Sha256,
        };
        let list = accept
            .iter()
            .map(|media_type| media_type.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let accept_header = HeaderValue::try_from(list).expect("media type names are ASCII");
        let api = self.api.clone();
        let received = self
            .registry
            .execute(&self.pull(), |auth| {
                let prepared = api
                    .clone()
                    .maybe_authorization(auth)
                    .manifest(reference.clone(), &accept_header)
                    .prepare()?;
                match reference.as_digest() {
                    Some(digest) => verbatim_last_segment(prepared, digest.as_str()),
                    None => Ok(prepared),
                }
            })
            .await?;
        let (received, meta) = expect_status(received, StatusCode::OK).await?;
        let protocol = |problem| RegistryError::protocol(problem, Some(meta.clone()));
        // The head first: the media type must be one the call accepted, before the body is read.
        let mut types = meta.headers().get_all(CONTENT_TYPE).iter();
        let media_type = match (types.next(), types.next()) {
            (Some(value), None) => value
                .to_str()
                .ok()
                .and_then(ManifestMediaType::from_media_type)
                .filter(|media_type| accept.contains(media_type)),
            _ => None,
        }
        .ok_or_else(|| protocol(ProtocolProblem::UnexpectedMediaType))?;
        let header_digest = meta.digest_header().map_err(protocol)?;
        let bytes = received
            .decode_limited(MANIFEST_LIMIT)
            .await
            .map_err(|error| RegistryError::request(error, meta.clone()))?;
        let digest = Digest::compute(algorithm, &bytes);
        let integrity = |problem| RegistryError::integrity(problem, Some(meta.clone()));
        if let Some(expected) = reference.as_digest()
            && *expected != digest
        {
            return Err(integrity(IntegrityProblem::DigestMismatch {
                expected: expected.clone(),
                actual: digest,
            }));
        }
        if let Some(header) = header_digest
            && header.algorithm() == digest.algorithm()
            && header != digest
        {
            return Err(integrity(IntegrityProblem::HeaderDigestMismatch {
                expected: digest,
                header,
            }));
        }
        let image: ImageManifest = serde_json::from_slice(&bytes)
            .map_err(|_| protocol(ProtocolProblem::MalformedManifest))?;
        if image.schema_version != 2 {
            return Err(protocol(ProtocolProblem::MalformedManifest));
        }
        if let Some(declared) = &image.media_type
            && ManifestMediaType::from_media_type(declared) != Some(media_type)
        {
            return Err(protocol(ProtocolProblem::MediaTypeMismatch));
        }
        Ok(Manifest {
            media_type,
            digest,
            bytes,
            image,
            meta,
        })
    }

    /// `PUT /v2/<name>/manifests/<reference>` with the upload's bytes and media type; `201
    /// Created` is required. A missing referenced blob is the registry's `400` with `BLOB_UNKNOWN`
    /// entries ([`ErrorEntry::detail_digest`](crate::ErrorEntry::detail_digest) names each).
    ///
    /// # Errors
    ///
    /// [`InputError::ReferenceDigest`] when pushing by a digest the bytes do not have (nothing
    /// sent); then a [`RegistryError`] (`Integrity` when `Docker-Content-Digest` names other
    /// bytes).
    pub async fn put_manifest(
        &self,
        reference: &Reference,
        manifest: &ManifestUpload,
    ) -> Result<ManifestPushed, RegistryError> {
        if let Some(digest) = reference.as_digest()
            && digest != manifest.digest()
        {
            return Err(InputError::ReferenceDigest.into());
        }
        let content_type = HeaderValue::from_static(manifest.media_type.as_str());
        let api = self.api.clone();
        let received = self
            .registry
            .execute(&self.push(), |auth| {
                let body = EncodedBody::with_content_type(
                    manifest.bytes.clone().into(),
                    content_type.clone(),
                );
                let prepared = api
                    .clone()
                    .maybe_authorization(auth)
                    .put_manifest(reference.clone(), body)
                    .prepare()?;
                match reference.as_digest() {
                    Some(digest) => verbatim_last_segment(prepared, digest.as_str()),
                    None => Ok(prepared),
                }
            })
            .await?;
        let (received, meta) = expect_status(received, StatusCode::CREATED).await?;
        let header = meta
            .digest_header()
            .map_err(|problem| RegistryError::protocol(problem, Some(meta.clone())))?;
        drain(received, &meta).await?;
        if let Some(header) = header
            && header.algorithm() == manifest.digest.algorithm()
            && header != manifest.digest
        {
            return Err(RegistryError::integrity(
                IntegrityProblem::HeaderDigestMismatch {
                    expected: manifest.digest.clone(),
                    header,
                },
                Some(meta),
            ));
        }
        Ok(ManifestPushed {
            digest: manifest.digest.clone(),
            meta,
        })
    }
}
