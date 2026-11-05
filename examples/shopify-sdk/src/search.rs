//! Shopify's search syntax for the `query:` argument of connections (`products`,
//! `collections`, `locations`, ...), as a small typed builder.
//!
//! Terms are `field:value` (no space after the colon), joined by implicit `AND`. A value that is
//! not a plain word is double-quoted with `\` escaping `"` and `\`; range terms use `:>`, `:>=`,
//! `:<`, `:<=` with a quoted value (dates must be quoted). The search string itself is then just
//! a GraphQL `String` variable: its JSON encoding is serde_json's, independent of this syntax.
//! Source: <https://shopify.dev/docs/api/usage/search-syntax> (retrieved 4 November 2026); the
//! page lists `:`, `\`, `(` and `)` as characters to escape in bare values and does not say how a
//! quote inside a quoted value is written: this builder escapes it with `\` (see `PROVIDER.md`).

use std::fmt;

/// A search query string, built from typed terms or taken verbatim.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct SearchQuery(String);

/// A range comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compare {
    /// `:>`
    Greater,
    /// `:>=`
    GreaterOrEqual,
    /// `:<`
    Less,
    /// `:<=`
    LessOrEqual,
}

impl SearchQuery {
    /// An empty query (matches everything).
    pub fn new() -> Self {
        Self::default()
    }

    /// A query written by the caller in Shopify's syntax, sent as given.
    pub fn raw(query: impl Into<String>) -> Self {
        Self(query.into())
    }

    /// Adds `field:value` (the value quoted and escaped when it is not a plain word).
    #[must_use]
    pub fn term(mut self, field: &str, value: &str) -> Self {
        self.push(&format!("{field}:{}", value_text(value)));
        self
    }

    /// Adds `-field:value`.
    #[must_use]
    pub fn not_term(mut self, field: &str, value: &str) -> Self {
        self.push(&format!("-{field}:{}", value_text(value)));
        self
    }

    /// Adds a range term, `field:>'value'` style (the value always quoted).
    #[must_use]
    pub fn compare(mut self, field: &str, compare: Compare, value: &str) -> Self {
        let op = match compare {
            Compare::Greater => ">",
            Compare::GreaterOrEqual => ">=",
            Compare::Less => "<",
            Compare::LessOrEqual => "<=",
        };
        self.push(&format!("{field}:{op}{}", quoted(value)));
        self
    }

    /// The query text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether no term was added.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn push(&mut self, term: &str) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        self.0.push_str(term);
    }
}

impl fmt::Debug for SearchQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Search terms can be business data (a vendor, a title): show the size only.
        write!(f, "SearchQuery({} bytes)", self.0.len())
    }
}

fn value_text(value: &str) -> String {
    let plain = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '*'))
        && !value.starts_with('-');
    if plain {
        value.to_owned()
    } else {
        quoted(value)
    }
}

fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        if matches!(c, '"' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}
