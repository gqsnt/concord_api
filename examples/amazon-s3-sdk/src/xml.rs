//! S3's XML documents: the `ListObjectsV2` result (a Concord [`Decode`] format) and the
//! `<Error>` document of failed calls.
//!
//! Parsing uses `roxmltree` (a read-only XML 1.0 tree; DTD entity expansion is bounded by the
//! crate). Elements are matched by local name, so the S3 namespace
//! (`http://s3.amazonaws.com/doc/2006-03-01/`) may be present or absent, and **unknown elements
//! and attributes are ignored**: S3 adds response elements over time (`ChecksumAlgorithm`,
//! `ChecksumType`, `RestoreStatus` were added after `ListObjectsV2` was published).

use std::collections::BTreeMap;

use concord::bytes::Bytes;
use concord::{CodecError, ContentTypeField, Decode, ResponseHead};
use percent_encoding::percent_decode_str;
use roxmltree::{Document, Node};

use crate::error::ResponseMeta;
use crate::types::{ContinuationToken, ETag, StorageClass};

/// Uses the default `Decode::ACCEPT = None`: no format-added `accept`.
///
/// The response format of `ListObjectsV2`: requires an XML `Content-Type` (`application/xml` or
/// `text/xml`, parameters allowed) before the body is read, then parses `<ListBucketResult>`.
///
/// AWS documents that "a 200 OK response can contain valid or invalid XML": invalid XML, another
/// root element (an `<Error>` inside a 200) or a missing `IsTruncated` is a `Decode` error, never
/// a partial listing.
pub struct ListXml;

fn check_xml_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
    match head.content_type() {
        ContentTypeField::One(value) => {
            let text = value
                .to_str()
                .map_err(|_| CodecError::new("the response Content-Type is not visible ASCII"))?;
            let essence = text.split(';').next().unwrap_or("").trim();
            if essence.eq_ignore_ascii_case("application/xml")
                || essence.eq_ignore_ascii_case("text/xml")
            {
                Ok(())
            } else {
                Err(CodecError::new(
                    "the response Content-Type is not application/xml or text/xml",
                ))
            }
        }
        ContentTypeField::Missing => Err(CodecError::new("the XML response has no Content-Type")),
        ContentTypeField::Repeated => Err(CodecError::new(
            "the response has more than one Content-Type",
        )),
    }
}

impl Decode<ListObjectsV2Output> for ListXml {
    type Context = ResponseMeta;

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        check_xml_content_type(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<ResponseMeta, CodecError> {
        Ok(ResponseMeta::from_headers(head.status(), head.headers()))
    }

    fn decode_body(meta: ResponseMeta, body: Bytes) -> Result<ListObjectsV2Output, CodecError> {
        parse_listing(&body, meta)
    }
}

/// One page of `ListObjectsV2`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ListObjectsV2Output {
    /// `<Name>`: the bucket.
    pub name: Option<String>,
    /// `<Prefix>` (decoded when the listing was URL-encoded).
    pub prefix: Option<String>,
    /// `<Delimiter>` (decoded when URL-encoded).
    pub delimiter: Option<String>,
    /// `<StartAfter>` (decoded when URL-encoded).
    pub start_after: Option<String>,
    /// `<MaxKeys>`.
    pub max_keys: Option<u32>,
    /// `<KeyCount>`: keys and common prefixes on this page, as S3 counts them (not checked).
    pub key_count: Option<u32>,
    /// `<IsTruncated>`: more results follow.
    pub is_truncated: bool,
    /// `<ContinuationToken>`: the token this page was requested with.
    pub continuation_token: Option<ContinuationToken>,
    /// `<NextContinuationToken>`: the token of the next page (when truncated).
    pub next_continuation_token: Option<ContinuationToken>,
    /// `<EncodingType>`: `url` when S3 percent-encoded the key fields (the SDK decoded them).
    pub encoding_type: Option<String>,
    /// `<Contents>`, in S3's order (lexicographic by key for general-purpose buckets).
    pub contents: Vec<ObjectSummary>,
    /// `<CommonPrefixes>`: the keys rolled up by the delimiter.
    pub common_prefixes: Vec<String>,
    /// Status and request ids of the response.
    pub meta: ResponseMeta,
}

