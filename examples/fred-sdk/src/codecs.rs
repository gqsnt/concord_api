use std::io::{Cursor, Read};

use concord::{CodecError, Decode, ResponseHead, bytes::Bytes};
use roxmltree::{Document, Node};
use serde::de::DeserializeOwned;

use crate::{
    BODY_LIMIT, CsvExport, ENTRY_LIMIT, EXTRACTED_LIMIT, Metadata, Observations, Problem,
    SearchPage,
};

/// FRED XML response format, with DTDs disabled and a bounded node count.
pub struct Xml;
/// ZIP format for FRED's `file_type=csv`; never a plain CSV decoder.
pub struct ZippedCsv;
pub(crate) struct ProblemDocument;

fn media(head: &ResponseHead<'_>, accepted: &[&str]) -> Result<(), CodecError> {
    let mut fields = head.headers().get_all("content-type").iter();
    let value = fields
        .next()
        .ok_or_else(|| CodecError::new("missing Content-Type"))?;
    if fields.next().is_some() {
        return Err(CodecError::new("repeated Content-Type"));
    }
    let parsed = value
        .to_str()
        .ok()
        .and_then(|s| s.parse::<mime::Mime>().ok())
        .ok_or_else(|| CodecError::new("invalid Content-Type"))?;
    if !accepted
        .iter()
        .any(|m| parsed.essence_str().eq_ignore_ascii_case(m))
    {
        return Err(CodecError::new("unexpected Content-Type"));
    }
    let charsets: Vec<_> = parsed
        .params()
        .filter(|(name, _)| *name == mime::CHARSET)
        .collect();
    if charsets.len() > 1
        || charsets.iter().any(|(_, v)| {
            !v.as_str().eq_ignore_ascii_case("utf-8")
                && !v.as_str().eq_ignore_ascii_case("us-ascii")
        })
    {
        return Err(CodecError::new("unsupported charset"));
    }
    Ok(())
}

fn xml(text: &str) -> Result<Document<'_>, CodecError> {
    Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 100_000,
            ..Default::default()
        },
    )
    .map_err(|_| CodecError::new("invalid FRED XML"))
}

fn text(body: &Bytes) -> Result<&str, CodecError> {
    if body.len() > BODY_LIMIT {
        return Err(CodecError::new("FRED document exceeds byte limit"));
    }
    std::str::from_utf8(body).map_err(|_| CodecError::new("FRED document is not UTF-8"))
}

// XML attributes map to the same provider model as JSON. Only known numeric attributes
// are converted; values and dates remain strings. Unknown attributes are forward-compatible.
fn attributes(node: Node<'_, '_>) -> Result<serde_json::Value, CodecError> {
    let mut object = serde_json::Map::new();
    for attr in node.attributes() {
        let value = if matches!(
            attr.name(),
            "count" | "offset" | "limit" | "popularity" | "group_popularity"
        ) {
            serde_json::Value::from(
                attr.value()
                    .parse::<u64>()
                    .map_err(|_| CodecError::new("invalid numeric XML attribute"))?,
            )
        } else {
            serde_json::Value::from(attr.value())
        };
        object.insert(attr.name().to_owned(), value);
    }
    Ok(serde_json::Value::Object(object))
}

fn envelope<T: DeserializeOwned>(
    body: Bytes,
    root_name: &str,
    item_name: &str,
    array_name: &str,
) -> Result<T, CodecError> {
    let doc = xml(text(&body)?)?;
    let root = doc.root_element();
    if !root.has_tag_name(root_name) {
        return Err(CodecError::new("unexpected FRED XML root"));
    }
    let mut value = attributes(root)?;
    let mut items = Vec::new();
    for child in root.children().filter(Node::is_element) {
        if !child.has_tag_name(item_name) || child.children().any(|n| n.is_element() || n.is_text())
        {
            return Err(CodecError::new("unexpected FRED XML item"));
        }
        items.push(attributes(child)?);
    }
    value
        .as_object_mut()
        .expect("attributes creates an object")
        .insert(array_name.to_owned(), serde_json::Value::Array(items));
    serde_json::from_value(value).map_err(|_| CodecError::new("invalid FRED XML model"))
}

