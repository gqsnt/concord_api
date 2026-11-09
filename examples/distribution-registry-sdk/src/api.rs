//! The Concord declarations: the registry root, one repository, and the token service.
//!
//! They stay private behind [`Registry`](crate::Registry) and [`Repository`](crate::Repository),
//! which add what a declaration cannot carry: the Bearer challenge and retry, server-provided
//! upload URLs, digest verification and the protocol checks of each response.
//!
//! Three clients, because three URLs are not known when the SDK is compiled. Their names are the
//! ones Concord puts in error messages (`GET Repository.manifest: ...`), so they mirror the
//! public types rather than saying "api":
//!
//! - `Registry`: `GET /v2/` and the catalog, at the registry's base URL.
//! - `Repository`: everything under `/v2/<name>/`. A repository name spans several path segments
//!   (`acme/tools/app`), and a Concord path value is always exactly one segment (`/` would be
//!   sent as `%2F`), so each repository handle is this client with the base URL
//!   `<registry>/v2/<name>` (`base origin;`), the name validated by
//!   [`RepositoryName`](crate::RepositoryName) first.
//! - `TokenService`: the token service, at the `realm` URL a challenge names (`base origin;`).
//!
//! The upload endpoints (`upload_chunk`, `upload_status`, `complete_upload`) declare the upload
//! collection as a placeholder path: every request of an upload session is sent to the latest
//! `Location` the registry returned (`Prepared::follow`), never to a URL built from the upload's
//! UUID.

use concord::bytes::Bytes;
use concord::reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_RANGE, HeaderValue};
use concord::{Binary, Encoded, EncodedBody, Json};

use crate::digest::Digest;
use crate::model::{CatalogBody, TagsBody, TokenBody};
use crate::reference::{PageSize, Reference};

concord::api! {
    /// The registry root, at the registry's base URL.
    pub(crate) client Registry(
        origin: concord::BaseUrl,
        /// `Authorization: Bearer <token>` once a challenge was answered.
        #[sensitive] header(AUTHORIZATION) authorization?: HeaderValue,
    ) {
        base origin;

        /// `GET /v2/`: API version check.
        GET ping() { path "v2/"; }

        /// `GET /v2/_catalog`: one page of repository names.
        GET catalog(query n?: PageSize, query last?: String) -> CatalogBody {
            path "v2/_catalog";
            page next link;
            response Json;
        }
    }
}

concord::api! {
    /// One repository, at the base URL `<registry>/v2/<name>`.
    pub(crate) client Repository(
        origin: concord::BaseUrl,
        /// `Authorization: Bearer <token>` once a challenge was answered.
        #[sensitive] header(AUTHORIZATION) authorization?: HeaderValue,
    ) {
        base origin;

        /// `GET /v2/<name>/tags/list`: one page of tags.
        GET tags(query n?: PageSize, query last?: String) -> TagsBody {
            path "tags/list";
            page next link;
            response Json;
        }

        /// `GET /v2/<name>/manifests/<reference>` with the caller's `Accept`; the raw bytes (the
        /// digest is computed over them).
        GET manifest(reference: Reference, header(ACCEPT) accept: HeaderValue) -> Bytes {
            path "manifests/{reference}";
            response Binary;
            limit crate::manifest::MANIFEST_LIMIT;
        }

        /// `PUT /v2/<name>/manifests/<reference>`: the manifest bytes with their media type.
        PUT put_manifest(reference: Reference, body manifest: EncodedBody) {
            path "manifests/{reference}";
            request Encoded;
        }

        /// `HEAD /v2/<name>/blobs/<digest>`: existence and length.
        HEAD blob_head(digest: Digest) { path "blobs/{digest}"; }

        /// `GET /v2/<name>/blobs/<digest>`: the unread body, read and verified by the SDK.
        GET blob(digest: Digest) -> stream { path "blobs/{digest}"; }

        /// `POST /v2/<name>/blobs/uploads/`: starts an upload (an empty body, `Content-Length: 0`).
        POST start_upload(body empty: Bytes) {
            path "blobs/uploads/";
            request Binary;
        }

        /// `PATCH <Location>`: one chunk, `Content-Range: <start>-<end>` (inclusive).
        PATCH upload_chunk(header(CONTENT_RANGE) range: String, body chunk: Bytes) {
            path "blobs/uploads/";
            request Binary;
        }

        /// `GET <Location>`: the upload's progress.
        GET upload_status() { path "blobs/uploads/"; }

        /// `PUT <Location>&digest=<digest>`: completes the upload, with a last chunk or empty.
        PUT complete_upload(body last: Bytes) {
            path "blobs/uploads/";
            request Binary;
        }
    }
}

concord::api! {
    /// The token service, at a challenge's `realm`.
    pub(crate) client TokenService(
        origin: concord::BaseUrl,
        /// `Authorization: Basic ...` for the token service, when configured.
        #[sensitive] header(AUTHORIZATION) credentials?: HeaderValue,
    ) {
        base origin;
        limit crate::client::TOKEN_BODY_LIMIT;

        /// `GET <realm>?service=..&scope=..`: a Bearer token.
        GET token(
            query service?: String,
            query("scope") each scopes: Vec<String>,
            query client_id?: String,
        ) -> TokenBody {
            response Json;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_names_compose() {
        Registry::validate_header_names().unwrap();
        Repository::validate_header_names().unwrap();
        TokenService::validate_header_names().unwrap();
    }

    #[tokio::test]
    async fn native_manifest_send_keeps_its_declared_body_limit() {
        use concord_example_testserver::{Reply, TestServer};

        let server = TestServer::start(|_| {
            Reply::new(200).body(vec![b' '; crate::manifest::MANIFEST_LIMIT + 1])
        })
        .await;
        let api = concord::BaseUrl::parse((server.url()).as_ref())
            .map(|origin| {
                Repository::new(
                    concord::reqwest::Client::builder()
                        .no_proxy()
                        .build()
                        .unwrap(),
                    RepositoryConfig { origin },
                )
            })
            .unwrap();
        let error = api
            .manifest(
                Reference::tag("latest").unwrap(),
                HeaderValue::from_static("application/vnd.oci.image.manifest.v1+json"),
            )
            .prepare()
            .unwrap()
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.kind(), concord::ErrorKind::Limit);
        assert_eq!(error.head().unwrap().status().as_u16(), 200);
        server.shutdown().await;
    }

    #[tokio::test]
    async fn native_token_send_keeps_its_declared_body_limit() {
        use concord_example_testserver::{Reply, TestServer};

        let server = TestServer::start(|_| {
            Reply::json(200, vec![b' '; crate::client::TOKEN_BODY_LIMIT + 1])
        })
        .await;
        let api = concord::BaseUrl::parse((server.url()).as_ref())
            .map(|origin| {
                TokenService::new(
                    concord::reqwest::Client::builder()
                        .no_proxy()
                        .build()
                        .unwrap(),
                    TokenServiceConfig { origin },
                )
            })
            .unwrap();
        let Err(error) = api.token(Vec::new()).prepare().unwrap().send().await else {
            panic!("native send must reject an oversized token body");
        };
        assert_eq!(error.kind(), concord::ErrorKind::Limit);
        assert_eq!(error.head().unwrap().status().as_u16(), 200);
        server.shutdown().await;
    }
}
