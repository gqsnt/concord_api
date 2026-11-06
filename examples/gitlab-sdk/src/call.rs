//! One adapter for every operation: [`Call::call`] sends a generated endpoint and returns the
//! typed value with its [`ResponseMeta`], or a [`GitLabError`] that keeps GitLab's error document.

use std::future::Future;

use concord::{Decode, Endpoint, Json, Received};

use crate::error::{ApiError, ERROR_BODY_LIMIT, ErrorBody, ErrorDocument, GitLabError};
use crate::meta::{Response, ResponseMeta};

/// Sends any operation of the [`GitLab`](crate::GitLab) declaration with the SDK's error.
///
/// ```no_run
/// use concord_sdk_gitlab::{Call, GitLab, IssueIid, ProjectRef};
///
/// # async fn run(gitlab: GitLab) -> Result<(), Box<dyn std::error::Error>> {
/// let project = gitlab.project(ProjectRef::path("acme/widgets")?);
/// let issue = project.issue(IssueIid::new(7)?).call().await?;
/// println!("{} (request {:?})", issue.value.title, issue.meta.request_id());
/// # Ok(()) }
/// ```
pub trait Call<T>: Endpoint<T> {
    /// Prepares, executes once and decodes as declared. Never retries.
    ///
    /// # Errors
    ///
    /// [`GitLabError::Api`] for a non-2xx status (GitLab's document read up to
    /// [`ERROR_BODY_LIMIT`]); [`GitLabError::Request`] for a request that could not be built
    /// (nothing sent), a transport failure, a 2xx response not labelled JSON, or a body that does
    /// not decode — with the response metadata once a response arrived.
    fn call(self) -> impl Future<Output = Result<Response<T>, GitLabError>> + Send;
}

impl<T, E> Call<T> for E
where
    T: Send + 'static,
    E: Endpoint<T>,
{
    fn call(self) -> impl Future<Output = Result<Response<T>, GitLabError>> + Send {
        let prepared = self.prepare();
        async move {
            let received = prepared?.receive().await?;
            finish(received).await
        }
    }
}

/// The SDK's handling of a received response: metadata first, then GitLab's error document for a
/// non-2xx status, then the declared decoding.
pub(crate) async fn finish<T: 'static>(received: Received<T>) -> Result<Response<T>, GitLabError> {
    let meta = ResponseMeta::capture(&received);
    if !received.status().is_success() {
        return Err(api_error(received, meta).await);
    }
    decode(received, meta).await
}

/// The declared decoding of a 2xx response, keeping `meta` with the value or the error.
pub(crate) async fn decode<T: 'static>(
    received: Received<T>,
    meta: ResponseMeta,
) -> Result<Response<T>, GitLabError> {
    match received.decode().await {
        Ok(value) => Ok(Response { value, meta }),
        Err(error) => Err(GitLabError::Request {
            error,
            meta: Some(Box::new(meta)),
        }),
    }
}

/// A non-2xx response as [`GitLabError::Api`], its body read up to the limit.
pub(crate) async fn api_error<T>(received: Received<T>, meta: ResponseMeta) -> GitLabError {
    let operation = received.operation();
    let status = received.status();
    let body = match received.bytes_limited(ERROR_BODY_LIMIT).await {
        Ok(bytes) => match <Json as Decode<ErrorDocument>>::decode_body((), bytes.clone()) {
            Ok(document) => ErrorBody::Document(document),
            Err(_) => ErrorBody::Unrecognized(bytes),
        },
        Err(error) if error.kind() == concord::ErrorKind::Limit => ErrorBody::TooLarge {
            limit: ERROR_BODY_LIMIT,
        },
        Err(error) => ErrorBody::Unreadable(error),
    };
    GitLabError::Api(Box::new(ApiError {
        operation,
        status,
        body,
        meta,
    }))
}
