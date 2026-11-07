use concord::{Endpoint, Json, SendWith};

use crate::{Error, HttpError};

/// Maximum finite success-body size in bytes, after any transport decompression (1 MiB).
pub const RESPONSE_BODY_LIMIT: usize = 1024 * 1024;
/// Maximum error-document size in bytes (16 KiB).
pub const ERROR_BODY_LIMIT: usize = 16 * 1024;

/// One request, strict JSON media checks and bounded bodies; no retries or authentication.
///
/// Generated `send()` selects this policy. The lower-level `prepare()?.send()` bypasses it and
/// returns Concord's ordinary result without these SDK limits or HTTP document handling.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReadPolicy;

impl<T: Send> SendWith<T> for ReadPolicy {
    type Output = Result<T, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            let (meta, document) = received
                .document::<Json, serde_json::Value>(ERROR_BODY_LIMIT)
                .await;
            return Err(Error::Http(Box::new(HttpError { meta, document })));
        }
        Ok(received.decode().await?)
    }
}
