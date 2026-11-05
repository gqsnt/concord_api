//! Independent ASTRA caller exercise; all networking is loopback.
use concord::reqwest::{StatusCode, header::HeaderValue};

/// Capture a selected header and status while preserving the endpoint's decoder.
pub async fn with_metadata<T>(
    prepared: concord::Prepared<T>,
) -> Result<(StatusCode, Option<HeaderValue>, T), concord::Error> {
    let (client, request, decoder) = prepared.into_parts();
    let response = match client.execute(request).await {
        Ok(response) => response,
        Err(error) => return Err(decoder.finish(Err(error)).await.err().unwrap()),
    };
    let status = response.status();
    let request_id = response.headers().get("x-github-request-id").cloned();
    let value = decoder.decode(response).await?;
    Ok((status, request_id, value))
}
