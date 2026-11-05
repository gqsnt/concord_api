//! The writer's record serializer: one pass over a row's `Serialize` that checks its shape with
//! the same rules as `csv`'s header writer, captures (or compares) its field names, counts its
//! fields against the field limit and, while doing so, writes each value's field with `csv`'s
//! value text ([`Fields`]).
//!
//! Rules (those of `csv::Writer::serialize` with headers): every named struct field holds one
//! scalar; every scalar is either a named field value or the table is positional; structs may sit
//! inside tuples and sequences (their names are concatenated). Maps, flattened structs and
//! tuple/struct enum variants are refused, as `csv` refuses them.
//!
//! The names that are checked are those of the serialization whose values are written: the row's
//! `Serialize` runs once per record, so a stateful or conditional implementation cannot be
//! validated as one schema and written as another.

use std::fmt;

use serde::ser::{self, Serialize};

use super::fields::{Exceeded, FieldError, Fields};

/// The shape of one serialized row.
#[derive(Debug)]
pub(crate) struct Shape {
    /// Whether the row has field names (a struct or structs).
    pub(crate) named: bool,
    /// The number of fields.
    pub(crate) fields: usize,
    /// The names, in order (collected only when no expected names were given).
    pub(crate) names: Vec<&'static str>,
    /// With expected names: whether the row's names were exactly those, in order.
    pub(crate) matches: bool,
}

/// Why a row cannot be a CSV record. It stops the row's serialization where it is found.
#[derive(Debug)]
pub(crate) enum ShapeError {
    /// An unsupported shape (a static label).
    Shape(&'static str),
    /// The row's own `Serialize` failed (or a number had no text): its message, which may quote
    /// the value, kept only as the unredacted source.
    Custom(String),
    /// One field more than the field limit.
    Fields,
    /// The encoded record would exceed the record limit.
    Bytes,
}

impl ShapeError {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Shape(label) => label,
            Self::Custom(_) => "a CSV row cannot be written",
            Self::Fields => "a CSV row has more fields than the configured limit",
            Self::Bytes => "a CSV record exceeds the configured size",
        }
    }
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Custom(message) => f.write_str(message),
            other => f.write_str(other.label()),
        }
    }
}

impl std::error::Error for ShapeError {}

impl ser::Error for ShapeError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self::Custom(message.to_string())
    }
}

impl From<Exceeded> for ShapeError {
    fn from(Exceeded: Exceeded) -> Self {
        Self::Bytes
    }
}

impl From<FieldError> for ShapeError {
    fn from(error: FieldError) -> Self {
        match error {
            FieldError::Exceeded => Self::Bytes,
            FieldError::Text => Self::Custom("a number has no CSV text".to_owned()),
        }
    }
}

const fn shape_error(label: &'static str) -> ShapeError {
    ShapeError::Shape(label)
}

const MIXED: &str = "a CSV row mixes named struct fields and unnamed values";
const NESTED: &str =
    "a named CSV field holds a nested container (only scalars can be named fields)";
const MAP: &str = "a CSV row cannot be a map (or contain a flattened struct)";
const VARIANT: &str = "a CSV row cannot contain a tuple or struct enum variant";

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// Nothing seen yet.
    Start,
    /// Scalars outside any struct field: a positional row.
    Positional,
    /// Struct fields seen, outside a field value now.
    Named,
    /// Inside a struct field's value.
    InField,
}

/// Serializes `row` once into `out` (fields only; the caller ends the record), checking its shape
/// and at most `max_fields` fields. With `expect`, the names are compared with it instead of
/// collected; collected names are the serializer's own `&'static str`, never copied.
pub(crate) fn encode<T: Serialize + ?Sized>(
    row: &T,
    expect: Option<&[&str]>,
    max_fields: usize,
    out: &mut Fields<'_>,
) -> Result<Shape, ShapeError> {
    walk(row, expect, max_fields, Some(out))
}

/// The shape of `row`, nothing written. With `expect`, the names are compared with it instead of
/// collected.
#[cfg(test)]
pub(crate) fn shape<T: Serialize + ?Sized>(
    row: &T,
    expect: Option<&[String]>,
) -> Result<Shape, ShapeError> {
    let expect: Option<Vec<&str>> = expect.map(|e| e.iter().map(String::as_str).collect());
    walk(row, expect.as_deref(), usize::MAX, None)
}

