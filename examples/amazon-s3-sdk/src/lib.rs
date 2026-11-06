//! A bounded Amazon S3 SDK for general-purpose buckets, built on Concord.
//!
//! Operations: `ListObjectsV2` (XML pages with opaque continuation tokens), `HeadObject`,
//! `GetObject` (whole object or one byte range, conditional, streamed body) and single-request
//! `PutObject` (a finite body with an explicit content type and user metadata). Every request is
//! signed with AWS Signature Version 4 (`Authorization` header) for the client's Region, the `s3`
//! service and the client's or the request's credentials. Not covered: multipart uploads,
//! presigned URLs, streaming (chunked) signatures, directory buckets, path-style addressing and
//! every other operation; `README.md` lists the limits.
//!
//! # Layout
//!
//! - [`S3Client`] — endpoint, Region, [`Credentials`], [`Clock`] over your `reqwest::Client`
//!   (build it with [`http_client_builder`]). [`S3Client::bucket`] returns a [`Bucket`].
//! - [`Bucket`] — [`list_objects_v2`](Bucket::list_objects_v2),
//!   [`head_object`](Bucket::head_object), [`get_object`](Bucket::get_object),
//!   [`put_object`](Bucket::put_object). Each returns a builder with typed setters,
//!   `prepare()` (signed, not sent) and `send()` (one request).
//! - [`ListObjectsV2::pages`] — the listing as a `concord_pagination::Pages` chain.
//! - [`S3Error`] — where a call stopped, with the response's [`ResponseMeta`] (request ids),
//!   S3's `<Error>` code, `304`/`412` outcomes and body-read failures.
//! - [`types`] — validated values ([`BucketName`], [`ObjectKey`], [`ETag`], [`ByteRange`],
//!   [`HttpDate`], [`Metadata`], ...); [`sigv4`] — the signer, usable on native requests.
//!
//! The Concord declaration (`src/api.rs`) is private: S3 rejects unsigned requests, and the
//! generated `send()` would send one. The builders wrap the generated requests and sign them
//! between `prepare()` and execution.
//!
//! # Example
//!
//! ```no_run
//! use concord_sdk_amazon_s3::{
//!     BucketName, ByteRange, ContentType, Credentials, Metadata, ObjectKey, Region, S3Client,
//!     http_client_builder,
//! };
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let credentials = Credentials::new(
//!     std::env::var("AWS_ACCESS_KEY_ID")?,
//!     std::env::var("AWS_SECRET_ACCESS_KEY")?,
//! )?;
//! let s3 = S3Client::new(http_client_builder().build()?, Region::new("eu-west-3")?, credentials)?;
//! let bucket = s3.bucket(BucketName::new("my-bucket")?);
//!
//! let key = ObjectKey::new("notes/hello.txt")?;
//! let metadata = Metadata::new().insert("author", "me")?;
//! bucket
//!     .put_object(key.clone(), "hello, world", ContentType::new("text/plain")?)
//!     .metadata(metadata)
//!     .send()
//!     .await?;
//!
//! let first_five = bucket.get_object(key).range(ByteRange::inclusive(0, 4)?).send().await?;
//! let (head, bytes) = first_five.bytes(1024).await?;
//! assert_eq!(&bytes[..], b"hello");
//! assert_eq!(head.metadata.get("author"), Some("me"));
//! # Ok(()) }
//! ```

mod api;
mod client;
pub mod error;
mod object;
pub mod sigv4;
pub mod types;
mod xml;

pub use client::{
    Bucket, GetObject, HeadObject, ListObjectsV2, ListPage, ObjectPages, PutObject, S3Client,
    SignedRequest, http_client_builder,
};
pub use error::{
    ApiError, BodyError, BodyErrorKind, ErrorBody, InputError, MAX_ERROR_BODY, NotModified,
    ProtocolErrorKind, ResponseMeta, S3Error, SigningError,
};
pub use object::{
    Download, GetObjectOutput, HeadFormat, ObjectBody, ObjectHead, PutFormat, PutObjectOutput,
};
pub use sigv4::{Clock, Credentials};
pub use types::{
    AccountId, BucketName, ByteRange, ContentRange, ContentType, ContinuationToken, ETag, HttpDate,
    MaxKeys, Metadata, ObjectKey, PartNumber, Region, StorageClass, VersionId,
};
pub use xml::{ListObjectsV2Output, ListXml, ObjectSummary, Owner};

/// The README's examples, compiled as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