/// One `<Contents>` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ObjectSummary {
    /// `<Key>` (decoded when the listing was URL-encoded).
    pub key: String,
    /// `<LastModified>`, as S3 wrote it (ISO 8601, `2009-10-12T17:50:30.000Z`).
    pub last_modified: Option<String>,
    /// `<ETag>`: an opaque validator, not necessarily a digest of the content.
    pub etag: Option<ETag>,
    /// `<Size>` in bytes.
    pub size: Option<u64>,
    /// `<StorageClass>`.
    pub storage_class: Option<StorageClass>,
    /// `<Owner>` (only with `fetch-owner=true`).
    pub owner: Option<Owner>,
    /// `<ChecksumAlgorithm>` entries.
    pub checksum_algorithms: Vec<String>,
}

/// An object's `<Owner>`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Owner {
    /// `<ID>`.
    pub id: Option<String>,
    /// `<DisplayName>` (AWS is retiring it; often absent).
    pub display_name: Option<String>,
}

fn children<'a, 'i>(node: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children()
        .filter(move |c| c.is_element() && c.tag_name().name() == name)
}

/// The text of the single child `name`; a repeated element is an error (it would make the page
/// ambiguous), an absent one is `None`. An empty element (`<Prefix/>`) is `Some("")`.
fn single(node: Node<'_, '_>, name: &'static str) -> Result<Option<String>, CodecError> {
    let mut found = children(node, name);
    let first = found.next();
    if found.next().is_some() {
        return Err(CodecError::new(
            "an S3 XML element that occurs once is repeated",
        ));
    }
    Ok(first.map(|n| n.text().unwrap_or("").to_owned()))
}

fn number<T: std::str::FromStr>(text: Option<String>) -> Result<Option<T>, CodecError> {
    text.map(|t| {
        t.trim()
            .parse::<T>()
            .map_err(|_| CodecError::new("an S3 XML number does not parse"))
    })
    .transpose()
}

fn boolean(text: Option<String>) -> Result<Option<bool>, CodecError> {
    match text.as_deref().map(str::trim) {
        None => Ok(None),
        Some("true") => Ok(Some(true)),
        Some("false") => Ok(Some(false)),
        Some(_) => Err(CodecError::new(
            "an S3 XML boolean is neither true nor false",
        )),
    }
}

fn parse(body: &[u8]) -> Result<Document<'_>, CodecError> {
    let text = std::str::from_utf8(body).map_err(|e| {
        CodecError::new("the S3 XML document is not UTF-8").at("byte", e.valid_up_to() as u64)
    })?;
    Document::parse(text).map_err(|e| {
        let pos = e.pos();
        CodecError::new("the S3 XML document is not well-formed")
            .at("line", u64::from(pos.row))
            .at("column", u64::from(pos.col))
            .with_source(e)
    })
}

/// Decodes a `url`-encoded listing field: `+` is a space and `%XX` a byte (S3 encodes a literal
/// `+` as `%2B`, so reading `+` as a space is safe either way).
fn url_decode(text: String) -> Result<String, CodecError> {
    let spaced = text.replace('+', " ");
    percent_decode_str(&spaced)
        .decode_utf8()
        .map(|s| s.into_owned())
        .map_err(|_| CodecError::new("a URL-encoded listing field is not UTF-8"))
}

