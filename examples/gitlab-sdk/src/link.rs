//! The `Link` response header (RFC 8288), as GitLab sends it on paginated responses.
//!
//! GitLab's REST documentation shows `Link: <url>; rel="prev", <url>; rel="next", <url>;
//! rel="first", <url>; rel="last"` and says to use these links instead of generating URLs. This
//! module reads those four relations. The traversal follows `next` only; `last` is omitted by
//! GitLab for collections over 10,000 records and is never used to compute anything.
//!
//! Parsing is strict where a guess would end a traversal silently: a link-value that does not
//! start with `<`, an unterminated `<...>` or quoted string, a malformed parameter, a target that
//! does not resolve, or two different URLs for one relation are a [`LinkError`], not "no next
//! page". Unknown relations and parameters are ignored (RFC 8288 §3.3: only the first `rel`
//! counts). Relative targets resolve against the URL of the response that carried them; several
//! `Link` field lines are combined in order.

use std::fmt;

use concord::reqwest::Url;
use concord::reqwest::header::HeaderValue;

/// The pagination relations of one response. `Debug` says which are present, never their URLs
/// (they carry the listing's filters, a search term for example).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Links {
    next: Option<Url>,
    prev: Option<Url>,
    first: Option<Url>,
    last: Option<Url>,
}

impl Links {
    /// `rel="next"`: the page after this one; `None` on the last page.
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

    /// `rel="last"`; GitLab omits it for collections over 10,000 records.
    pub fn last(&self) -> Option<&Url> {
        self.last.as_ref()
    }
}

impl fmt::Debug for Links {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Links")
            .field("next", &self.next.is_some())
            .field("prev", &self.prev.is_some())
            .field("first", &self.first.is_some())
            .field("last", &self.last.is_some())
            .finish()
    }
}

/// Why a `Link` header could not be read. `Display` and `Debug` never contain the header text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LinkError {
    /// The field value is not visible ASCII.
    NotText,
    /// A link-value does not start with `<` (byte offset in the combined value).
    ExpectedTarget {
        /// Byte offset.
        at: usize,
    },
    /// A `<...>` target or a quoted string is not terminated.
    Unterminated {
        /// Byte offset where it started.
        at: usize,
    },
    /// A parameter is not `; name` or `; name=value`.
    Parameter {
        /// Byte offset.
        at: usize,
    },
    /// A target is not a valid URL reference.
    Url,
    /// One relation has two different targets.
    Conflict {
        /// The relation.
        rel: &'static str,
    },
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Link header not understood: ")?;
        match self {
            Self::NotText => f.write_str("not visible ASCII"),
            Self::ExpectedTarget { at } => write!(f, "expected `<` at byte {at}"),
            Self::Unterminated { at } => write!(f, "unterminated target or string at byte {at}"),
            Self::Parameter { at } => write!(f, "malformed parameter at byte {at}"),
            Self::Url => f.write_str("a target is not a valid URL reference"),
            Self::Conflict { rel } => write!(f, "two different `{rel}` targets"),
        }
    }
}

impl std::error::Error for LinkError {}

/// Reads every `Link` field line of one response, resolving relative targets against `base` (the
/// response's URL).
///
/// # Errors
///
/// A [`LinkError`]; see the [module documentation](self).
pub fn parse<'a>(
    values: impl IntoIterator<Item = &'a HeaderValue>,
    base: &Url,
) -> Result<Links, LinkError> {
    let mut combined = String::new();
    for value in values {
        let text = value.to_str().map_err(|_| LinkError::NotText)?;
        if !combined.is_empty() {
            combined.push(',');
        }
        combined.push_str(text);
    }
    parse_str(&combined, base)
}

