use concord::{Endpoint, SendWith};

use crate::Error;

/// Maximum finite success-body size in bytes, after any transport decompression (1 MiB).
pub const RESPONSE_BODY_LIMIT: usize = 1024 * 1024;
/// Maximum error-document size in bytes (16 KiB).
pub const ERROR_BODY_LIMIT: usize = 16 * 1024;

/// One request, strict JSON media checks and bounded bodies; no retries or authentication.
///
/// Generated `send()` selects this policy. The lower-level `prepare()?.send()` bypasses it and
/// returns Concord's typed document error; the success limit and error fact still apply.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReadPolicy;

impl<T: Send> SendWith<T, serde_json::Value> for ReadPolicy {
    type Output = Result<T, Error>;

    async fn send<E: Endpoint<T, serde_json::Value> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint
            .prepare()?
            .error_body_limit(crate::ERROR_BODY_LIMIT)
            .receive()
            .await?;

        Ok(received.decode().await?)
    }
}
