use concord::error::Class;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio_util::io::ReaderStream;           // crate `tokio-util`, feature `io`; `tokio::fs` needs tokio's `fs`

/// Sends the file at `path` as `name`: at most three calls, within ten minutes.
async fn upload(files: &Files, name: &str, path: &Path) -> Result<(), BoxError> {
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut calls = 0;
    loop {
        let file = tokio::fs::File::open(path).await?;             // the file, opened again
        let body = concord::Body::stream(ReaderStream::new(file));  // a new stream for each call
        let left = deadline.saturating_duration_since(Instant::now());
        let error = match files.upload(name, body).with_timeout(left).await {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        calls += 1;
        // A `PUT` of the same file to the same name may be repeated; these failures are worth it.
        let again = error.class() == Some(Class::Connect) || error.is_status(503);
        let wait = error.retry_after().unwrap_or(Duration::from_secs(1));
        if !again || calls == 3 || wait >= deadline.saturating_duration_since(Instant::now()) {
            return Err(error.into());
        }
        tokio::time::sleep(wait).await;
    }
}
