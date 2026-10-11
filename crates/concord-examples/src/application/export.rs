//! A bounded export prefix (§23.5).
//!
//! Awaiting the export request returns after acquisition (the stream is
//! opened, not completed). The stream summary records all acquired bytes,
//! which can exceed the application-retained prefix by the remainder of the
//! last chunk. Stopping at a prefix is not EOF: `finish()` records an
//! application stop. The zero path explicitly skips request construction and
//! execution; that is this application's choice, not something inferred from
//! every downstream `finish`.

// ANCHOR: export_prefix
use concord::response::StreamSummary;
use concord::{ApiError, CallOptions};

use super::execute::execute;
use crate::contracts::catalog_v3::catalog_api;

/// The retained export prefix.
#[derive(Debug)]
pub enum ExportPrefix {
    /// Nothing was requested; no request was constructed or executed.
    NotStarted,
    /// The retained bytes and the stream summary at the stop.
    Read {
        /// Exactly the retained prefix (at most `wanted_bytes`).
        bytes: Vec<u8>,
        /// The stream summary; its acquired bytes can exceed the prefix.
        stream: StreamSummary,
    },
}

/// Reads at most `wanted_bytes` of the export stream.
///
/// # Errors
///
/// Acquisition failure or a correlated stream consumption failure.
pub async fn export_prefix(
    scope: &catalog_api::regional::Regional,
    wanted_bytes: usize,
    limits: &CallOptions,
) -> Result<ExportPrefix, ApiError> {
    if wanted_bytes == 0 {
        return Ok(ExportPrefix::NotStarted);
    }
    let mut stream = execute(scope.export(), limits).await?;
    let mut output = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let remaining = wanted_bytes - output.len();
        let take = remaining.min(chunk.len());
        output.extend_from_slice(&chunk[..take]);
        if output.len() == wanted_bytes {
            break;
        }
    }
    Ok(ExportPrefix::Read {
        bytes: output,
        stream: stream.finish(),
    })
}
// ANCHOR_END: export_prefix
