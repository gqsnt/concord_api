//! The private declaration: one client, one scope per credential role, and the response policy
//! of the JSON operations.
//!
//! B2 issues the hosts at run time (`apiUrl`, `downloadUrl` from `b2_authorize_account`, an
//! opaque `uploadUrl` from `b2_get_upload_url`). Each handle declares its checked `origin` value;
//! the SDK constructs each role's scope from the URL B2 issued for it, and the
//! upload request moves to the issued upload URL with `Prepared::follow`.

use std::collections::BTreeMap;

use concord::bytes::Bytes;
use concord::reqwest::header::{AUTHORIZATION, HeaderValue};
use concord::{Binary, Encoded, EncodedBody, Endpoint, Json, Received, ResponseMeta, SendWith};
use serde::{Deserialize, Serialize};

use crate::error::{Error, ProviderError, WireErrorDocument};

concord::api! {
    /// Backblaze B2 Native API v4: the operations of this slice.
    pub client B2Wire(origin: concord::BaseUrl) {
        base origin;
        path "b2api/v4";
        format Json;
        limit crate::JSON_BODY_LIMIT;
        send B2Policy -> Result<(_, ResponseMeta), Error>;

        /// The application key's Basic credential, at the documented account host.
        scope account(#[sensitive] header(AUTHORIZATION) credentials: HeaderValue) {
            GET authorize_account() -> AuthorizeAccountWire { path "b2_authorize_account"; }
        }

        /// The account authorization token, bound to the issued `apiUrl`.
        scope storage(#[sensitive] header(AUTHORIZATION) token: String) {
            GET get_upload_url(query("bucketId") bucket_id: String) -> UploadUrlWire {
                path "b2_get_upload_url";
            }
            GET list_file_names(
                query("bucketId") bucket_id: String,
                query("startFileName") start_file_name?: String,
                query("prefix") prefix?: String,
                query("delimiter") delimiter?: String,
                query("maxFileCount") max_file_count?: u16,
            ) -> ListFileNamesWire {
                path "b2_list_file_names";
            }
            POST delete_file_version(body version: DeleteFileVersionRequest) -> DeletedFileVersionWire {
                path "b2_delete_file_version";
            }
        }

        /// The account authorization token, bound to the issued `downloadUrl`.
        scope download(#[sensitive] header(AUTHORIZATION) token: String) {
            GET download_file_by_id(query("fileId") file_id: String) -> Bytes {
                path "b2_download_file_by_id";
                response Binary;
            }
        }

        /// The upload authorization token, bound to the origin of the issued `uploadUrl`; the
        /// prepared request is then moved to that URL (`follow`), path and query as issued.
        scope upload(#[sensitive] header(AUTHORIZATION) token: String) {
            /// The body carries the file's `Content-Type` (a runtime media type) and its exact
            /// length (`Content-Length`: bytes, or a one-shot stream with an exact size hint).
            POST upload_file(
                header("X-Bz-File-Name") file_name: String,
                header("X-Bz-Content-Sha1") content_sha1: String,
                header("X-Bz-Info-src_last_modified_millis") last_modified_millis?: u64,
                body file: EncodedBody,
            ) -> FileVersionWire {
                path "b2_upload_file";
                request Encoded;
            }
        }
    }
}

/// The JSON operations' policy: one execution; a non-2xx is a [`ProviderError`] with the bounded
/// error document; a 2xx is decoded with its head.
#[derive(Default)]
pub struct B2Policy;

impl<T: Send> SendWith<T> for B2Policy {
    type Output = Result<(T, ResponseMeta), Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            return Err(provider_error(received).await);
        }
        Ok(received.decode_with_meta().await?)
    }
}

/// Reads a non-2xx body as B2's error document, bounded; the status stays the failure whatever
/// the body is.
pub(crate) async fn provider_error<T>(received: Received<T>) -> Error {
    let (meta, document) = received
        .document::<Json, WireErrorDocument>(crate::ERROR_BODY_LIMIT)
        .await;
    Error::Provider(Box::new(ProviderError::new(meta, document)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeAccountWire {
    pub account_id: String,
    pub api_info: ApiInfoWire,
    pub authorization_token: String,
    #[serde(default)]
    pub application_key_expiration_timestamp: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiInfoWire {
    pub storage_api: StorageApiWire,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageApiWire {
    pub api_url: String,
    pub download_url: String,
    pub allowed: AllowedWire,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowedWire {
    pub buckets: Vec<AllowedBucketWire>,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub name_prefix: Option<String>,
}

#[derive(Deserialize)]
pub struct AllowedBucketWire {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadUrlWire {
    pub bucket_id: String,
    pub upload_url: String,
    pub authorization_token: String,
}

/// `contentLength` is documented as a string (example `"46"`); a JSON number is accepted too.
/// Either way the value must be a plain unsigned decimal.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum Count {
    Number(u64),
    Text(String),
}

impl Count {
    pub fn value(&self) -> Option<u64> {
        match self {
            Self::Number(n) => Some(*n),
            Self::Text(text) if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => {
                text.parse().ok()
            }
            Self::Text(_) => None,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileVersionWire {
    pub account_id: String,
    pub action: String,
    pub bucket_id: String,
    pub content_length: Count,
    pub content_sha1: Option<String>,
    #[serde(default)]
    pub content_md5: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
    pub file_id: Option<String>,
    #[serde(default)]
    pub file_info: BTreeMap<String, String>,
    pub file_name: String,
    pub upload_timestamp: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFileNamesWire {
    pub files: Vec<FileVersionWire>,
    // A missing cursor is malformed, not evidence that traversal finished.
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub next_file_name: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFileVersionRequest {
    pub file_name: String,
    pub file_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedFileVersionWire {
    pub file_name: String,
    pub file_id: String,
}

#[cfg(test)]
mod tests {
    use super::B2Wire;

    #[test]
    fn header_names_compose() {
        B2Wire::validate_header_names().unwrap();
    }

    /// Why the upload request is moved with `follow` instead of binding the issued URL as the
    /// base: the documented query form of `uploadUrl` is not a `BaseUrl` accepts.
    #[test]
    fn the_documented_query_form_upload_url_is_not_a_base() {
        let issued = "https://pod-000-1005-03.backblaze.com/b2api/v4/b2_upload_file?cvt=c001_v0001005_t0027&bucket=4a48fe8875c6214145260818";
        let error = concord::BaseUrl::parse(issued).unwrap_err();
        assert_eq!(error.kind(), concord::ErrorKind::Build);
        let path_form = "https://pod-000-1007-13.backblaze.com/b2api/v4/b2_upload_file/4a48fe8875c6214145260818/c001_v0001007_t0042";
        assert!(concord::BaseUrl::parse(path_form).is_ok());
    }
}