pub(crate) fn parse_listing(
    body: &[u8],
    meta: ResponseMeta,
) -> Result<ListObjectsV2Output, CodecError> {
    let doc = parse(body)?;
    let root = doc.root_element();
    match root.tag_name().name() {
        "ListBucketResult" => {}
        "Error" => {
            return Err(CodecError::new(
                "a 2xx listing response carried an S3 <Error> document",
            ));
        }
        _ => {
            return Err(CodecError::new(
                "the XML document is not a <ListBucketResult>",
            ));
        }
    }
    let encoding_type = single(root, "EncodingType")?;
    let encoded = encoding_type.as_deref() == Some("url");
    let decode = |value: Option<String>| -> Result<Option<String>, CodecError> {
        match value {
            Some(v) if encoded => url_decode(v).map(Some),
            other => Ok(other),
        }
    };
    let token = |value: Option<String>| -> Result<Option<ContinuationToken>, CodecError> {
        value
            .filter(|t| !t.is_empty())
            .map(|t| {
                ContinuationToken::new(t)
                    .map_err(|_| CodecError::new("an S3 continuation token is invalid"))
            })
            .transpose()
    };
    let is_truncated = boolean(single(root, "IsTruncated")?)?
        .ok_or_else(|| CodecError::new("the listing has no <IsTruncated>"))?;

    let mut contents = Vec::new();
    for (index, node) in children(root, "Contents").enumerate() {
        let at = |e: CodecError| e.at("record", index as u64 + 1);
        let key = decode(single(node, "Key").map_err(at)?)
            .map_err(at)?
            .ok_or_else(|| at(CodecError::new("a <Contents> entry has no <Key>")))?;
        let etag = single(node, "ETag")
            .map_err(at)?
            .map(|t| {
                ETag::parse(t)
                    .map_err(|_| at(CodecError::new("a <Contents> ETag is not an entity tag")))
            })
            .transpose()?;
        let owner = match children(node, "Owner").next() {
            Some(owner) => Some(Owner {
                id: single(owner, "ID").map_err(at)?,
                display_name: single(owner, "DisplayName").map_err(at)?,
            }),
            None => None,
        };
        contents.push(ObjectSummary {
            key,
            last_modified: single(node, "LastModified").map_err(at)?,
            etag,
            size: number(single(node, "Size").map_err(at)?).map_err(at)?,
            storage_class: single(node, "StorageClass")
                .map_err(at)?
                .map(|s| StorageClass::from_wire(&s)),
            owner,
            checksum_algorithms: children(node, "ChecksumAlgorithm")
                .map(|n| n.text().unwrap_or("").to_owned())
                .collect(),
        });
    }
    let mut common_prefixes = Vec::new();
    for node in children(root, "CommonPrefixes") {
        if let Some(prefix) = decode(single(node, "Prefix")?)? {
            common_prefixes.push(prefix);
        }
    }
    Ok(ListObjectsV2Output {
        name: single(root, "Name")?,
        prefix: decode(single(root, "Prefix")?)?,
        delimiter: decode(single(root, "Delimiter")?)?,
        start_after: decode(single(root, "StartAfter")?)?,
        max_keys: number(single(root, "MaxKeys")?)?,
        key_count: number(single(root, "KeyCount")?)?,
        is_truncated,
        continuation_token: token(single(root, "ContinuationToken")?)?,
        next_continuation_token: token(single(root, "NextContinuationToken")?)?,
        encoding_type,
        contents,
        common_prefixes,
        meta,
    })
}

/// The unredacted fields of an S3 REST `<Error>` document.
#[derive(Debug, Default)]
pub struct ErrorDocument {
    /// `<Code>`.
    pub code: Option<String>,
    /// `<Message>`.
    pub message: Option<String>,
    /// `<Resource>`.
    pub resource: Option<String>,
    /// `<RequestId>`.
    pub request_id: Option<String>,
    /// `<HostId>`.
    pub host_id: Option<String>,
}

/// S3's REST error document codec. The declaration explicitly accepts mislabeled bodies,
/// matching the SDK's existing error reader; successful listings still require XML media.
pub struct S3ErrorXml;

impl Decode<ErrorDocument> for S3ErrorXml {
    type Context = ();

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        check_xml_content_type(head)
    }

    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode_body((): (), body: Bytes) -> Result<ErrorDocument, CodecError> {
        parse_error_document(&body).ok_or_else(|| CodecError::new("not an S3 Error document"))
    }
}

/// Parses an `<Error>` document; `None` for anything else (invalid XML, another root).
pub(crate) fn parse_error_document(body: &[u8]) -> Option<ErrorDocument> {
    let doc = parse(body).ok()?;
    let root = doc.root_element();
    if root.tag_name().name() != "Error" {
        return None;
    }
    let field = |name| single(root, name).ok().flatten();
    Some(ErrorDocument {
        code: field("Code"),
        message: field("Message"),
        resource: field("Resource"),
        request_id: field("RequestId"),
        host_id: field("HostId"),
    })
}

/// `x-amz-meta-*` response headers, by lowercase name without the prefix. A value that is not
/// UTF-8 is refused (S3 returns non-ASCII metadata RFC 2047-encoded, i.e. ASCII).
pub(crate) fn metadata_headers(
    headers: &concord::reqwest::header::HeaderMap,
) -> Result<BTreeMap<String, String>, CodecError> {
    let mut entries = BTreeMap::new();
    for (name, value) in headers {
        if let Some(meta) = name.as_str().strip_prefix("x-amz-meta-") {
            let text = std::str::from_utf8(value.as_bytes())
                .map_err(|_| CodecError::new("an x-amz-meta-* response header is not UTF-8"))?;
            entries.insert(meta.to_owned(), text.to_owned());
        }
    }
    Ok(entries)
}
