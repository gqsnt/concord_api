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
    /// Declared Dropbox RPC transport. Public domain methods bind cursors to their context.
    pub client DropboxApi(
        /// Bearer token without the prefix. Never printed by Concord's Debug.
        #[sensitive] token: String,
    ) {
        base "https://api.dropboxapi.com/2";
        headers { AUTHORIZATION = "Bearer {token}"; }
        format Json;
        send DropboxPolicy;

        scope selected(
            #[sensitive] header("Dropbox-API-Select-User") member: String,
            #[sensitive] header("Dropbox-API-Path-Root") root: HeaderValue,
        ) {
            POST list_folder(body options: ListFolderOptions) -> WirePage {
                path "files/list_folder";
            }
            POST list_folder_continue(body continuation: ContinueBody) -> WirePage {
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
        let meta = ResponseMeta(received.meta());
        if !meta.status().is_success() {
            let body = ErrorBody::new(received.bytes_limited(crate::ERROR_BODY_LIMIT).await);
            return Err(Error::Http(Box::new(HttpError { meta, body })));
        }
        let value = received
            .decode_limited(crate::SUCCESS_BODY_LIMIT)
            .await
            .map_err(|error| Error::Concord {
                error,
                meta: Some(Box::new(meta.clone())),
            })?;
        Ok(Response { value, meta })
    }
}
