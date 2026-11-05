//! The writer's schema capture: a small Serde serializer that records a row's field names and
//! counts its fields with the same shape rules as `csv`'s header writer, without producing any
//! output.
//!
//! Rules (those of `csv::Writer::serialize` with headers): every named struct field holds one
//! scalar; every scalar is either a named field value or the table is positional; structs may sit
//! inside tuples and sequences (their names are concatenated). Maps, flattened structs and
//! tuple/struct enum variants are refused, as `csv` refuses them.

use std::fmt;

use serde::ser::{self, Serialize};

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

/// Why a row's shape cannot be a CSV record: a static label, and for a failure of the row's own
/// `Serialize` implementation its message (which may quote the value: kept only as the
/// unredacted source).
#[derive(Debug)]
pub(crate) struct ShapeError {
    label: &'static str,
    message: Option<String>,
}

const fn shape_error(label: &'static str) -> ShapeError {
    ShapeError {
        label,
        message: None,
    }
}

impl ShapeError {
    pub(crate) fn label(&self) -> &'static str {
        self.label
    }

    /// Whether the row's own `Serialize` failed (rather than its shape being unsupported).
    pub(crate) fn is_custom(&self) -> bool {
        self.message.is_some()
    }
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message.as_deref().unwrap_or(self.label))
    }
}

impl std::error::Error for ShapeError {}

impl ser::Error for ShapeError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        ShapeError {
            label: "a CSV row cannot be written",
            message: Some(message.to_string()),
        }
    }
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

/// The shape of `row`. With `expect`, the names are compared with it instead of collected.
pub(crate) fn shape<T: Serialize + ?Sized>(
    row: &T,
    expect: Option<&[String]>,
) -> Result<Shape, ShapeError> {
    let mut capture = Capture {
        state: State::Start,
        fields: 0,
        named: 0,
        names: Vec::new(),
        expect,
        matches: true,
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

struct Capture<'e> {
    state: State,
    fields: usize,
    named: usize,
    names: Vec<&'static str>,
    expect: Option<&'e [String]>,
    matches: bool,
}

impl Capture<'_> {
    fn scalar(&mut self) -> Result<(), ShapeError> {
        match self.state {
            State::Start | State::Positional => {
                self.state = State::Positional;
                self.fields += 1;
                Ok(())
            }
            State::InField => {
                self.fields += 1;
                Ok(())
            }
            State::Named => Err(shape_error(MIXED)),
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
                if expect.get(self.named).map(String::as_str) != Some(key) {
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

impl ser::Serializer for &mut Capture<'_> {
    type Ok = ();
    type Error = ShapeError;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = ser::Impossible<(), ShapeError>;
    type SerializeMap = ser::Impossible<(), ShapeError>;
    type SerializeStruct = Self;
    type SerializeStructVariant = ser::Impossible<(), ShapeError>;

    fn serialize_bool(self, _: bool) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_i8(self, _: i8) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_i16(self, _: i16) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_i32(self, _: i32) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_i64(self, _: i64) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_i128(self, _: i128) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_u8(self, _: u8) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_u16(self, _: u16) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_u32(self, _: u32) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_u64(self, _: u64) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_u128(self, _: u128) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_f32(self, _: f32) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_f64(self, _: f64) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_char(self, _: char) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_str(self, _: &str) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_none(self) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), ShapeError> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), ShapeError> {
        self.scalar()
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<(), ShapeError> {
        self.scalar()
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

impl ser::SerializeSeq for &mut Capture<'_> {
    type Ok = ();
    type Error = ShapeError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ShapeError> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<(), ShapeError> {
        Ok(())
    }
}

impl ser::SerializeTuple for &mut Capture<'_> {
    type Ok = ();
    type Error = ShapeError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ShapeError> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<(), ShapeError> {
        Ok(())
    }
}

impl ser::SerializeTupleStruct for &mut Capture<'_> {
    type Ok = ();
    type Error = ShapeError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ShapeError> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<(), ShapeError> {
        Ok(())
    }
}

impl ser::SerializeStruct for &mut Capture<'_> {
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
