//! Strict `Link` field parsing, enabled by the `link` feature. No HTTP types are required.
//!
//! [`parse`] returns links in field/value order, resolving every target against the response
//! URL. [`relation`] distinguishes absence, one link and several links, even when their targets
//! are equal. [`parse_page_links`] projects the four pagination relations (`next`, `prev`,
//! `first`, `last`), refusing two different targets for one relation. Which relations to follow,
//! origin/path checks, and `anchor` interpretation belong to the caller. Parameters retain their original spelling and
//! raw values; unknown names are accepted, including parameters without a value.
//!
//! Empty fields, whitespace-only fields and empty comma-separated list elements are ignored.
//! An empty target (`<>`) is a valid relative reference to the response URL. Empty quoted
//! parameter values are accepted; an empty unquoted value, bare `rel`, trailing `;`, missing
//! delimiter or unterminated string is an error. The first `rel` wins, even if it is `rel=""`:
//! this follows [RFC 8288 §3.3](https://www.rfc-editor.org/rfc/rfc8288.html#section-3.3) and
//! prevents later parameters from changing a link's meaning. Later parameters are still checked
//! for syntactic validity. Relation types are unescaped, split on ASCII whitespace and compared
//! without ASCII case sensitivity; parameter names are also case-insensitive.
//!
//! Input must be visible ASCII plus horizontal tabs. Non-UTF-8 bytes, non-ASCII UTF-8 and control
//! bytes are rejected rather than decoded lossily. Targets must use URI characters (percent
//! escapes for other bytes); URL resolution uses [`url::Url::join`]. Each field must be complete:
//! a quote or target cannot span field lines. Errors contain only static descriptions and byte
//! offsets into the field values as if joined by one comma, including empty fields. `Debug`
//! withholds targets, relations and parameter contents, which may contain sensitive data.
//!
//! ```
//! use concord_pagination::link::{self, Relation};
//! let base = url::Url::parse("https://example.test/items?page=1")?;
//! let links = link::parse([b"<?page=2>; REL=\"next last\"".as_slice()], &base)?;
//! match link::relation(&links, "next") {
//!     Relation::One(link) => assert_eq!(link.target.query(), Some("page=2")),
//!     Relation::Absent | Relation::Several => panic!("expected one next link"),
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::fmt;
use url::Url;

/// One link-value, without provider-specific interpretation.
#[derive(Clone, PartialEq, Eq)]
pub struct Link {
    /// Target resolved against the response URL; not checked for origin, path or credentials.
    pub target: Url,
    /// Target exactly as sent between `<` and `>`, before URL normalization.
    pub raw_target: String,
    /// Relation types from the first `rel`, unescaped and ASCII-lowercased, in order.
    pub relations: Vec<String>,
    /// All parameters, including `rel`, duplicates and unknown names, in order.
    pub parameters: Vec<Parameter>,
}

impl Link {
    /// Whether the first `rel` contains this relation (ASCII case-insensitive).
    pub fn has_relation(&self, relation: &str) -> bool {
        self.relations
            .iter()
            .any(|r| r.eq_ignore_ascii_case(relation))
    }
}

impl fmt::Debug for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Link")
            .field("relations", &self.relations.len())
            .field("parameters", &self.parameters.len())
            .finish_non_exhaustive()
    }
}

/// A parameter whose value has no provider-specific interpretation.
#[derive(Clone, PartialEq, Eq)]
pub struct Parameter {
    /// Original name; compare with `eq_ignore_ascii_case`.
    pub name: String,
    /// Original token or quoted string, including quotes and escapes, excluding surrounding
    /// whitespace. `None` means no `=`; `Some("\"\"")` means an empty quoted value.
    pub raw_value: Option<String>,
}

impl fmt::Debug for Parameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Parameter")
            .field("has_value", &self.raw_value.is_some())
            .finish_non_exhaustive()
    }
}

