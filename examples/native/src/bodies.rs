//! U16 / U29 (R09): custom representations and a native multipart body.

use concord::reqwest::{
    self,
    header::{CONTENT_TYPE, HeaderValue},
};

use crate::api::{Inventory, Item, StockRow};

/// Imports stock rows; request and response are NDJSON via this crate's
/// [`Ndjson`](crate::repr::Ndjson) representation (U16).
///
/// Nothing here differs from a built-in representation: the declaration names
/// `via Ndjson`, the generated method takes `Vec<StockRow>`, and a malformed response line is a
/// [`concord::ErrorKind::Decode`] error whose `Display` names the line number, never its content.
pub async fn import_stock(
    api: &Inventory,
    rows: Vec<StockRow>,
) -> Result<Vec<StockRow>, concord::Error> {
    api.import_stock(rows).send().await
}

/// Sends a form-encoded feedback submission through this crate's [`Form`](crate::repr::Form)
/// representation (U29).
///
/// Pairs keep their order and repeated names. Use a custom `Encode` like this whenever the body
/// is fully determined by the value and its content type is constant.
pub async fn send_feedback(
    api: &Inventory,
    fields: Vec<(String, String)>,
) -> Result<(), concord::Error> {
    api.feedback(fields).send().await
}

/// A file part for [`upload_file`].
#[derive(Debug, Clone)]
pub struct FilePart<'a> {
    /// The form field name.
    pub field: &'a str,
    /// The file name sent in `Content-Disposition` (`"`, CR and LF are percent-encoded).
    pub file_name: &'a str,
    /// The part's media type.
    pub content_type: &'a str,
    /// The file content.
    pub content: &'a [u8],
}

/// Uploads one file as `multipart/form-data` built by the application and attached to the
/// native request (U29).
///
/// Why native: a multipart `Content-Type` carries a boundary chosen per body, but
/// [`concord::Encode::CONTENT_TYPE`] is a constant. (An `Encode` with a fixed boundary also
/// works if it rejects content containing that boundary.) This recipe declares the endpoint
/// without a body, then sets the body and its content type on the native request, keeping the
/// declared path, headers (authorization) and decoder. reqwest's own `multipart` feature is
/// not enabled in this workspace, so the body is assembled by hand and buffered in memory;
/// streamed file parts are not covered here.
pub async fn upload_file(api: &Inventory, part: FilePart<'_>) -> Result<Item, concord::Error> {
    let (client, mut request, decoder) = api.upload().prepare()?.into_parts();
    let (boundary, body) = multipart_body(&part);
    let content_type = HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
        .expect("boundary is a visible-ASCII token");
    request.headers_mut().insert(CONTENT_TYPE, content_type);
    *request.body_mut() = Some(reqwest::Body::from(body));
    decoder.finish(client.execute(request).await).await
}

/// Builds a single-part multipart body with a boundary that occurs nowhere in the part.
fn multipart_body(part: &FilePart<'_>) -> (String, Vec<u8>) {
    fn header_safe(text: &str) -> String {
        text.replace('"', "%22")
            .replace('\r', "%0D")
            .replace('\n', "%0A")
    }
    let head = format!(
        "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
        header_safe(part.field),
        header_safe(part.file_name),
        header_safe(part.content_type),
    );
    let mut boundary = String::new();
    for n in 0u64.. {
        boundary = format!("concord-example-boundary-{n:016x}");
        let needle = boundary.as_bytes();
        let clash = |hay: &[u8]| hay.windows(needle.len()).any(|w| w == needle);
        if !clash(part.content) && !clash(head.as_bytes()) {
            break;
        }
    }
    let mut body = Vec::with_capacity(part.content.len() + head.len() + 2 * boundary.len() + 16);
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(head.as_bytes());
    body.extend_from_slice(part.content);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary, body)
}
