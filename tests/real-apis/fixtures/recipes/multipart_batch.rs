use crate::multipart_response::{parts, BoxError};
use concord::{
    bytes::Bytes,
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    Answer, Body, ByteStream,
};
use std::io;

concord::api! {
    pub client Gmail(authorization: concord::Secret) {
        base "https://gmail.googleapis.com";
        headers { "authorization" = authorization; }
        POST batch(request: concord::Body) -> concord::ByteStream {
            path "batch/gmail/v1"; body request; read concord::read::Stream;
        }
    }
}
concord::api! {
    pub client SharePoint(site: concord::Base, authorization: concord::Secret) {
        base site;
        headers { "authorization" = authorization; }
        POST batch(request: concord::Body) -> concord::ByteStream {
            path "_api/$batch"; body request; read concord::read::Stream;
        }
    }
}

/// Builds a read-only batch from trusted content IDs and GET request targets.
pub fn get_batch(targets: &[(&str, &str)]) -> Result<Body, BoxError> {
    let mut requests = Vec::new();
    for (id, target) in targets {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/http"));
        headers.insert("content-id", HeaderValue::from_str(id)?);
        headers.insert(
            "content-transfer-encoding",
            HeaderValue::from_static("binary"),
        );
        let request = Body::new(
            format!("GET {target} HTTP/1.1\r\nAccept: application/json\r\n\r\n"),
            "application/http",
        );
        requests.push((headers, request));
    }
    Ok(concord::multipart_mixed(requests)?)
}

/// A MIME content ID, when present, and a complete embedded HTTP response.
pub type BatchReply = (Option<String>, Answer<Bytes>);

/// Decodes one buffered HTTP response; transfer-coded responses are refused.
pub fn http_response(bytes: Bytes, method: &Method) -> Result<Answer<Bytes>, BoxError> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "Invalid batch HTTP response.");
    let end = bytes
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .map(|offset| offset + 4)
        .ok_or_else(invalid)?;
    if end > 16 * concord::KIB {
        return Err(invalid().into());
    }
    let mut fields = [httparse::EMPTY_HEADER; 64];
    let mut response = httparse::Response::new(&mut fields);
    let httparse::Status::Complete(offset) = response.parse(&bytes)? else {
        return Err(invalid().into());
    };
    let status = StatusCode::from_u16(response.code.ok_or_else(invalid)?)?;
    if status.is_informational() {
        return Err(invalid().into());
    }
    let mut headers = HeaderMap::new();
    for header in response.headers.iter() {
        headers.append(
            HeaderName::from_bytes(header.name.as_bytes())?,
            HeaderValue::from_bytes(header.value)?,
        );
    }
    if headers.contains_key("transfer-encoding") {
        return Err(invalid().into());
    }
    let payload = bytes.slice(offset..);
    let no_body = method == Method::HEAD
        || status == StatusCode::NO_CONTENT
        || status == StatusCode::NOT_MODIFIED;
    if no_body {
        if !payload.is_empty() {
            return Err(invalid().into());
        }
    } else if headers.contains_key("content-length") {
        let mut lengths = headers.get_all("content-length").iter();
        let length: usize = lengths.next().ok_or_else(invalid)?.to_str()?.parse()?;
        if lengths.next().is_some() || length != payload.len() {
            return Err(invalid().into());
        }
    }
    Ok(Answer::new(status, headers.into(), payload))
}

/// Appends completed GET replies; a later MIME or HTTP failure leaves them available.
pub async fn read_batch(
    body: ByteStream,
    completed: &mut Vec<BatchReply>,
    max_bytes: u64,
) -> Result<(), BoxError> {
    let mut response = parts(body, max_bytes)?;
    while let Some(part) = response.next_field().await? {
        if part.content_type().map(mime::Mime::essence_str) != Some("application/http") {
            return Err(
                io::Error::new(io::ErrorKind::InvalidData, "Expected an HTTP batch part.").into(),
            );
        }
        let id = part
            .headers()
            .get("content-id")
            .map(|value| value.to_str().map(str::to_owned))
            .transpose()?;
        let reply = http_response(part.bytes().await?, &Method::GET)?;
        completed.push((id, reply));
    }
    Ok(())
}

/// Reads two Gmail messages, keeping content IDs for correlation even on failure.
pub async fn gmail_batch(api: &Gmail, completed: &mut Vec<BatchReply>) -> Result<(), BoxError> {
    let start = completed.len();
    let request = get_batch(&[
        ("<one>", "/gmail/v1/users/me/messages/one"),
        ("<two>", "/gmail/v1/users/me/messages/two"),
    ])?;
    let body = api
        .batch(request)
        .with_retry(concord::Retry::none())
        .await?;
    read_batch(body, completed, 4 * concord::MIB as u64).await?;
    let replies = &completed[start..];
    if replies.len() != 2
        || ["response-one", "response-two"].iter().any(|expected| {
            replies
                .iter()
                .filter(|reply| {
                    reply
                        .0
                        .as_deref()
                        .is_some_and(|id| id.trim_matches(['<', '>']) == *expected)
                })
                .count()
                != 1
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Missing or duplicate Gmail reply ID.",
        )
        .into());
    }
    Ok(())
}

/// Reads this SharePoint site's lists and title, keeping the service's reply order.
pub async fn sharepoint_batch(
    api: &SharePoint,
    completed: &mut Vec<BatchReply>,
) -> Result<(), BoxError> {
    let start = completed.len();
    let request = get_batch(&[
        (
            "1",
            "https://tenant.sharepoint.com/sites/demo/_api/web/lists",
        ),
        (
            "2",
            "https://tenant.sharepoint.com/sites/demo/_api/web/title",
        ),
    ])?;
    let body = api
        .batch(request)
        .with_retry(concord::Retry::none())
        .await?;
    read_batch(body, completed, 4 * concord::MIB as u64).await?;
    if completed.len() - start != 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unexpected SharePoint reply count.",
        )
        .into());
    }
    Ok(())
}
