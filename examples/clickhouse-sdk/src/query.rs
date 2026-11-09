//! Constrained SQL: query text with typed placeholders, the values bound to them, identifiers,
//! table names and query IDs.
//!
//! The SDK never splices a value into SQL text. A [`Query`] is the caller's SQL with ClickHouse
//! placeholders (`{name:Type}`); each value is bound by name and travels as a separate
//! `param_<name>` URL parameter in ClickHouse's "escaped" text format, which the server parses
//! with the placeholder's type. Identifiers (table, database and column names) are a restricted
//! [`Identifier`] type, bound as `{name:Identifier}` or written by the SDK into the `INSERT`
//! statements it builds.

use std::collections::BTreeMap;
use std::fmt;

/// Why a query, a value or a name was refused before anything was sent.
///
/// `Display` names the rule and the placeholder or parameter name (an SDK author's identifier,
/// never a bound value).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RequestError {
    /// The SQL text is empty or only whitespace.
    EmptyQuery,
    /// A placeholder in the SQL text has no bound value.
    Unbound {
        /// The placeholder name.
        name: String,
    },
    /// A value was bound to a name that is not a placeholder of the SQL text.
    UnknownParameter {
        /// The bound name.
        name: String,
    },
    /// The same name was bound twice.
    DuplicateParameter {
        /// The bound name.
        name: String,
    },
    /// An identifier is not `[A-Za-z_][A-Za-z0-9_]*` (at most 255 bytes).
    InvalidIdentifier,
    /// A query ID or quota key is empty or contains an ASCII control character.
    InvalidToken {
        /// Which value (`query_id`, `quota_key`).
        what: &'static str,
    },
    /// A user name for HTTP Basic authentication contains `:` or a control character, or a
    /// credential cannot be sent as a header value.
    InvalidCredentials,
    /// The insert's columns are refused by the CSV writer (a repeated name), before sending.
    InvalidColumns {
        /// The writer's static label.
        label: &'static str,
    },
    /// A floating-point value is NaN or infinite.
    NonFiniteFloat {
        /// The parameter name.
        name: String,
    },
}

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyQuery => f.write_str("the SQL text is empty"),
            Self::Unbound { name } => write!(f, "the placeholder `{name}` has no bound value"),
            Self::UnknownParameter { name } => {
                write!(f, "`{name}` is not a placeholder of the SQL text")
            }
            Self::DuplicateParameter { name } => write!(f, "`{name}` is bound twice"),
            Self::InvalidIdentifier => {
                f.write_str("an identifier must match [A-Za-z_][A-Za-z0-9_]* (at most 255 bytes)")
            }
            Self::InvalidToken { what } => {
                write!(f, "the {what} is empty or contains a control character")
            }
            Self::InvalidCredentials => f.write_str(
                "the credentials cannot be sent (a `:` or control character in the user name, or a value that is not a header value)",
            ),
            Self::InvalidColumns { label } => write!(f, "the insert columns are refused: {label}"),
            Self::NonFiniteFloat { name } => {
                write!(f, "the value bound to `{name}` is NaN or infinite")
            }
        }
    }
}

impl std::error::Error for RequestError {}

/// A table, database or column name: `[A-Za-z_][A-Za-z0-9_]*`, 1 to 255 bytes.
///
/// Deliberately narrower than ClickHouse's identifier grammar: such a name needs no quoting, so
/// the SDK can write it into an `INSERT` statement (backquoted) and bind it to an
/// `{name:Identifier}` placeholder without an escaping rule of its own.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, concord::Debug)]
pub struct Identifier(String);

impl Identifier {
    /// Validates `name`.
    ///
    /// # Errors
    ///
    /// [`RequestError::InvalidIdentifier`].
    pub fn new(name: impl Into<String>) -> Result<Self, RequestError> {
        let name = name.into();
        if is_identifier(&name) && name.len() <= 255 {
            Ok(Self(name))
        } else {
            Err(RequestError::InvalidIdentifier)
        }
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn is_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_'))
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// A table, optionally qualified by its database: the target of an insert.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Table {
    database: Option<Identifier>,
    name: Identifier,
}

impl Table {
    /// A table of the request's default database (the client's or the call's `database`, else the
    /// server's default).
    pub fn new(name: Identifier) -> Self {
        Self {
            database: None,
            name,
        }
    }

    /// The same table qualified by `database`.
    #[must_use]
    pub fn in_database(mut self, database: Identifier) -> Self {
        self.database = Some(database);
        self
    }

    /// The database, if qualified.
    pub fn database(&self) -> Option<&Identifier> {
        self.database.as_ref()
    }