/// Number of link-values carrying a relation. Identical targets are not deduplicated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation<'a> {
    /// No link carries the relation.
    Absent,
    /// Exactly one link carries the relation.
    One(&'a Link),
    /// More than one link carries the relation; the caller decides whether this is an error.
    Several,
}

/// Looks up a relation without assigning an ambiguity policy. Repeating a relation inside
/// one `rel` value still counts as one link.
pub fn relation<'a>(links: &'a [Link], relation: &str) -> Relation<'a> {
    let mut matches = links.iter().filter(|link| link.has_relation(relation));
    match (matches.next(), matches.next()) {
        (None, _) => Relation::Absent,
        (Some(link), None) => Relation::One(link),
        (Some(_), Some(_)) => Relation::Several,
    }
}

/// Malformed field syntax. Offsets are zero-based bytes in fields joined by one comma.
/// Neither `Debug` nor `Display` includes any input text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// A byte is neither visible ASCII nor a horizontal tab.
    NotText {
        /// Offset of the invalid byte.
        at: usize,
    },
    /// A link-value does not start with `<`.
    ExpectedTarget {
        /// Offset where `<` was expected.
        at: usize,
    },
    /// A target or quoted string is not terminated within its field.
    Unterminated {
        /// Offset of its opening delimiter.
        at: usize,
    },
    /// A parameter name, value or delimiter is malformed, or `rel` has no value.
    Parameter {
        /// Offset where the parameter was malformed.
        at: usize,
    },
    /// A target contains a non-URI character, a bad percent escape, or cannot resolve.
    Url {
        /// Offset of the invalid byte, or start of the unresolvable target.
        at: usize,
    },
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (reason, at) = match *self {
            Self::NotText { at } => ("not visible ASCII text", at),
            Self::ExpectedTarget { at } => ("expected `<`", at),
            Self::Unterminated { at } => ("unterminated target or string", at),
            Self::Parameter { at } => ("malformed parameter", at),
            Self::Url { at } => ("invalid URL reference", at),
        };
        write!(f, "Link header: {reason} at byte {at}")
    }
}

impl std::error::Error for LinkError {}

/// Parses every field value, in order, without making HTTP requests or selecting relations.
/// Pass the URL of the response carrying these fields as `base`.
///
/// # Errors
/// Returns [`LinkError`] on malformed input, including invalid targets on unknown relations.
/// No partially parsed listing is returned.
pub fn parse<'a>(
    values: impl IntoIterator<Item = &'a [u8]>,
    base: &Url,
) -> Result<Vec<Link>, LinkError> {
    let mut links = Vec::new();
    let mut offset = 0;
    for bytes in values {
        if let Some(at) = bytes
            .iter()
            .position(|&b| b != b'\t' && !(b' '..=b'~').contains(&b))
        {
            return Err(LinkError::NotText { at: offset + at });
        }
        // ASCII was checked above, so this conversion cannot fail.
        let text = std::str::from_utf8(bytes).expect("checked ASCII");
        let mut cursor = Cursor {
            text,
            at: 0,
            offset,
        };
        loop {
            cursor.skip(|b| matches!(b, b' ' | b'\t' | b','));
            if cursor.peek().is_none() {
                break;
            }
            let start = cursor.at;
            if cursor.peek() != Some(b'<') {
                return Err(LinkError::ExpectedTarget { at: offset + start });
            }
            let end = text[start + 1..]
                .find('>')
                .map(|i| start + 1 + i)
                .ok_or(LinkError::Unterminated { at: offset + start })?;
            let raw_target = &text[start + 1..end];
            validate_target(raw_target, offset + start + 1)?;
            cursor.at = end + 1;
            let mut relations = None;
            let mut parameters = Vec::new();
            loop {
                cursor.whitespace();
                match cursor.peek() {
                    None | Some(b',') => break,
                    Some(b';') => cursor.at += 1,
                    Some(_) => return Err(cursor.parameter_error()),
                }
                cursor.whitespace();
                let name = cursor.token();
                if name.is_empty() {
                    return Err(cursor.parameter_error());
                }
                cursor.whitespace();
                let value = if cursor.peek() == Some(b'=') {
                    cursor.at += 1;
                    cursor.whitespace();
                    Some(cursor.value()?)
                } else {
                    None
                };
                if name.eq_ignore_ascii_case("rel") {
                    let value = value.ok_or_else(|| cursor.parameter_error())?;
                    if relations.is_none() {
                        relations = Some(
                            unquote(value)
                                .split_ascii_whitespace()
                                .map(str::to_ascii_lowercase)
                                .collect(),
                        );
                    }
                }
                parameters.push(Parameter {
                    name: name.to_owned(),
                    raw_value: value.map(str::to_owned),
                });
            }
            let target = base.join(raw_target).map_err(|_| LinkError::Url {
                at: offset + start + 1,
            })?;
            links.push(Link {
                target,
                raw_target: raw_target.to_owned(),
                relations: relations.unwrap_or_default(),
                parameters,
            });
        }
        offset += bytes.len() + 1;
    }
    Ok(links)
}

