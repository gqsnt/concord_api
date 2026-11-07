use crate::{Author, Category, Feed, Link, Paper, Problem};
use concord::{CodecError, Decode, ResponseHead, bytes::Bytes};
use roxmltree::{Document, Node, ParsingOptions};

const ATOM: &str = "http://www.w3.org/2005/Atom";
const ARXIV: &str = "http://arxiv.org/schemas/atom";
const OPEN: &str = "http://a9.com/-/spec/opensearch/1.1/";

/// Namespace-aware UTF-8 Atom format, with strict media checks and bounded XML nodes.
pub struct Atom;
impl Decode<Feed> for Atom {
    type Context = ();
    const ACCEPT: Option<&'static str> = Some("application/atom+xml");
    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        concord_codecs::check_content_type(
            head,
            &["application/atom+xml"],
            concord_codecs::Charset::Utf8,
        )
    }
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: Bytes) -> Result<Feed, CodecError> {
        let text = std::str::from_utf8(&body).map_err(|_| bad("Atom is not UTF-8"))?;
        let doc = Document::parse_with_options(
            text,
            ParsingOptions {
                allow_dtd: false,
                nodes_limit: 50_000,
                ..ParsingOptions::default()
            },
        )
        .map_err(|_| bad("malformed or excessive Atom XML"))?;
        let root = doc.root_element();
        if !root.has_tag_name((ATOM, "feed")) {
            return Err(bad("expected Atom feed"));
        }
        let mut entries = Vec::new();
        let mut problem = None;
        for entry in root.children().filter(|n| n.has_tag_name((ATOM, "entry"))) {
            let id = required(entry, ATOM, "id")?;
            if id.starts_with("http://arxiv.org/api/errors#")
                || id.starts_with("https://arxiv.org/api/errors#")
            {
                if problem.is_some() {
                    return Err(bad("multiple Atom error entries"));
                }
                problem = Some(Problem {
                    id,
                    summary: required(entry, ATOM, "summary")?,
                });
            } else {
                entries.push(paper(entry, id)?);
            }
        }
        if problem.is_some() && !entries.is_empty() {
            return Err(bad("mixed papers and Atom error"));
        }
        let count = |name| -> Result<u64, CodecError> {
            match optional(root, OPEN, name)? {
                None if problem.is_some() => Ok(0),
                Some(s) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) => {
                    s.parse().map_err(|_| bad("OpenSearch count overflow"))
                }
                _ => Err(bad("missing or invalid OpenSearch count")),
            }
        };
        Ok(Feed {
            total_results: count("totalResults")?,
            start: count("startIndex")?,
            items_per_page: count("itemsPerPage")?,
            entries,
            problem,
        })
    }
}

fn bad(label: &'static str) -> CodecError {
    CodecError::new(label)
}
fn one<'a, 'i>(
    node: Node<'a, 'i>,
    ns: &str,
    name: &str,
) -> Result<Option<Node<'a, 'i>>, CodecError> {
    let mut found = node.children().filter(|n| n.has_tag_name((ns, name)));
    let first = found.next();
    if found.next().is_some() {
        return Err(bad("duplicate Atom field"));
    }
    Ok(first)
}
fn text(node: Node<'_, '_>) -> String {
    node.children()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect::<String>()
        .trim()
        .to_owned()
}
fn optional(node: Node<'_, '_>, ns: &str, name: &str) -> Result<Option<String>, CodecError> {
    Ok(one(node, ns, name)?.map(text))
}
fn required(node: Node<'_, '_>, ns: &str, name: &str) -> Result<String, CodecError> {
    optional(node, ns, name)?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("missing required Atom field"))
}
fn attr(node: Node<'_, '_>, name: &str) -> Result<String, CodecError> {
    node.attribute(name)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| bad("missing Atom attribute"))
}
fn category(node: Node<'_, '_>) -> Result<Category, CodecError> {
    Ok(Category {
        term: attr(node, "term")?,
        scheme: node.attribute("scheme").map(str::to_owned),
    })
}
fn paper(node: Node<'_, '_>, id: String) -> Result<Paper, CodecError> {
    let mut authors = Vec::new();
    let mut categories = Vec::new();
    let mut links = Vec::new();
    for child in node.children() {
        if child.has_tag_name((ATOM, "author")) {
            authors.push(Author {
                name: required(child, ATOM, "name")?,
                affiliations: child
                    .children()
                    .filter(|n| n.has_tag_name((ARXIV, "affiliation")))
                    .map(text)
                    .collect(),
            });
        } else if child.has_tag_name((ATOM, "category")) {
            categories.push(category(child)?);
        } else if child.has_tag_name((ATOM, "link")) {
            links.push(Link {
                href: attr(child, "href")?,
                rel: child.attribute("rel").map(str::to_owned),
                title: child.attribute("title").map(str::to_owned),
                media_type: child.attribute("type").map(str::to_owned),
            });
        }
    }
    Ok(Paper {
        id,
        title: required(node, ATOM, "title")?,
        summary: required(node, ATOM, "summary")?,
        published: required(node, ATOM, "published")?,
        updated: required(node, ATOM, "updated")?,
        authors,
        categories,
        links,
        primary_category: one(node, ARXIV, "primary_category")?
            .map(category)
            .transpose()?,
        comment: optional(node, ARXIV, "comment")?,
        journal_ref: optional(node, ARXIV, "journal_ref")?,
        doi: optional(node, ARXIV, "doi")?,
    })
}
