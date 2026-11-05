//! SOQL construction, kept apart from user-data encoding.
//!
//! A [`Soql`] statement is built from **typed identifiers** ([`SObjectName`], [`FieldPath`]) and
//! **literals** ([`Literal`]); user data only ever enters through a literal, which escapes it
//! for SOQL (`'O\'Brien'`). Identifiers are validated API names and are never quoted or escaped:
//! a value that is not an identifier cannot become one. The finished statement is then plain
//! text, which the transport encodes for its own placement: Concord percent-encodes it as the
//! `q` query value of `GET query` (`O%5C%27Brien`), and `serde_json` escapes it in the body of a
//! Bulk query job. The two encodings never mix.
//!
//! SOQL escaping (SOQL and SOSL Reference, "Quoted String Escape Sequences"): `\'`, `\"`, `\\`,
//! `\n`, `\r`, `\t`, `\b`, `\f`; in a `LIKE` pattern `\_` and `\%` escape the wildcards, which
//! [`Literal::like_contains`] and friends apply to user text.

use std::fmt;

use crate::ids::{FieldName, InvalidValue, RecordId, SObjectName};

/// A field path in a `SELECT` list or condition: a field (`Name`) or a child-to-parent
/// relationship path (`Account.Name`), each part an API name.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct FieldPath(Vec<FieldName>);

impl FieldPath {
    /// One field.
    pub fn field(name: FieldName) -> Self {
        Self(vec![name])
    }

    /// Parses `Name` or `Account.Owner.Email`.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] when a part is not an API name.
    pub fn parse(text: &str) -> Result<Self, InvalidValue> {
        let parts = text
            .split('.')
            .map(FieldName::new)
            .collect::<Result<Vec<_>, _>>()?;
        if parts.len() > 5 {
            return Err(InvalidValue::new(
                "field path",
                "too many relationship levels",
            ));
        }
        Ok(Self(parts))
    }

    /// The parts.
    pub fn parts(&self) -> &[FieldName] {
        &self.0
    }
}

impl From<FieldName> for FieldPath {
    fn from(name: FieldName) -> Self {
        Self::field(name)
    }
}

impl fmt::Display for FieldPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, part) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(".")?;
            }
            f.write_str(part.as_str())?;
        }
        Ok(())
    }
}

impl fmt::Debug for FieldPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FieldPath({self})")
    }
}

/// A SOQL literal: user data, escaped for SOQL when written.
#[derive(Clone, PartialEq)]
pub enum Literal {
    /// A quoted string literal.
    Text(String),
    /// A `LIKE` pattern: user text (its `%`/`_` escaped) around the SDK's own wildcards.
    Pattern(Vec<PatternPart>),
    /// An id, written quoted.
    Id(RecordId),
    /// An integer.
    Integer(i64),
    /// `true` / `false`.
    Bool(bool),
    /// `null`.
    Null,
    /// A date literal `YYYY-MM-DD` (validated shape).
    Date(String),
    /// A dateTime literal `YYYY-MM-DDThh:mm:ssZ` (validated shape).
    DateTime(String),
}

impl Literal {
    /// A quoted text literal.
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    /// A `LIKE` pattern matching values that contain `value` (`'%' + escaped + '%'`).
    pub fn like_contains(value: &str) -> Self {
        Self::Pattern(vec![
            PatternPart::AnyRun,
            PatternPart::Text(value.to_owned()),
            PatternPart::AnyRun,
        ])
    }

    /// A `LIKE` pattern matching values that end with `value`.
    pub fn like_suffix(value: &str) -> Self {
        Self::Pattern(vec![
            PatternPart::AnyRun,
            PatternPart::Text(value.to_owned()),
        ])
    }

    /// A date literal.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] unless `value` has the shape `YYYY-MM-DD`.
    pub fn date(value: &str) -> Result<Self, InvalidValue> {
        if shape(value, "dddd-dd-dd") {
            Ok(Self::Date(value.to_owned()))
        } else {
            Err(InvalidValue::new("date literal", "expected YYYY-MM-DD"))
        }
    }

