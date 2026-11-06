//! The `concord::api!` declaration of the four operations.
//!
//! It is private to the SDK: every S3 request must be signed after Concord built it and before it
//! is sent, so the generated `send()` (which would send the request unsigned) is never handed to
//! callers. [`crate::S3Client`] and the operation builders wrap the generated requests and send
//! them through [`crate::client`]'s signing pipeline.
//!
//! Addressing is virtual-hosted style: the bucket is the leftmost host label
//! (`{bucket}.s3.{region}.amazonaws.com`); the object key is the path. The declared base is a
//! placeholder replaced by the client's endpoint.

use concord::Binary;
use concord::bytes::Bytes;
use concord::reqwest::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_ENCODING, CONTENT_LANGUAGE, CONTENT_TYPE,
};

use crate::object::{Download, HeadFormat, ObjectHead, PutFormat, PutObjectOutput};
use crate::types::{
    AccountId, AnyObject, BucketName, ByteRange, ContinuationToken, ETag, HttpDate, MaxKeys,
    ObjectKey, PartNumber, Requester, StorageClass, UrlEncoding, VersionId,
};
use crate::xml::{ListObjectsV2Output, ListXml};

concord::api! {
    /// Amazon S3, general-purpose buckets: `ListObjectsV2`, `HeadObject`, `GetObject` and
    /// single-request `PutObject`. Requests are unsigned until the SDK signs them.
    pub client S3Api {
        base "https://s3.us-east-1.amazonaws.com";

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

            /// `ListObjectsV2`: `GET /?list-type=2`.
            GET list_objects_v2(
                #[sensitive] query("continuation-token") continuation_token?: ContinuationToken,
                query delimiter?: String,
                query("encoding-type") encoding_type?: UrlEncoding,
                query("fetch-owner") fetch_owner?: bool,
                query("max-keys") max_keys?: MaxKeys,
                query prefix?: String,
                query("start-after") start_after?: ObjectKey,
            ) -> ListObjectsV2Output {
                query { "list-type" = "2"; }
                response ListXml;
            }

            /// `HeadObject`: `HEAD /{key}`.
            HEAD head_object(
                key: ObjectKey,
                range?: ByteRange,
                if_match?: ETag,
                if_none_match?: ETag,
                if_modified_since?: HttpDate,
                if_unmodified_since?: HttpDate,
                query("partNumber") part_number?: PartNumber,
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

            /// `GetObject`: `GET /{key}`; the body is returned unread.
            GET get_object(
                key: ObjectKey,
                range?: ByteRange,
                if_match?: ETag,
                if_none_match?: ETag,
                if_modified_since?: HttpDate,
                if_unmodified_since?: HttpDate,
                query("partNumber") part_number?: PartNumber,
                query("response-cache-control") response_cache_control?: String,
                query("response-content-disposition") response_content_disposition?: String,
                query("response-content-encoding") response_content_encoding?: String,
                query("response-content-language") response_content_language?: String,
                query("response-content-type") response_content_type?: String,
                query("response-expires") response_expires?: HttpDate,
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

            /// `PutObject`: `PUT /{key}` with the whole body in one request.
            PUT put_object(
                key: ObjectKey,
                body data: Bytes,
                header(CONTENT_TYPE) content_type: String,
                header(CACHE_CONTROL) cache_control?: String,
                header(CONTENT_DISPOSITION) content_disposition?: String,
                header(CONTENT_ENCODING) content_encoding?: String,
                header(CONTENT_LANGUAGE) content_language?: String,
                expires?: HttpDate,
                storage_class?: StorageClass,
                if_match?: ETag,
                if_none_match?: AnyObject,
            ) -> PutObjectOutput {
                path "{key}";
                headers {
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