fn walk<T: Serialize + ?Sized>(
    row: &T,
    expect: Option<&[&str]>,
    max_fields: usize,
    out: Option<&mut Fields<'_>>,
) -> Result<Shape, ShapeError> {
    let mut capture = Capture {
        state: State::Start,
        fields: 0,
        named: 0,
        names: Vec::new(),
        expect,
        matches: true,
        max_fields,
        out,
    };
    row.serialize(&mut capture)?;
    let named = capture.named > 0;
    let matches = match expect {
        Some(expect) => named && capture.matches && capture.named == expect.len(),
        None => true,
    };
    Ok(Shape {
        named,
        fields: capture.fields,
        names: capture.names,
        matches,
    })
}

struct Capture<'e, 'o, 'f> {
    state: State,
    fields: usize,
    named: usize,
    names: Vec<&'static str>,
    expect: Option<&'e [&'e str]>,
    matches: bool,
    max_fields: usize,
    out: Option<&'o mut Fields<'f>>,
}

impl Capture<'_, '_, '_> {
    /// Accounts for one scalar; the caller then writes its field.
    fn scalar(&mut self) -> Result<(), ShapeError> {
        match self.state {
            State::Start | State::Positional => self.state = State::Positional,
            State::InField => {}
            State::Named => return Err(shape_error(MIXED)),
        }
        if self.fields == self.max_fields {
            return Err(ShapeError::Fields);
        }
        self.fields += 1;
        Ok(())
    }

    /// A scalar whose field is `value`.
    fn bytes(&mut self, value: &[u8]) -> Result<(), ShapeError> {
        self.scalar()?;
        match &mut self.out {
            Some(out) => Ok(out.field(value)?),
            None => Ok(()),
        }
    }

    fn integer(&mut self, value: impl fmt::Display) -> Result<(), ShapeError> {
        self.scalar()?;
        match &mut self.out {
            Some(out) => Ok(out.integer(value)?),
            None => Ok(()),
        }
    }

    fn float<F: Serialize>(&mut self, value: F) -> Result<(), ShapeError> {
        self.scalar()?;
        match &mut self.out {
            Some(out) => Ok(out.float(value)?),
            None => Ok(()),
        }
    }

    fn container(&mut self) -> Result<&mut Self, ShapeError> {
        if self.state == State::InField {
            Err(shape_error(NESTED))
        } else {
            Ok(self)
        }
    }

    fn field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), ShapeError> {
        match self.state {
            State::Positional => return Err(shape_error(MIXED)),
            State::InField => return Err(shape_error(NESTED)),
            State::Start | State::Named => {}
        }
        match self.expect {
            Some(expect) => {
                if expect.get(self.named).copied() != Some(key) {
                    self.matches = false;
                }
            }
            None => self.names.push(key),
        }
        self.named += 1;
        self.state = State::InField;
        let before = self.fields;
        value.serialize(&mut *self)?;
        if self.fields != before + 1 {
            return Err(shape_error(NESTED));
        }
        self.state = State::Named;
        Ok(())
    }
}