fn validate_target(target: &str, offset: usize) -> Result<(), LinkError> {
    let bytes = target.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        let valid = if b == b'%' {
            bytes.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
                && bytes.get(i + 2).is_some_and(u8::is_ascii_hexdigit)
        } else {
            b.is_ascii_alphanumeric() || b"-._~:/?#[]@!$&'()*+,;=".contains(&b)
        };
        if !valid {
            return Err(LinkError::Url { at: offset + i });
        }
    }
    Ok(())
}

struct Cursor<'a> {
    text: &'a str,
    at: usize,
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn skip(&mut self, mut pred: impl FnMut(u8) -> bool) {
        while self.peek().is_some_and(&mut pred) {
            self.at += 1;
        }
    }

    fn whitespace(&mut self) {
        self.skip(|b| matches!(b, b' ' | b'\t'));
    }

    fn token(&mut self) -> &'a str {
        let start = self.at;
        self.skip(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b));
        &self.text[start..self.at]
    }

    fn parameter_error(&self) -> LinkError {
        LinkError::Parameter {
            at: self.offset + self.at,
        }
    }

    fn value(&mut self) -> Result<&'a str, LinkError> {
        let start = self.at;
        if self.peek() != Some(b'"') {
            let value = self.token();
            return if value.is_empty() {
                Err(self.parameter_error())
            } else {
                Ok(value)
            };
        }
        self.at += 1;
        loop {
            match self.peek() {
                None => {
                    return Err(LinkError::Unterminated {
                        at: self.offset + start,
                    });
                }
                Some(b'"') => {
                    self.at += 1;
                    return Ok(&self.text[start..self.at]);
                }
                Some(b'\\') => {
                    self.at += 1;
                    if self.peek().is_none() {
                        return Err(LinkError::Unterminated {
                            at: self.offset + start,
                        });
                    }
                    self.at += 1;
                }
                Some(_) => self.at += 1,
            }
        }
    }
}

fn unquote(raw: &str) -> String {
    if !raw.starts_with('"') {
        return raw.to_owned();
    }
    let mut chars = raw[1..raw.len() - 1].chars();
    let mut value = String::new();
    while let Some(c) = chars.next() {
        value.push(if c == '\\' {
            chars.next().expect("checked escape")
        } else {
            c
        });
    }
    value
}

/// The four pagination relations (`next`, `prev`, `first`, `last`) of one response, as
/// [`parse_page_links`] projects them. `Debug` says which are present, never their targets.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct PageLinks {
    next: Option<Url>,
    prev: Option<Url>,
    first: Option<Url>,
    last: Option<Url>,
}

impl PageLinks {
    /// `rel="next"`: the page to fetch after this one. `None` on the last page.
    pub fn next(&self) -> Option<&Url> {
        self.next.as_ref()
    }

