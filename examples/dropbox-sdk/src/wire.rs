//! Private declaration and common response policy.

use crate::{Entry, Error, ErrorBody, HttpError, ListFolderOptions, Response, ResponseMeta};
use concord::{
    Endpoint, Json, SendWith,
    reqwest::header::{AUTHORIZATION, HeaderValue},
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct WirePage {
    pub entries: Vec<Entry>,
    pub cursor: String,
    pub has_more: bool,
}

#[derive(Serialize)]
pub struct ContinueBody {
    pub cursor: String,
}

concord::api! {
    /// Declared Dropbox RPC transport: `send()` returns `Result<Response<T>, Error>`.
    /// Public domain methods bind cursors to their context.
    pub client DropboxApi(
        /// Bearer token without the prefix. Never printed by Concord's Debug.
        #[sensitive] token: String,
    ) {
        base "https://api.dropboxapi.com/2";
        headers { AUTHORIZATION = "Bearer {token}"; }
        format Json;
        limit crate::SUCCESS_BODY_LIMIT;
        send DropboxPolicy -> Result<Response<_>, Error>;

        scope selected(
            #[sensitive] header("Dropbox-API-Select-User") member: String,
            #[sensitive] header("Dropbox-API-Path-Root") root: HeaderValue,
        ) {
            #[read] POST list_folder(body options: ListFolderOptions) -> WirePage {
                path "files/list_folder";
            }
            #[read] POST list_folder_continue(body continuation: ContinueBody) -> WirePage {
                path "files/list_folder/continue";
            }
        }
    }
}

#[derive(Default)]
pub struct DropboxPolicy;

impl<T: Send> SendWith<T> for DropboxPolicy {
    type Output = Result<Response<T>, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            let meta = ResponseMeta(received.meta());
            let body = ErrorBody::new(received.bytes_limited(crate::ERROR_BODY_LIMIT).await);
            return Err(Error::Http(Box::new(HttpError { meta, body })));
        }
        // A failure after the head arrived keeps it in the Concord error (`From` reads it).
        let (value, meta) = received.decode_with_meta().await?;
        Ok(Response {
            value,
            meta: ResponseMeta(meta),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concord_example_testserver::{Reply, TestServer};

    #[tokio::test]
    async fn native_send_keeps_the_declared_success_body_limit() {
        let server =
            TestServer::start(|_| Reply::json(200, vec![b' '; crate::SUCCESS_BODY_LIMIT + 1]))
                .await;
        let api = DropboxApi::new(
            concord::reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap(),
            DropboxApiConfig {
                token: "synthetic-token".into(),
            },
        )
        .with_base_url(server.url())
        .unwrap();
        let error = api
            .selected("member-id", HeaderValue::from_static("{\".tag\":\"home\"}"))
            .list_folder(ListFolderOptions::default())
            .prepare()
            .unwrap()
            .send()
            .await
            .err()
            .expect("native send must reject an oversized body");
        assert_eq!(error.kind(), concord::ErrorKind::Limit);
        assert_eq!(error.head().unwrap().status().as_u16(), 200);
        server.shutdown().await;
    }
}

#[cfg(test)]
mod read_operation_tests {
    use super::*;
    #[test]
    fn declared_post_reads_keep_their_method_and_effect() {
        for operation in [
            DropboxApiSelectedListFolder::operation(),
            DropboxApiSelectedListFolderContinue::operation(),
        ] {
            assert_eq!(operation.method(), Some("POST"));
            assert!(operation.is_read(), "{}", operation.name());
        }
    }
}