    /// The table name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// The backquoted SQL reference: `` `db`.`t` `` or `` `t` ``.
    pub(crate) fn sql(&self) -> String {
        match &self.database {
            Some(db) => format!("`{db}`.`{}`", self.name),
            None => format!("`{}`", self.name),
        }
    }
}

/// A query ID chosen by the caller (`query_id` URL parameter). ClickHouse documents it as "any
/// string"; the SDK refuses an empty one and ASCII control characters, so the server's echo in
/// `X-ClickHouse-Query-Id` can be compared with it.
///
/// Without one, the server assigns an ID and reports it in `X-ClickHouse-Query-Id` when it sends
/// that header.
#[derive(Clone, PartialEq, Eq, Hash, concord::Debug)]
pub struct QueryId(String);

impl QueryId {
    /// Validates `id`.
    ///
    /// # Errors
    ///
    /// [`RequestError::InvalidToken`].
    pub fn new(id: impl Into<String>) -> Result<Self, RequestError> {
        let id = id.into();
        if is_token(&id) {
            Ok(Self(id))
        } else {
            Err(RequestError::InvalidToken { what: "query_id" })
        }
    }

    /// The ID.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for QueryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub(crate) fn is_token(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}

/// One value bound to a placeholder, already in ClickHouse's "escaped" text format.
///
/// Built from Rust values with `From`: integers, `bool`, `f32`/`f64` (finite), `&str`/`String`
/// (escaped), [`Identifier`], and `Option<T>` of these (`None` is `\N`, NULL, for a `Nullable`
/// placeholder). The server parses the text with the placeholder's declared type: binding `"x"`
/// to `{n:UInt8}` is the server's `BAD_QUERY_PARAMETER` exception, not a local check.
#[derive(Clone, PartialEq, Eq, concord::Debug)]
#[sensitive]
pub struct Param {
    text: String,
    non_finite: bool,
}

impl Param {
    /// The escaped text sent as the `param_<name>` value.
    pub fn as_escaped(&self) -> &str {
        &self.text
    }

    fn raw(text: String) -> Self {
        Self {
            text,
            non_finite: false,
        }
    }

    /// NULL (`\N`).
    pub fn null() -> Self {
        Self::raw("\\N".to_owned())
    }
}

/// Escapes a string for ClickHouse's escaped (TabSeparated) text format: backslash, tab, line
/// feed and carriage return (the minimum set the format requires, plus CR), and NUL, backspace and
/// form feed as the format's own output does. Every other character, `'` included, is literal.
pub fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c => out.push(c),
        }
    }
    out
}

macro_rules! display_params {
    ($($ty:ty),*) => {
        $(impl From<$ty> for Param {
            fn from(value: $ty) -> Self {
                Self::raw(value.to_string())
            }
        })*
    };
}

display_params!(i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, bool);

impl From<f64> for Param {
    fn from(value: f64) -> Self {
        Self {
            text: value.to_string(),
            non_finite: !value.is_finite(),
        }
    }
}

impl From<f32> for Param {
    fn from(value: f32) -> Self {
        Self {
            text: value.to_string(),
            non_finite: !value.is_finite(),
        }
    }
}

impl From<&str> for Param {
    fn from(value: &str) -> Self {
        Self::raw(escape(value))
    }
}

impl From<String> for Param {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl From<&String> for Param {
    fn from(value: &String) -> Self {
        Self::from(value.as_str())
    }
}

impl From<Identifier> for Param {
    fn from(value: Identifier) -> Self {
        Self::raw(value.0)
    }
}

impl From<&Identifier> for Param {
    fn from(value: &Identifier) -> Self {
        Self::raw(value.0.clone())
    }
}

impl<T: Into<Param>> From<Option<T>> for Param {
    fn from(value: Option<T>) -> Self {
        value.map_or_else(Param::null, Into::into)
    }
}

/// A placeholder found in the SQL text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placeholder {
    /// The name (`id` in `{id:UInt8}`).
    pub name: String,
    /// The declared type, as written (`UInt8`, `Array(String)`).
    pub ty: String,
}

/// SQL text with `{name:Type}` placeholders and the values bound to them.
///
/// The text is a `&'static str`: a literal in the caller's code, never assembled from data. A
/// query whose shape is chosen at run time from trusted pieces uses
/// [`from_trusted`](Self::from_trusted), whose name says what the caller vouches for. Values are
/// bound by name ([`bind`](Self::bind)); the SDK checks before sending that every placeholder has
/// exactly one value and every value a placeholder. Placeholders inside string literals, quoted
/// identifiers and comments are not placeholders (ClickHouse does not substitute them).
///
/// ```
/// use concord_sdk_clickhouse::Query;
///
/// let query = Query::new(
///     "SELECT home_team, away_team FROM football WHERE season = {season:Int16} AND home_team = {team:String}",
/// )
/// .bind("season", 2021_i16)?
/// .bind("team", "Sutton United")?;
/// assert_eq!(query.placeholders().len(), 2);
/// # Ok::<(), concord_sdk_clickhouse::RequestError>(())
/// ```
#[derive(Clone)]
pub struct Query {
    sql: String,
    placeholders: Vec<Placeholder>,
    params: BTreeMap<String, Param>,
}

