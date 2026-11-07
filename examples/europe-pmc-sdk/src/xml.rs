use crate::{Cursor, Publication, ResultList, SearchPage};
use concord::{CodecError, Decode, ResponseHead, bytes::Bytes};
use roxmltree::{Document, Node, ParsingOptions};

/// SDK XML search decoder. Shares Concord's strict media parser and buffers at most 256 KiB.
/// UTF-8/US-ASCII only; external entities are never resolved and DTD entities are refused.
pub struct SearchXml;

impl Decode<SearchPage> for SearchXml {
    type Context = ();
    const ACCEPT: Option<&'static str> = Some("application/xml");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        concord_codecs::check_content_type(
            head,
            &["application/xml", "text/xml"],
            concord_codecs::Charset::Utf8,
        )
    }

    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode_body((): (), body: Bytes) -> Result<SearchPage, CodecError> {
        let text = std::str::from_utf8(&body).map_err(|_| bad("XML body is not UTF-8"))?;
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
    }
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