    /// `rel="prev"`.
    pub fn prev(&self) -> Option<&Url> {
        self.prev.as_ref()
    }

    /// `rel="first"`.
    pub fn first(&self) -> Option<&Url> {
        self.first.as_ref()
    }

    /// `rel="last"`; a provider may omit it when it cannot be calculated.
    pub fn last(&self) -> Option<&Url> {
        self.last.as_ref()
    }

    /// Whether no pagination relation was present.
    pub fn is_empty(&self) -> bool {
        self.next.is_none() && self.prev.is_none() && self.first.is_none() && self.last.is_none()
    }
}

impl fmt::Debug for PageLinks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PageLinks")
            .field("next", &self.next.is_some())
            .field("prev", &self.prev.is_some())
            .field("first", &self.first.is_some())
            .field("last", &self.last.is_some())
            .finish()
    }
}

/// Why the pagination relations could not be read. `Display`/`Debug` never contain the field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PageLinkError {
    /// The field value is not visible ASCII text.
    NotText,
    /// A link-value does not start with `<` (byte offset within the combined value).
    ExpectedTarget {
        /// Byte offset.
        at: usize,
    },
    /// A `<...>` target or a quoted string is not terminated.
    Unterminated {
        /// Byte offset where it started.
        at: usize,
    },
    /// A parameter is not `name` or `name=value`.
    Parameter {
        /// Byte offset.
        at: usize,
    },
    /// The target does not resolve to a URL.
    Url,
    /// The same relation is given two different URLs.
    Conflict {
        /// The relation.
        rel: &'static str,
    },
}

impl fmt::Display for PageLinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Link header not understood: ")?;
        match self {
            Self::NotText => f.write_str("not visible ASCII text"),
            Self::ExpectedTarget { at } => write!(f, "expected `<` at byte {at}"),
            Self::Unterminated { at } => write!(f, "unterminated target or string at byte {at}"),
            Self::Parameter { at } => write!(f, "malformed parameter at byte {at}"),
            Self::Url => f.write_str("a target is not a valid URL reference"),
            Self::Conflict { rel } => write!(f, "two different `{rel}` targets"),
        }
    }
}

impl std::error::Error for PageLinkError {}

impl From<LinkError> for PageLinkError {
    fn from(error: LinkError) -> Self {
        match error {
            LinkError::NotText { .. } => Self::NotText,
            LinkError::ExpectedTarget { at } => Self::ExpectedTarget { at },
            LinkError::Unterminated { at } => Self::Unterminated { at },
            LinkError::Parameter { at } => Self::Parameter { at },
            LinkError::Url { .. } => Self::Url,
        }
    }
}

/// Parses every `Link` field value of one response (in order) and projects the four pagination
/// relations, resolving relative targets against `response_url`. Two **different** targets for
/// one relation are a conflict; identical duplicates are one.
///
/// # Errors
///
/// A [`PageLinkError`] for malformed syntax or a conflict.
pub fn parse_page_links<V: AsRef<[u8]>>(
    values: impl IntoIterator<Item = V>,
    response_url: &Url,
) -> Result<PageLinks, PageLinkError> {
    let values: Vec<V> = values.into_iter().collect();
    project(parse(values.iter().map(AsRef::as_ref), response_url)?)
}

/// [`parse_page_links`] of one (combined) field value.
///
/// # Errors
///
/// A [`PageLinkError`].
pub fn parse_page_links_str(text: &str, response_url: &Url) -> Result<PageLinks, PageLinkError> {
    project(parse([text.as_bytes()], response_url)?)
}