impl Query {
    /// A query from literal SQL text.
    pub fn new(sql: &'static str) -> Self {
        Self::from_trusted(sql.to_owned())
    }

    /// A query from SQL text the caller vouches for: built from trusted pieces only, never from
    /// a value that should have been a placeholder.
    pub fn from_trusted(sql: String) -> Self {
        let placeholders = scan_placeholders(&sql);
        Self {
            sql,
            placeholders,
            params: BTreeMap::new(),
        }
    }

    /// Binds `value` to the placeholder `name`.
    ///
    /// # Errors
    ///
    /// [`RequestError::UnknownParameter`] if the text has no such placeholder,
    /// [`RequestError::DuplicateParameter`] if it is already bound,
    /// [`RequestError::NonFiniteFloat`] for a NaN or infinite float.
    pub fn bind(mut self, name: &str, value: impl Into<Param>) -> Result<Self, RequestError> {
        let value = value.into();
        if !self.placeholders.iter().any(|p| p.name == name) {
            return Err(RequestError::UnknownParameter {
                name: name.to_owned(),
            });
        }
        if value.non_finite {
            return Err(RequestError::NonFiniteFloat {
                name: name.to_owned(),
            });
        }
        if self.params.insert(name.to_owned(), value).is_some() {
            return Err(RequestError::DuplicateParameter {
                name: name.to_owned(),
            });
        }
        Ok(self)
    }

    /// The SQL text, unchanged.
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// The placeholders in order of appearance (a name used twice appears twice).
    pub fn placeholders(&self) -> &[Placeholder] {
        &self.placeholders
    }

    /// Checks the query and returns its URL parameters: `(param_<name>, escaped value)`, sorted
    /// by name.
    pub(crate) fn url_params(&self) -> Result<Vec<(String, String)>, RequestError> {
        if self.sql.trim().is_empty() {
            return Err(RequestError::EmptyQuery);
        }
        if let Some(missing) = self
            .placeholders
            .iter()
            .find(|p| !self.params.contains_key(&p.name))
        {
            return Err(RequestError::Unbound {
                name: missing.name.clone(),
            });
        }
        Ok(self
            .params
            .iter()
            .map(|(name, value)| (format!("param_{name}"), value.text.clone()))
            .collect())
    }
}

impl fmt::Debug for Query {
    /// The SQL text and the bound names; never the bound values.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Query")
            .field("sql", &self.sql)
            .field("bound", &self.params.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Finds `{name:Type}` placeholders outside string literals, quoted identifiers and comments.
fn scan_placeholders(sql: &str) -> Vec<Placeholder> {
    let bytes = sql.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'\'' | b'"' | b'`') => i = skip_quoted(bytes, i, quote),
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                i = bytes[i..]
                    .iter()
                    .position(|&b| b == b'\n')
                    .map_or(bytes.len(), |n| i + n + 1);
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => i = skip_block_comment(bytes, i),
            b'{' => match placeholder_at(sql, i) {
                Some((placeholder, end)) => {
                    found.push(placeholder);
                    i = end;
                }
                None => i += 1,
            },
            _ => i += 1,
        }
    }
    found
}

/// The index after the closing quote (or the end): backslash escapes the next byte, a doubled
/// quote is a quote.
fn skip_quoted(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b if b == quote => {
                if bytes.get(i + 1) == Some(&quote) {
                    i += 2;
                } else {
                    return i + 1;
                }
            }
            _ => i += 1,
        }
    }
    bytes.len()
}