    /// A UTC dateTime literal.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] unless `value` has the shape `YYYY-MM-DDThh:mm:ssZ`.
    pub fn date_time(value: &str) -> Result<Self, InvalidValue> {
        if shape(value, "dddd-dd-ddTdd:dd:ddZ") {
            Ok(Self::DateTime(value.to_owned()))
        } else {
            Err(InvalidValue::new(
                "dateTime literal",
                "expected YYYY-MM-DDThh:mm:ssZ",
            ))
        }
    }
}

fn shape(value: &str, pattern: &str) -> bool {
    value.len() == pattern.len()
        && value.bytes().zip(pattern.bytes()).all(|(v, p)| match p {
            b'd' => v.is_ascii_digit(),
            other => v == other,
        })
}

/// Escapes text for a quoted SOQL string.
fn escape_text(value: &str, out: &mut String) {
    for c in value.chars() {
        match c {
            '\'' => out.push_str("\\'"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c => out.push(c),
        }
    }
}

/// One part of a `LIKE` pattern.
#[derive(Clone, PartialEq, Eq)]
pub enum PatternPart {
    /// `%`: any run of characters (the SDK's wildcard).
    AnyRun,
    /// User text, matched literally: its `%` and `_` are written `\%` and `\_`.
    Text(String),
}

impl Literal {
    fn write(&self, out: &mut String) {
        match self {
            Literal::Text(text) => {
                out.push('\'');
                escape_text(text, out);
                out.push('\'');
            }
            Literal::Pattern(parts) => {
                out.push('\'');
                for part in parts {
                    match part {
                        PatternPart::AnyRun => out.push('%'),
                        PatternPart::Text(text) => {
                            for c in text.chars() {
                                match c {
                                    '%' => out.push_str("\\%"),
                                    '_' => out.push_str("\\_"),
                                    c => escape_text(c.encode_utf8(&mut [0; 4]), out),
                                }
                            }
                        }
                    }
                }
                out.push('\'');
            }
            Literal::Id(id) => {
                out.push('\'');
                out.push_str(id.as_str());
                out.push('\'');
            }
            Literal::Integer(n) => out.push_str(&n.to_string()),
            Literal::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Literal::Null => out.push_str("null"),
            Literal::Date(d) | Literal::DateTime(d) => out.push_str(d),
        }
    }
}

impl fmt::Debug for Literal {
    /// The kind only: a literal is user data.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Literal::Text(_) => "Literal::Text(..)",
            Literal::Pattern(_) => "Literal::Pattern(..)",
            Literal::Id(_) => "Literal::Id(..)",
            Literal::Integer(_) => "Literal::Integer(..)",
            Literal::Bool(_) => "Literal::Bool(..)",
            Literal::Null => "Literal::Null",
            Literal::Date(_) => "Literal::Date(..)",
            Literal::DateTime(_) => "Literal::DateTime(..)",
        })
    }
}

/// A comparison operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `=`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `LIKE` (use a [`Literal::Pattern`]).
    Like,
}

impl Op {
    fn text(self) -> &'static str {
        match self {
            Op::Eq => "=",
            Op::Ne => "!=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::Like => "LIKE",
        }
    }
}

/// One condition, `field op literal`; conditions of a statement are joined with `AND`.
#[derive(Clone, Debug)]
pub struct Condition {
    field: FieldPath,
    op: Op,
    value: Literal,
}

impl Condition {
    /// `field op value`.
    pub fn new(field: impl Into<FieldPath>, op: Op, value: Literal) -> Self {
        Self {
            field: field.into(),
            op,
            value,
        }
    }
}

/// A sort direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// `ASC`
    Asc,
    /// `DESC`
    Desc,
}

/// A finished SOQL statement: plain text for the transport to encode. `Display` is the
/// statement; `Debug` shows only its shape (the object and field count), never literals.
#[derive(Clone, PartialEq)]
pub struct Soql {
    text: String,
    object: SObjectName,
    fields: Vec<FieldPath>,
}

impl Soql {
    /// Starts `SELECT fields FROM object`.
    pub fn select(object: SObjectName, fields: impl IntoIterator<Item = FieldPath>) -> SoqlBuilder {
        SoqlBuilder {
            object,
            fields: fields.into_iter().collect(),
            conditions: Vec::new(),
            order: Vec::new(),
            limit: None,
        }
    }

    /// The statement text, **unredacted** (it contains the literals).
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The queried object.
    pub fn object(&self) -> &SObjectName {
        &self.object
    }