/// Reads one (combined) `Link` field value. See [`parse`].
///
/// # Errors
///
/// A [`LinkError`].
pub fn parse_str(text: &str, base: &Url) -> Result<Links, LinkError> {
    let mut cursor = Cursor {
        text,
        bytes: text.as_bytes(),
        at: 0,
    };
    let mut links = Links::default();
    loop {
        cursor.skip(|b| matches!(b, b' ' | b'\t' | b','));
        if cursor.done() {
            return Ok(links);
        }
        let start = cursor.at;
        if cursor.peek() != Some(b'<') {
            return Err(LinkError::ExpectedTarget { at: start });
        }
        let end = text[start + 1..]
            .find('>')
            .map(|offset| start + 1 + offset)
            .ok_or(LinkError::Unterminated { at: start })?;
        let target = &text[start + 1..end];
        cursor.at = end + 1;
        let rels = cursor.parameters()?;
        let mut url = None;
        for rel in rels.iter().filter_map(|rel| relation(rel)) {
            let url = match &url {
                Some(url) => url,
                None => url.insert(base.join(target).map_err(|_| LinkError::Url)?),
            };
            let slot = match rel {
                "next" => &mut links.next,
                "prev" => &mut links.prev,
                "first" => &mut links.first,
                _ => &mut links.last,
            };
            match slot {
                Some(existing) if existing != url => return Err(LinkError::Conflict { rel }),
                Some(_) => {}
                None => *slot = Some(url.clone()),
            }
        }
    }
}

/// The relation names this module keeps.
fn relation(rel: &str) -> Option<&'static str> {
    ["next", "prev", "first", "last"]
        .into_iter()
        .find(|known| rel.eq_ignore_ascii_case(known))
}

struct Cursor<'a> {
    text: &'a str,
    bytes: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip(&mut self, mut pred: impl FnMut(u8) -> bool) {
        while self.peek().is_some_and(&mut pred) {
            self.at += 1;
        }
    }

    fn whitespace(&mut self) {
        self.skip(|b| matches!(b, b' ' | b'\t'));
    }

    fn token(&mut self) -> &str {
        let start = self.at;
        self.skip(is_tchar);
        &self.text[start..self.at]
    }

    /// The parameters of one link-value, up to the next top-level `,`; returns the relation
    /// types of its first `rel` parameter.
    fn parameters(&mut self) -> Result<Vec<String>, LinkError> {
        let mut rels: Option<Vec<String>> = None;
        loop {
            self.whitespace();
            match self.peek() {
                None | Some(b',') => return Ok(rels.unwrap_or_default()),
                Some(b';') => self.at += 1,
                Some(_) => return Err(LinkError::Parameter { at: self.at }),
            }
            self.whitespace();
            let at = self.at;
            let name = self.token().to_ascii_lowercase();
            if name.is_empty() {
                // A trailing `;` before the end or the next link-value is tolerated.
                if matches!(self.peek(), None | Some(b',')) {
                    continue;
                }
                return Err(LinkError::Parameter { at });
            }
            self.whitespace();
            let value = if self.peek() == Some(b'=') {
                self.at += 1;
                self.whitespace();
                Some(self.value()?)
            } else {
                None
            };
            if name == "rel"
                && rels.is_none()
                && let Some(value) = value
            {
                rels = Some(value.split_ascii_whitespace().map(str::to_owned).collect());
            }
        }
    }

    /// A token or a quoted string (with `\` escapes).
    fn value(&mut self) -> Result<String, LinkError> {
        if self.peek() != Some(b'"') {
            let at = self.at;
            let token = self.token();
            if token.is_empty() {
                return Err(LinkError::Parameter { at });
            }
            return Ok(token.to_owned());
        }
        let quote = self.at;
        self.at += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(LinkError::Unterminated { at: quote }),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    let escaped = self
                        .bytes
                        .get(self.at + 1)
                        .ok_or(LinkError::Unterminated { at: quote })?;
                    out.push(char::from(*escaped));
                    self.at += 2;
                }
                Some(byte) => {
                    out.push(char::from(byte));
                    self.at += 1;
                }
            }
        }
    }
}

/// RFC 9110 `tchar`.
fn is_tchar(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}
