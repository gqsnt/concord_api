//! The `concord::api!` declaration of the four operations: the generated bucket handle and
//! operation builders are the SDK's public ones (re-exported as [`crate::Bucket`],
//! [`crate::ListObjectsV2`], [`crate::HeadObject`], [`crate::GetObject`] and
//! [`crate::PutObject`]).
//!
//! Every generated `send()` goes through the client's send policy, the client value `signer`
//! ([`crate::Signer`]: Region, credentials, clock; not on the wire): it brings the built target
//! into S3's canonical form, signs the request (SigV4), executes it once and maps the response to
//! [`crate::S3Error`]. The generated `prepare()` is the declared request **unsigned** (S3 refuses
//! it); [`crate::S3Client::prepare`] signs one without sending it.
//!
//! Addressing is virtual-hosted style: the bucket is the leftmost host label
//! (`{bucket}.s3.{region}.amazonaws.com`); the object key is the path. The declared base is a
//! placeholder replaced by the client's endpoint.

use concord::bytes::Bytes;
use concord::reqwest::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_ENCODING, CONTENT_LANGUAGE, CONTENT_TYPE,
};
use concord::{Binary, IgnoreContentType};

use crate::client::Signer;
use crate::object::{Download, HeadFormat, ObjectHead, PutFormat, PutObjectOutput};
use crate::types::{
    AccountId, AnyObject, BucketName, ByteRange, ContentType, ContinuationToken, ETag, HttpDate,
    MaxKeys, ObjectKey, PartNumber, Requester, StorageClass, UrlEncoding, VersionId,
};
use crate::xml::{ErrorDocument, ListObjectsV2Output, ListXml, S3ErrorXml};

