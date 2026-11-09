//! The client's send policy: every generated `send()` returns the typed value with its
//! [`ResponseMeta`], or a [`GitLabError`] that keeps GitLab's error document.

use concord::{Endpoint, IgnoreContentType, Json, Received, SendWith};

use crate::error::{ApiError, ERROR_BODY_LIMIT, ErrorBody, ErrorDocument, GitLabError};
use crate::meta::{Response, ResponseMeta};

/// The send policy of [`GitLab`](crate::GitLab) (`send SendPolicy -> Result<crate::Response<_>, crate::GitLabError>;`): what every generated
/// `send()` returns, `Result<`[`Response<T>`]`, `[`GitLabError`]`>`. Callers never name it.
///
/// `send()` prepares, executes once and decodes as declared; it never retries.
/// [`GitLabError::Api`] is a non-2xx status (GitLab's document read up to
/// [`ERROR_BODY_LIMIT`]); [`GitLabError::Request`] is a request that could not be built (nothing
/// sent), a transport failure, a 2xx response not labelled JSON, or a body that does not decode —
/// with the response metadata once a response arrived. `prepare()` still returns the native
/// request with `concord::Error`.
///
/// ```no_run
/// use concord_sdk_gitlab::{GitLab, IssueIid, ProjectRef};
///
/// # async fn run(gitlab: GitLab) -> Result<(), Box<dyn std::error::Error>> {
/// let project = gitlab.project(ProjectRef::path("acme/widgets")?);
/// let issue = project.issue(IssueIid::new(7)?).send().await?;
/// println!("{} (request {:?})", issue.value.title, issue.meta.request_id());
/// # Ok(()) }
/// ```
#[derive(Default)]
pub struct SendPolicy;

impl<T: Send + 'static> SendWith<T> for SendPolicy {
    type Output = Result<Response<T>, GitLabError>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        finish(received).await
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
async fn decode<T: 'static>(
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

/// A non-2xx response as [`GitLabError::Api`], its body read up to the limit. GitLab's documents
/// are read whatever their `Content-Type` (a proxy page is kept raw as `Unrecognized`).
async fn api_error<T>(received: Received<T>, meta: ResponseMeta) -> GitLabError {
    let operation = received.operation();
    let status = received.status();
    let (_, document) = received
        .document::<IgnoreContentType<Json>, ErrorDocument>(ERROR_BODY_LIMIT)
        .await;
    let body = match document {
        Ok(document) => ErrorBody::Document(document),
        Err(error) => match error.kind() {
            concord::ErrorKind::Decode => {
                ErrorBody::Unrecognized(error.body_unredacted().cloned().unwrap_or_default())
            }
            concord::ErrorKind::Limit => ErrorBody::TooLarge {
                limit: ERROR_BODY_LIMIT,
            },
            _ => ErrorBody::Unreadable(error),
        },
    };
    GitLabError::Api(Box::new(ApiError {
        operation,
        status,
        body,
        meta,
    }))
}