/// The index after a (possibly nested) `/* .. */` comment, or the end.
fn skip_block_comment(bytes: &[u8], start: usize) -> usize {
    let mut depth = 0usize;
    let mut i = start;
    while i + 1 < bytes.len() {
        if bytes[i] == b'/' && bytes[i + 1] == b'*' {
            depth += 1;
            i += 2;
        } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    bytes.len()
}

/// `{ name : Type }` at `start` (spaces allowed around the parts; the type may contain balanced
/// parentheses), and the index after `}`.
fn placeholder_at(sql: &str, start: usize) -> Option<(Placeholder, usize)> {
    let bytes = sql.as_bytes();
    let mut i = start + 1;
    let skip_spaces = |mut i: usize| {
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        i
    };
    i = skip_spaces(i);
    let name_start = i;
    while bytes
        .get(i)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        i += 1;
    }
    let name = &sql[name_start..i];
    if !is_identifier(name) {
        return None;
    }
    i = skip_spaces(i);
    if bytes.get(i) != Some(&b':') {
        return None;
    }
    i += 1;
    let ty_start = i;
    let mut depth = 0usize;
    while let Some(&b) = bytes.get(i) {
        match b {
            b'(' => depth += 1,
            b')' => depth = depth.checked_sub(1)?,
            b'}' if depth == 0 => break,
            b'{' | b'\'' | b'"' | b'`' | b'\n' => return None,
            _ => {}
        }
        i += 1;
    }
    if bytes.get(i) != Some(&b'}') {
        return None;
    }
    let ty = sql[ty_start..i].trim();
    if ty.is_empty() {
        return None;
    }
    Some((
        Placeholder {
            name: name.to_owned(),
            ty: ty.to_owned(),
        },
        i + 1,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(sql: &'static str) -> Vec<String> {
        Query::new(sql)
            .placeholders()
            .iter()
            .map(|p| p.name.clone())
            .collect()
    }

    #[test]
    fn placeholders_are_found_with_their_types() {
        let q =
            Query::new("SELECT {p1:UInt8} + {p2 : Array(UInt16)} , { t: Tuple(UInt8, String) }");
        let found: Vec<_> = q
            .placeholders()
            .iter()
            .map(|p| (p.name.as_str(), p.ty.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                ("p1", "UInt8"),
                ("p2", "Array(UInt16)"),
                ("t", "Tuple(UInt8, String)")
            ]
        );
    }

    #[test]
    fn literals_identifiers_and_comments_hold_no_placeholders() {
        assert_eq!(
            names(
                "SELECT '{a:String}', \"{b:String}\", `{c:String}` -- {d:String}\n /* {e:String} /* {f:String} */ */ {g:UInt8}"
            ),
            ["g"]
        );
        assert_eq!(names("SELECT 'it''s {a:String}', {b:UInt8}"), ["b"]);
        assert_eq!(names("SELECT 'esc \\' {a:String}', {b:UInt8}"), ["b"]);
        // A map literal is not a placeholder.
        assert_eq!(names("SELECT {'k':1}"), Vec::<String>::new());
    }

    #[test]
    fn escaping_follows_the_escaped_format() {
        assert_eq!(escape("abc\t123"), "abc\\t123");
        assert_eq!(escape("a\\b\nc\rd\0"), "a\\\\b\\nc\\rd\\0");
        assert_eq!(escape("it's"), "it's");
        assert_eq!(escape("\\N"), "\\\\N");
        assert_eq!(Param::from(None::<i32>).as_escaped(), "\\N");
    }

    #[test]
    fn binding_is_checked() {
        let q = Query::new("SELECT {a:UInt8}, {a:UInt8}, {b:String}");
        assert_eq!(
            q.clone().bind("c", 1_u8).unwrap_err(),
            RequestError::UnknownParameter { name: "c".into() }
        );
        let q = q.bind("a", 1_u8).unwrap();
        assert_eq!(
            q.clone().bind("a", 2_u8).unwrap_err(),
            RequestError::DuplicateParameter { name: "a".into() }
        );
        assert_eq!(
            q.url_params().unwrap_err(),
            RequestError::Unbound { name: "b".into() }
        );
        let q = q.bind("b", "x").unwrap();
        assert_eq!(
            q.url_params().unwrap(),
            [
                ("param_a".to_owned(), "1".to_owned()),
                ("param_b".to_owned(), "x".to_owned())
            ]
        );
        assert_eq!(
            Query::new("SELECT {f:Float64}")
                .bind("f", f64::NAN)
                .unwrap_err(),
            RequestError::NonFiniteFloat { name: "f".into() }
        );
    }

    #[test]
    fn identifiers_are_restricted() {
        assert!(Identifier::new("football_2021").is_ok());
        assert!(Identifier::new("_t").is_ok());
        for bad in ["", "1t", "a-b", "a b", "a`b", "a.b", "tablé"] {
            assert_eq!(Identifier::new(bad), Err(RequestError::InvalidIdentifier));
        }
        assert!(Identifier::new("a".repeat(256)).is_err());
        let table =
            Table::new(Identifier::new("t").unwrap()).in_database(Identifier::new("db").unwrap());
        assert_eq!(table.sql(), "`db`.`t`");
    }

    #[test]
    fn debug_withholds_bound_values() {
        let q = Query::new("SELECT {secret:String}")
            .bind("secret", "s3cr3t-value")
            .unwrap();
        let shown = format!("{q:?}");
        assert!(shown.contains("secret") && !shown.contains("s3cr3t-value"));
    }
}