fn project(parsed: Vec<Link>) -> Result<PageLinks, PageLinkError> {
    let mut links = PageLinks::default();
    for link in parsed {
        for (rel, slot) in [
            ("next", &mut links.next),
            ("prev", &mut links.prev),
            ("first", &mut links.first),
            ("last", &mut links.last),
        ] {
            if link.has_relation(rel) {
                match slot {
                    Some(existing) if *existing != link.target => {
                        return Err(PageLinkError::Conflict { rel });
                    }
                    Some(_) => {}
                    None => *slot = Some(link.target.clone()),
                }
            }
        }
    }
    Ok(links)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.test/items?page=1").unwrap()
    }

    fn read(text: &str) -> Result<Vec<Link>, LinkError> {
        parse([text.as_bytes()], &base())
    }

    #[test]
    fn multiple_fields_preserve_order_and_resolve_targets() {
        let links = parse(
            [
                b"<?page=2>; rel=next, </items?page=9>; rel=last".as_slice(),
                b"<https://other.test/help>; rel=help".as_slice(),
            ],
            &base(),
        )
        .unwrap();
        assert_eq!(links.len(), 3);
        assert_eq!(
            links[0].target.as_str(),
            "https://example.test/items?page=2"
        );
        assert_eq!(
            links[1].target.as_str(),
            "https://example.test/items?page=9"
        );
        assert_eq!(links[2].target.as_str(), "https://other.test/help");
        assert_eq!(links[0].raw_target, "?page=2");
    }

    #[test]
    fn quoted_commas_escaped_quotes_and_backslashes_are_not_delimiters() {
        let links =
            read(r#"</items?labels=a,b>; title="a, \"b\"; c\\d"; REL="N\EXT last""#).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].parameters[0].raw_value.as_deref(),
            Some(r#""a, \"b\"; c\\d""#)
        );
        assert_eq!(links[0].target.query(), Some("labels=a,b"));
        assert_eq!(links[0].relations, ["next", "last"]);
        assert!(links[0].has_relation("NeXt"));
        assert!(matches!(relation(&links, "LAST"), Relation::One(_)));
    }

    #[test]
    fn unknown_parameters_remain_raw_including_bare_and_empty_values() {
        let links = read(r##"</x>; UNKNOWN; x=""; Title="A\"B"; foo%'=bar; anchor="#z""##).unwrap();
        let parameters = &links[0].parameters;
        assert_eq!(parameters.len(), 5);
        assert_eq!(parameters[0].name, "UNKNOWN");
        assert_eq!(parameters[0].raw_value, None);
        assert_eq!(parameters[1].raw_value.as_deref(), Some("\"\""));
        assert_eq!(parameters[2].raw_value.as_deref(), Some(r#""A\"B""#));
        assert_eq!(parameters[3].name, "foo%'");
        assert_eq!(parameters[4].raw_value.as_deref(), Some("\"#z\""));
        assert_eq!(relation(&links, "next"), Relation::Absent);
    }

    #[test]
    fn first_rel_wins_even_when_empty() {
        let links = read(r#"</a>; rel=prev; REL=next, </b>; rel=""; rel=next"#).unwrap();
        assert_eq!(links[0].relations, ["prev"]);
        assert!(links[1].relations.is_empty());
        assert_eq!(links[0].parameters.len(), 2);
        assert_eq!(relation(&links, "next"), Relation::Absent);
        assert!(read(r#"</a>; rel=next; rel="unterminated"#).is_err());
    }

    #[test]
    fn empty_fields_and_list_elements_are_ignored() {
        assert!(parse(std::iter::empty(), &base()).unwrap().is_empty());
        assert!(
            parse([b"".as_slice(), b" \t, ,\t".as_slice()], &base())
                .unwrap()
                .is_empty()
        );
        let links = read(" , , </x>; rel=next, , ").unwrap();
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn empty_target_is_a_relative_reference() {
        let links = read("<>; rel=next").unwrap();
        assert_eq!(links[0].target, base());
        assert_eq!(links[0].raw_target, "");
    }

    #[test]
    fn missing_or_unterminated_targets_and_quotes_are_errors() {
        assert_eq!(
            read("https://x/; rel=next"),
            Err(LinkError::ExpectedTarget { at: 0 })
        );
        assert_eq!(
            read("<https://x/; rel=next"),
            Err(LinkError::Unterminated { at: 0 })
        );
        for text in ["<https://x/>; rel=\"next", "<https://x/>; rel=\"next\\"] {
            assert_eq!(read(text), Err(LinkError::Unterminated { at: 18 }));
        }
    }

    #[test]
    fn malformed_parameters_and_empty_unquoted_values_are_errors() {
        for text in [
            "</x> rel=next",
            "</x>; =next",
            "</x>; rel=",
            "</x>; rel",
            "</x>;",
            "</x>;, </y>",
            "</x>; title=",
            "</x>; title=/unquoted",
            "</x>; rel=next garbage",
            "</x>; rel=\"next\"junk",
            "</x>; rel=next; rel",
        ] {
            assert!(
                matches!(read(text), Err(LinkError::Parameter { .. })),
                "{text}"
            );
        }
    }

    #[test]
    fn non_utf8_unicode_and_controls_are_rejected() {
        for text in [
            b"</x>; title=\"\xff\"".as_slice(),
            "</x>; title=\"é\"".as_bytes(),
            b"</x>; title=\"\n\"".as_slice(),
            b"</x>; title=\"\x7f\"".as_slice(),
        ] {
            assert_eq!(parse([text], &base()), Err(LinkError::NotText { at: 13 }));
        }
        assert!(read("\t</x>\t;\trel\t=\tnext").is_ok());
    }

    #[test]
    fn repeated_next_is_several_even_with_identical_targets() {
        for text in [
            "</a>; rel=next, </b>; rel=next",
            "</a>; rel=next, </a>; rel=next",
        ] {
            assert_eq!(relation(&read(text).unwrap(), "next"), Relation::Several);
        }
        let links = read(r#"</a>; rel="next NEXT""#).unwrap();
        assert_eq!(relation(&links, "next"), Relation::One(&links[0]));
        assert_eq!(relation(&links, "prev"), Relation::Absent);
    }

    #[test]
    fn invalid_targets_are_errors_even_without_a_known_relation() {
        for text in [
            "<http://[::1/>; rel=help",
            "< /x>; rel=next",
            "</x y>; rel=next",
            "</x\ty>; rel=next",
            "</x\\y>; rel=next",
            "</x%zz>; rel=next",
            "</x%2>; rel=next",
            "<</x>; rel=next",
        ] {
            assert!(matches!(read(text), Err(LinkError::Url { .. })), "{text}");
        }
        assert!(read("</x%20y?opaque=%2f+%2B>; rel=next").is_ok());
    }

    #[test]
    fn offsets_include_all_field_lines_and_fields_cannot_complete_each_other() {
        let first = b"</a>; rel=prev".as_slice();
        assert_eq!(
            parse([first, b"", b"bad"], &base()),
            Err(LinkError::ExpectedTarget {
                at: first.len() + 2
            })
        );
        assert_eq!(
            parse([b"</a".as_slice(), b">; rel=next"], &base()),
            Err(LinkError::Unterminated { at: 0 })
        );
        assert_eq!(
            parse([b"</a>; title=\"x".as_slice(), b"y\"; rel=next"], &base()),
            Err(LinkError::Unterminated { at: 12 })
        );
    }

    #[test]
    fn diagnostics_withhold_all_header_contents() {
        let links =
            read(r#"</SECRET_TARGET>; rel=SECRET_REL; SECRET_NAME="SECRET_VALUE""#).unwrap();
        for shown in [
            format!("{links:?}"),
            format!("{:?}", links[0].parameters),
            format!("{:?}", relation(&links, "SECRET_REL")),
        ] {
            assert!(!shown.contains("SECRET"));
        }
        let error =
            read("</SECRET_TARGET>; rel=SECRET_REL; SECRET_NAME=\"SECRET_VALUE").unwrap_err();
        assert!(!format!("{error} {error:?}").contains("SECRET"));
    }
}