impl ser::Serializer for &mut Capture<'_, '_, '_> {
    type Ok = ();
    type Error = ShapeError;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = ser::Impossible<(), ShapeError>;
    type SerializeMap = ser::Impossible<(), ShapeError>;
    type SerializeStruct = Self;
    type SerializeStructVariant = ser::Impossible<(), ShapeError>;

    // The value text of `csv`'s record serializer, scalar by scalar.
    fn serialize_bool(self, v: bool) -> Result<(), ShapeError> {
        self.bytes(if v { b"true" } else { b"false" })
    }
    fn serialize_i8(self, v: i8) -> Result<(), ShapeError> {
        self.integer(v)
    }
    fn serialize_i16(self, v: i16) -> Result<(), ShapeError> {
        self.integer(v)
    }
    fn serialize_i32(self, v: i32) -> Result<(), ShapeError> {
        self.integer(v)
    }
    fn serialize_i64(self, v: i64) -> Result<(), ShapeError> {
        self.integer(v)
    }
    fn serialize_i128(self, v: i128) -> Result<(), ShapeError> {
        self.integer(v)
    }
    fn serialize_u8(self, v: u8) -> Result<(), ShapeError> {
        self.integer(v)
    }
    fn serialize_u16(self, v: u16) -> Result<(), ShapeError> {
        self.integer(v)
    }
    fn serialize_u32(self, v: u32) -> Result<(), ShapeError> {
        self.integer(v)
    }
    fn serialize_u64(self, v: u64) -> Result<(), ShapeError> {
        self.integer(v)
    }
    fn serialize_u128(self, v: u128) -> Result<(), ShapeError> {
        self.integer(v)
    }
    fn serialize_f32(self, v: f32) -> Result<(), ShapeError> {
        self.float(v)
    }
    fn serialize_f64(self, v: f64) -> Result<(), ShapeError> {
        self.float(v)
    }
    fn serialize_char(self, v: char) -> Result<(), ShapeError> {
        self.bytes(v.encode_utf8(&mut [0; 4]).as_bytes())
    }
    fn serialize_str(self, v: &str) -> Result<(), ShapeError> {
        self.bytes(v.as_bytes())
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<(), ShapeError> {
        self.bytes(v)
    }
    fn serialize_none(self) -> Result<(), ShapeError> {
        self.bytes(b"")
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), ShapeError> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), ShapeError> {
        self.bytes(b"")
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<(), ShapeError> {
        self.bytes(name.as_bytes())
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<(), ShapeError> {
        self.bytes(variant.as_bytes())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), ShapeError> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        value: &T,
    ) -> Result<(), ShapeError> {
        value.serialize(self)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self, ShapeError> {
        self.container()
    }
    fn serialize_tuple(self, _: usize) -> Result<Self, ShapeError> {
        self.container()
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self, ShapeError> {
        self.container()
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, ShapeError> {
        Err(shape_error(VARIANT))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, ShapeError> {
        Err(shape_error(MAP))
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self, ShapeError> {
        self.container()
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, ShapeError> {
        Err(shape_error(VARIANT))
    }
}

impl ser::SerializeSeq for &mut Capture<'_, '_, '_> {
    type Ok = ();
    type Error = ShapeError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ShapeError> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<(), ShapeError> {
        Ok(())
    }
}

impl ser::SerializeTuple for &mut Capture<'_, '_, '_> {
    type Ok = ();
    type Error = ShapeError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ShapeError> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<(), ShapeError> {
        Ok(())
    }
}

impl ser::SerializeTupleStruct for &mut Capture<'_, '_, '_> {
    type Ok = ();
    type Error = ShapeError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ShapeError> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<(), ShapeError> {
        Ok(())
    }
}

impl ser::SerializeStruct for &mut Capture<'_, '_, '_> {
    type Ok = ();
    type Error = ShapeError;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), ShapeError> {
        self.field(key, value)
    }
    fn end(self) -> Result<(), ShapeError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Row {
        id: u32,
        #[serde(rename = "Display Name")]
        name: &'static str,
        note: Option<&'static str>,
    }

    #[derive(Serialize)]
    struct Pair {
        x: u8,
        y: u8,
    }

    #[test]
    fn shapes_follow_the_csv_header_rules() {
        let row = Row {
            id: 1,
            name: "a",
            note: None,
        };
        let s = shape(&row, None).unwrap();
        assert!(s.named);
        assert_eq!((s.fields, s.names), (3, vec!["id", "Display Name", "note"]));

        let s = shape(&(1u8, "a", Some(2.5f64)), None).unwrap();
        assert!(!s.named);
        assert_eq!(s.fields, 3);

        let s = shape(&(Pair { x: 1, y: 2 }, Pair { x: 3, y: 4 }), None).unwrap();
        assert_eq!(s.names, ["x", "y", "x", "y"]);

        #[derive(Serialize)]
        struct Nested {
            a: u8,
            b: (u8, u8),
        }
        assert_eq!(
            shape(&Nested { a: 1, b: (2, 3) }, None)
                .unwrap_err()
                .label(),
            NESTED
        );
        assert_eq!(
            shape(&(1u8, Pair { x: 1, y: 2 }), None)
                .unwrap_err()
                .label(),
            MIXED
        );
        let map: std::collections::BTreeMap<&str, u8> = [("a", 1)].into();
        assert_eq!(shape(&map, None).unwrap_err().label(), MAP);
    }

    #[test]
    fn expected_names_are_compared_in_order() {
        let expect: Vec<String> = ["id", "Display Name", "note"].map(String::from).into();
        let row = Row {
            id: 1,
            name: "a",
            note: Some("n"),
        };
        assert!(shape(&row, Some(&expect)).unwrap().matches);
        let reordered: Vec<String> = ["Display Name", "id", "note"].map(String::from).into();
        assert!(!shape(&row, Some(&reordered)).unwrap().matches);
        assert!(!shape(&row, Some(&expect[..2])).unwrap().matches);
        assert!(!shape(&(1, 2, 3), Some(&expect)).unwrap().matches);
    }
}
