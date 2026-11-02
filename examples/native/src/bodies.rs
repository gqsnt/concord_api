//! U16 / U29 (R09): custom formats (NDJSON, form-encoded).

use crate::api::{Inventory, StockRow};

/// Imports stock rows; request and response are NDJSON with this crate's
/// [`Ndjson`](crate::repr::Ndjson) format (U16).
///
/// Nothing here differs from a built-in format: the declaration writes `format Ndjson;` where it
/// would write `format json;`, the generated method takes `Vec<StockRow>`, and a malformed
/// response line is a [`concord::ErrorKind::Decode`] error whose `Display` names the
/// line number, never its content.
pub async fn import_stock(
    api: &Inventory,
    rows: Vec<StockRow>,
) -> Result<Vec<StockRow>, concord::Error> {
    api.import_stock(rows).send().await
}

/// Sends a form-encoded feedback submission through this crate's [`Form`](crate::repr::Form)
/// format (U29).
///
/// Pairs keep their order and repeated names. Use a custom `Encode` like this whenever the body
/// is fully determined by the value and its content type is constant.
pub async fn send_feedback(
    api: &Inventory,
    fields: Vec<(String, String)>,
) -> Result<(), concord::Error> {
    api.feedback(fields).send().await
}

// Multipart uploads: see the `concord-example-multipart` crate (`examples/multipart`), which
// attaches a `reqwest::multipart::Form` to a declared endpoint with reqwest's own `multipart`
// feature. That feature is enabled there only; this crate does not hand-write MIME bodies.
