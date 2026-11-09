//! `QboJson`: `Json`, plus QuickBooks' documented rule that a successful response may carry a
//! `Fault` instead of the entity (top level, or inside `QueryResponse` for a query).

use std::fmt;

use concord::bytes::Bytes;
use concord::{CodecError, Decode, Json, ResponseHead};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::Fault;

/// Its `Decode::ACCEPT` suggests `accept: application/json`.
///
/// The response format of every Accounting endpoint: `Json`'s head check (exactly one
/// `application/json` `Content-Type`) and decoding, after looking for a `Fault` in the body. A
/// 2xx body that is a fault is a `Decode` error whose source is [`FaultInSuccess`]; the client's
/// send policy turns it into [`Error::Provider`](crate::Error::Provider) with status 200.
pub struct QboJson;

/// The fault found in a 2xx body. Reached through `concord::Error::codec_error()` and
/// `CodecError::source_unredacted()`; its `Display` names no provider text.
#[derive(Debug)]
pub struct FaultInSuccess(pub Fault);

impl fmt::Display for FaultInSuccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a successful QuickBooks response carried a Fault")
    }
}

impl std::error::Error for FaultInSuccess {}

#[derive(Deserialize)]
struct Probe {
    #[serde(rename = "Fault")]
    fault: Option<Fault>,
    #[serde(rename = "QueryResponse")]
    query: Option<QueryProbe>,
}

#[derive(Deserialize)]
struct QueryProbe {
    #[serde(rename = "Fault")]
    fault: Option<Fault>,
}

impl<T: DeserializeOwned> Decode<T> for QboJson {
    type Context = ();
    const ACCEPT: Option<&'static str> = Some("application/json");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <Json as Decode<T>>::check_content_type(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <Json as Decode<T>>::context(head)
    }

    fn decode_body((): (), body: Bytes) -> Result<T, CodecError> {
        // A body that is not even a JSON object is reported by the entity decoder below.
        if let Ok(probe) = serde_json::from_slice::<Probe>(&body)
            && let Some(fault) = probe.fault.or(probe.query.and_then(|q| q.fault))
        {
            return Err(CodecError::new("a 2xx QuickBooks response carried a Fault")
                .with_source(FaultInSuccess(fault)));
        }
        <Json as Decode<T>>::decode_body((), body)
    }
}