concord::api! {
    /// Amazon S3, general-purpose buckets: `ListObjectsV2`, `HeadObject`, `GetObject` and
    /// single-request `PutObject`. Every `send()` is signed by the client's [`Signer`].
    pub client S3Api(
        /// The SigV4 context and send policy of every operation.
        signer: Signer,
    ) {
        base "https://s3.us-east-1.amazonaws.com";
        send signer;
        error ErrorDocument in IgnoreContentType<S3ErrorXml>;

        /// One bucket, addressed as the leftmost host label.
        scope bucket(
            bucket: BucketName,
            /// `x-amz-expected-bucket-owner`: S3 answers `403` if another account owns the bucket.
            expected_owner?: AccountId,
            /// `x-amz-request-payer: requester` (Requester Pays buckets).
            requester_pays?: Requester,
        ) {
            subdomain "{bucket}";
            headers {
                "x-amz-expected-bucket-owner" = "{expected_owner}";
                "x-amz-request-payer" = "{requester_pays}";
            }

            /// `ListObjectsV2`: `GET /?list-type=2`, one page; every page through
            /// [`pages`](crate::ListObjectsV2::pages).
            GET list_objects_v2(
                /// `continuation-token`: continue an earlier listing.
                #[sensitive] query("continuation-token") continuation_token?: ContinuationToken,
                /// `delimiter`: roll keys up to the first delimiter after the prefix into common
                /// prefixes.
                query delimiter?: String,
                /// `encoding-type=url`: S3 percent-encodes key fields (for keys XML 1.0 cannot
                /// carry); the SDK decodes them, so the output holds the real keys either way.
                query("encoding-type") encoding_type?: UrlEncoding,
                /// `fetch-owner`: include each object's `<Owner>`.
                query("fetch-owner") fetch_owner?: bool,
                /// `max-keys`: at most this many keys and common prefixes per page.
                query("max-keys") max_keys?: MaxKeys,
                /// `prefix`: only keys that begin with it.
                query prefix?: String,
                /// `start-after`: list keys after this one.
                query("start-after") start_after?: ObjectKey,
            ) -> ListObjectsV2Output {
                query { "list-type" = "2"; }
                response ListXml;
            }

            /// `HeadObject`: `HEAD /{key}`, the object's metadata without its body. `304` is
            /// [`S3Error::NotModified`](crate::S3Error::NotModified), `412` an
            /// [`S3Error::Api`](crate::S3Error::Api); S3 sends no error document for `HEAD`.
            HEAD head_object(
                key: ObjectKey,
                /// `Range`: S3 reports the length of this range (`416` when unsatisfiable).
                range?: ByteRange,
                /// `If-Match`: only if the current entity tag is this one (`412` otherwise).
                if_match?: ETag,
                /// `If-None-Match`: only if the current entity tag differs (`304` otherwise).
                if_none_match?: ETag,
                /// `If-Modified-Since`: only if modified after this date (`304` otherwise).
                if_modified_since?: HttpDate,
                /// `If-Unmodified-Since`: only if not modified after this date (`412` otherwise).
                if_unmodified_since?: HttpDate,
                /// `partNumber`: one part of an object uploaded in parts.
                query("partNumber") part_number?: PartNumber,
                /// `versionId`: a specific version of the object.
                query("versionId") version_id?: VersionId,
            ) -> ObjectHead {
                path "{key}";
                headers {
                    "range" = "{range}";
                    "if-match" = "{if_match}";
                    "if-none-match" = "{if_none_match}";
                    "if-modified-since" = "{if_modified_since}";
                    "if-unmodified-since" = "{if_unmodified_since}";
                }
                response HeadFormat;
            }

            /// `GetObject`: `GET /{key}`, the whole object or one byte range, conditionally;
            /// `send()` returns the head with the **unread** body. When a range was requested, a
            /// `206` must answer it ([`ProtocolErrorKind::RangeMismatch`](crate::ProtocolErrorKind::RangeMismatch)
            /// otherwise); a `200` means S3 sent the whole object. `304` is
            /// [`S3Error::NotModified`](crate::S3Error::NotModified), `412` an
            /// [`S3Error::Api`](crate::S3Error::Api).
            GET get_object(
                key: ObjectKey,
                /// `Range`: one byte range (S3 does not serve several ranges in one `GET`).
                range?: ByteRange,
                /// `If-Match`: only if the current entity tag is this one (`412` otherwise).
                if_match?: ETag,
                /// `If-None-Match`: only if the current entity tag differs (`304` otherwise).
                if_none_match?: ETag,
                /// `If-Modified-Since`: only if modified after this date (`304` otherwise).
                if_modified_since?: HttpDate,
                /// `If-Unmodified-Since`: only if not modified after this date (`412` otherwise).
                if_unmodified_since?: HttpDate,
                /// `partNumber`: one part of an object uploaded in parts.
                query("partNumber") part_number?: PartNumber,
                /// `response-cache-control`: overrides the response's `Cache-Control`.
                query("response-cache-control") response_cache_control?: String,
                /// `response-content-disposition`: overrides the response's `Content-Disposition`.
                query("response-content-disposition") response_content_disposition?: String,
                /// `response-content-encoding`: overrides the response's `Content-Encoding`.
                query("response-content-encoding") response_content_encoding?: String,
                /// `response-content-language`: overrides the response's `Content-Language`.
                query("response-content-language") response_content_language?: String,
                /// `response-content-type`: overrides the response's `Content-Type`.
                query("response-content-type") response_content_type?: ContentType,
                /// `response-expires`: overrides the response's `Expires`.
                query("response-expires") response_expires?: HttpDate,
                /// `versionId`: a specific version of the object.
                query("versionId") version_id?: VersionId,
            ) -> stream Bytes {
                path "{key}";
                headers {
                    "range" = "{range}";
                    "if-match" = "{if_match}";
                    "if-none-match" = "{if_none_match}";
                    "if-modified-since" = "{if_modified_since}";
                    "if-unmodified-since" = "{if_unmodified_since}";
                }
                response Download;
            }

            /// `PutObject`: `PUT /{key}` with the whole body in one request and an explicit
            /// content type (the payload hash is the SHA-256 of the body). A failed `If-Match` /
            /// `If-None-Match: *` is a `412` [`S3Error::Api`](crate::S3Error::Api). User metadata:
            /// [`metadata`](crate::PutObject::metadata).
            PUT put_object(
                key: ObjectKey,
                body data: Bytes,
                content_type: ContentType,
                /// `Cache-Control` stored with the object.
                header(CACHE_CONTROL) cache_control?: String,
                /// `Content-Disposition` stored with the object.
                header(CONTENT_DISPOSITION) content_disposition?: String,
                /// `Content-Encoding` stored with the object (the body is sent as given).
                header(CONTENT_ENCODING) content_encoding?: String,
                /// `Content-Language` stored with the object.
                header(CONTENT_LANGUAGE) content_language?: String,
                /// `Expires` stored with the object.
                expires?: HttpDate,
                /// `x-amz-storage-class`.
                storage_class?: StorageClass,
                /// `If-Match`: write only if the current object has this entity tag (`412`
                /// otherwise, `409 ConditionalRequestConflict` on a concurrent write).
                if_match?: ETag,
                /// `If-None-Match: *` ([`AnyObject`]): write only if no object has this key (`412`
                /// otherwise).
                if_none_match?: AnyObject,
            ) -> PutObjectOutput {
                path "{key}";
                headers {
                    CONTENT_TYPE = "{content_type}";
                    "expires" = "{expires}";
                    "x-amz-storage-class" = "{storage_class}";
                    "if-match" = "{if_match}";
                    "if-none-match" = "{if_none_match}";
                }
                request Binary;
                response PutFormat;
            }
        }
    }
}