macro_rules! xml_decoder {
    ($out:ty, $root:literal, $item:literal, $array:literal) => {
        impl Decode<$out> for Xml {
            type Context = ();
            const ACCEPT: Option<&'static str> = Some("text/xml");
            fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
                media(head, &["text/xml", "application/xml"])
            }
            fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
                Ok(())
            }
            fn decode_body((): (), body: Bytes) -> Result<$out, CodecError> {
                envelope(body, $root, $item, $array)
            }
        }
    };
}
xml_decoder!(SearchPage, "seriess", "series", "seriess");
xml_decoder!(Metadata, "seriess", "series", "seriess");
xml_decoder!(Observations, "observations", "observation", "observations");

impl Decode<Problem> for ProblemDocument {
    type Context = ();
    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        media(head, &["text/xml", "application/xml", "application/json"])
    }
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: Bytes) -> Result<Problem, CodecError> {
        let input = text(&body)?;
        // The official errors page labels its JSON example text/xml. Sniff only
        // within these accepted error-document media types, never success bodies.
        if input.trim_start().starts_with('{') {
            return serde_json::from_str(input)
                .map_err(|_| CodecError::new("invalid FRED JSON problem"));
        }
        let doc = xml(input)?;
        let root = doc.root_element();
        if !root.has_tag_name("error") {
            return Err(CodecError::new("invalid FRED XML problem"));
        }
        Ok(Problem {
            error_code: root
                .attribute("code")
                .and_then(|n| n.parse().ok())
                .ok_or_else(|| CodecError::new("invalid FRED problem code"))?,
            error_message: root
                .attribute("message")
                .ok_or_else(|| CodecError::new("missing FRED problem message"))?
                .to_owned(),
        })
    }
}

impl Decode<CsvExport> for ZippedCsv {
    type Context = ();
    const ACCEPT: Option<&'static str> = Some("application/zip");
    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        media(
            head,
            &[
                "application/zip",
                "application/x-zip-compressed",
                "application/octet-stream",
            ],
        )
    }
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: Bytes) -> Result<CsvExport, CodecError> {
        if body.len() > BODY_LIMIT {
            return Err(CodecError::new("ZIP exceeds compressed byte limit"));
        }
        if !body.starts_with(b"PK\x03\x04") {
            return Err(CodecError::new("CSV export is not a ZIP archive"));
        }
        let bad = || CodecError::new("invalid or unsupported FRED ZIP");
        let mut archive = zip::ZipArchive::new(Cursor::new(body)).map_err(|_| bad())?;
        if archive.len() > ENTRY_LIMIT {
            return Err(CodecError::new("ZIP exceeds entry limit"));
        }
        let (mut csv, mut readme) = (None, None);
        let mut total = 0usize;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|_| bad())?;
            let name = file.name().to_ascii_lowercase();
            if name.contains('/')
                || name.contains('\\')
                || file.is_dir()
                || file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
            {
                return Err(bad());
            }
            let slot = if name.ends_with(".csv") {
                &mut csv
            } else if name == "readme.txt" {
                &mut readme
            } else {
                return Err(bad());
            };
            if slot.is_some() {
                return Err(CodecError::new("ambiguous ZIP members"));
            }
            let remaining = EXTRACTED_LIMIT - total;
            if file.size() > remaining as u64 {
                return Err(CodecError::new("ZIP exceeds extracted byte limit"));
            }
            let mut data = Vec::new();
            // Do not trust the central directory's claimed decompressed size.
            file.by_ref()
                .take(remaining as u64 + 1)
                .read_to_end(&mut data)
                .map_err(|_| bad())?;
            if data.len() > remaining {
                return Err(CodecError::new("ZIP exceeds extracted byte limit"));
            }
            if data.len() as u64 != file.size() {
                return Err(bad());
            }
            total += data.len();
            *slot = Some(data);
        }
        Ok(CsvExport {
            csv: csv.ok_or_else(|| CodecError::new("ZIP has no CSV member"))?,
            readme,
        })
    }
}
