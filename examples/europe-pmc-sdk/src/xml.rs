use crate::{
    Cursor, Publication, Reference, ReferenceList, ReferencesPage, ResultList, SearchPage,
};
use concord::{CodecError, Decode, ResponseHead, bytes::Bytes};
use roxmltree::{Document, Node, ParsingOptions};

/// SDK XML search decoder. Shares Concord's strict media parser and buffers at most 256 KiB.
/// UTF-8/US-ASCII only; external entities are never resolved and DTD entities are refused.
pub struct SearchXml;

/// SDK XML reference-list decoder, with exactly the media, size and parser rules of
/// [`SearchXml`]. A missing `referenceList` reads as an empty list.
pub struct ReferencesXml;

impl Decode<SearchPage> for SearchXml {
    type Context = ();
    const ACCEPT: Option<&'static str> = Some("application/xml");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        check_xml_media(head)
    }

    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode_body((): (), body: Bytes) -> Result<SearchPage, CodecError> {
        wrapper(&body, |root, hits| {
            let cursor = optional(root, "nextCursorMark")?
                .map(|s| Cursor::new(s).map_err(|_| bad("invalid XML cursor mark")))
                .transpose()?;
            let list = one(root, "resultList")?.ok_or_else(|| bad("missing XML resultList"))?;
            let result = list
                .children()
                .filter(|n| n.has_tag_name("result"))
                .map(publication)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SearchPage {
                hit_count: hits,
                next_cursor_mark: cursor,
                result_list: ResultList { result },
            })
        })
    }
}

impl Decode<ReferencesPage> for ReferencesXml {
    type Context = ();
    const ACCEPT: Option<&'static str> = Some("application/xml");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        check_xml_media(head)
    }

    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode_body((): (), body: Bytes) -> Result<ReferencesPage, CodecError> {
        wrapper(&body, |root, hits| {
            let reference = match one(root, "referenceList")? {
                Some(list) => list
                    .children()
                    .filter(|n| n.has_tag_name("reference"))
                    .map(reference)
                    .collect::<Result<Vec<_>, _>>()?,
                None => Vec::new(),
            };
            Ok(ReferencesPage {
                hit_count: hits,
                reference_list: ReferenceList { reference },
            })
        })
    }
}

fn check_xml_media(head: &ResponseHead<'_>) -> Result<(), CodecError> {
    concord_codecs::check_content_type(
        head,
        &["application/xml", "text/xml"],
        concord_codecs::Charset::Utf8,
    )
}

/// Parses a bounded `responseWrapper` document and its required unsigned `hitCount`.
fn wrapper<T>(
    body: &Bytes,
    build: impl FnOnce(Node<'_, '_>, u64) -> Result<T, CodecError>,
) -> Result<T, CodecError> {
    let text = std::str::from_utf8(body).map_err(|_| bad("XML body is not UTF-8"))?;
    let doc = Document::parse_with_options(
        text,
        ParsingOptions {
            nodes_limit: 20_000,
            ..ParsingOptions::default()
        },
    )
    .map_err(|e| bad("invalid Europe PMC XML").with_source(e))?;
    let root = doc.root_element();
    if !root.has_tag_name("responseWrapper") {
        return Err(bad("XML root is not responseWrapper"));
    }
    let hit_text = required(root, "hitCount")?;
    if !hit_text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad("XML hitCount is not unsigned decimal"));
    }
    let hits = hit_text
        .parse::<u64>()
        .map_err(|_| bad("XML hitCount is not unsigned decimal"))?;
    build(root, hits)
}

fn bad(label: &'static str) -> CodecError {
    CodecError::new(label)
}

fn one<'a, 'b>(node: Node<'a, 'b>, name: &str) -> Result<Option<Node<'a, 'b>>, CodecError> {
    let mut matches = node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == name);
    let first = matches.next();
    if matches.next().is_some() {
        return Err(bad("repeated XML singleton field"));
    }
    Ok(first)
}

fn optional(node: Node<'_, '_>, name: &str) -> Result<Option<String>, CodecError> {
    one(node, name)?
        .map(|node| {
            // Scalar fields must not silently truncate nested markup. Escaped markup remains text.
            if node.children().any(|n| n.is_element()) {
                return Err(bad("nested XML scalar field"));
            }
            Ok(node
                .children()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .collect())
        })
        .transpose()
}

fn required(node: Node<'_, '_>, name: &str) -> Result<String, CodecError> {
    optional(node, name)?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("missing required XML scalar"))
}

fn publication(node: Node<'_, '_>) -> Result<Publication, CodecError> {
    Ok(Publication {
        id: required(node, "id")?
            .try_into()
            .map_err(|_| bad("invalid XML publication identifier"))?,
        source: required(node, "source")?
            .try_into()
            .map_err(|_| bad("invalid XML source"))?,
        title: optional(node, "title")?,
        author_string: optional(node, "authorString")?,
        journal_title: optional(node, "journalTitle")?,
        pub_year: optional(node, "pubYear")?,
        doi: optional(node, "doi")?,
        abstract_text: optional(node, "abstractText")?,
    })
}

/// Identity is optional (an unmatched reference), but a present identity must be valid.
fn reference(node: Node<'_, '_>) -> Result<Reference, CodecError> {
    Ok(Reference {
        id: optional(node, "id")?
            .map(|s| s.try_into())
            .transpose()
            .map_err(|_| bad("invalid XML publication identifier"))?,
        source: optional(node, "source")?
            .map(|s| s.try_into())
            .transpose()
            .map_err(|_| bad("invalid XML source"))?,
        title: optional(node, "title")?,
        author_string: optional(node, "authorString")?,
        journal_abbreviation: optional(node, "journalAbbreviation")?,
        pub_year: optional(node, "pubYear")?,
    })
}