    /// The selected fields, in order: Bulk API 2.0 (API 50.0 and later) returns CSV columns in
    /// this order, which the CSV adapter checks.
    pub fn fields(&self) -> &[FieldPath] {
        &self.fields
    }
}

impl fmt::Display for Soql {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl fmt::Debug for Soql {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Soql")
            .field("object", &self.object)
            .field("fields", &self.fields.len())
            .finish_non_exhaustive()
    }
}

/// Builds a [`Soql`] statement.
#[derive(Clone, Debug)]
pub struct SoqlBuilder {
    object: SObjectName,
    fields: Vec<FieldPath>,
    conditions: Vec<Condition>,
    order: Vec<(FieldPath, Direction)>,
    limit: Option<u32>,
}

impl SoqlBuilder {
    /// Adds a condition (`AND`).
    #[must_use]
    pub fn filter(mut self, condition: Condition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Adds a sort key.
    #[must_use]
    pub fn order_by(mut self, field: impl Into<FieldPath>, direction: Direction) -> Self {
        self.order.push((field.into(), direction));
        self
    }

    /// `LIMIT n`.
    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// The statement.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] without fields or with a repeated field.
    pub fn build(self) -> Result<Soql, InvalidValue> {
        if self.fields.is_empty() {
            return Err(InvalidValue::new("SOQL statement", "no fields"));
        }
        for (i, f) in self.fields.iter().enumerate() {
            let text = f.to_string();
            if self.fields[..i]
                .iter()
                .any(|g| g.to_string().eq_ignore_ascii_case(&text))
            {
                return Err(InvalidValue::new("SOQL statement", "repeated field"));
            }
        }
        let mut text = String::from("SELECT ");
        for (i, field) in self.fields.iter().enumerate() {
            if i > 0 {
                text.push_str(", ");
            }
            text.push_str(&field.to_string());
        }
        text.push_str(" FROM ");
        text.push_str(self.object.as_str());
        for (i, condition) in self.conditions.iter().enumerate() {
            text.push_str(if i == 0 { " WHERE " } else { " AND " });
            text.push_str(&condition.field.to_string());
            text.push(' ');
            text.push_str(condition.op.text());
            text.push(' ');
            condition.value.write(&mut text);
        }
        for (i, (field, direction)) in self.order.iter().enumerate() {
            text.push_str(if i == 0 { " ORDER BY " } else { ", " });
            text.push_str(&field.to_string());
            text.push_str(match direction {
                Direction::Asc => " ASC",
                Direction::Desc => " DESC",
            });
        }
        if let Some(limit) = self.limit {
            text.push_str(&format!(" LIMIT {limit}"));
        }
        Ok(Soql {
            text,
            object: self.object,
            fields: self.fields,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str) -> FieldPath {
        FieldPath::parse(name).unwrap()
    }

    #[test]
    fn literals_are_escaped_and_identifiers_are_validated() {
        let soql = Soql::select(SObjectName::new("Account").unwrap(), [f("Id"), f("Name")])
            .filter(Condition::new(
                f("Name"),
                Op::Eq,
                Literal::text("O'Brien \\ \"Sons\"\n"),
            ))
            .filter(Condition::new(
                f("Name"),
                Op::Like,
                Literal::like_contains("50%_off'"),
            ))
            .order_by(f("Id"), Direction::Asc)
            .limit(10)
            .build()
            .unwrap();
        assert_eq!(
            soql.as_str(),
            "SELECT Id, Name FROM Account WHERE Name = 'O\\'Brien \\\\ \\\"Sons\\\"\\n' \
             AND Name LIKE '%50\\%\\_off\\'%' ORDER BY Id ASC LIMIT 10"
        );
        assert!(!format!("{soql:?}").contains("Brien"));
        assert!(FieldPath::parse("Name) OR (Id").is_err());
        assert_eq!(f("Account.Name").to_string(), "Account.Name");
    }

    #[test]
    fn date_literals_have_a_fixed_shape() {
        assert!(Literal::date("2026-11-04").is_ok());
        assert!(Literal::date("2026-11-04' OR '1'='1").is_err());
        assert!(Literal::date_time("2026-11-04T10:00:00Z").is_ok());
        assert!(Literal::date_time("2026-11-04").is_err());
    }
}
