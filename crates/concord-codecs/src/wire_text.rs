//! The wire text of a value with a scalar serde representation (`#[derive(WireDisplay)]`).

use std::fmt::{self, Display};

use serde::Serialize;
use serde::ser::{self, Impossible};

/// Writes `value`'s serde representation as text: strings and chars as written, booleans and
/// numbers in decimal, a unit variant as its (renamed) name, a newtype struct or `Some` as its
/// content. Any other shape is `fmt::Error`, which a placement reports as a build error naming
/// itself. Formatter flags (width, fill) are ignored: the text is exactly the wire text.
pub fn wire_text<T: Serialize + ?Sized>(value: &T, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    value.serialize(Scalar(f)).map_err(|NotScalar| fmt::Error)
}

/// The refusal of a shape that is not one scalar. It carries no text of the value.
#[derive(Debug)]
struct NotScalar;

impl Display for NotScalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the value is not one scalar")
    }
}

impl std::error::Error for NotScalar {}

impl ser::Error for NotScalar {
    fn custom<M: Display>(_: M) -> Self {
        NotScalar
    }
}

struct Scalar<'a, 'b>(&'a mut fmt::Formatter<'b>);

impl Scalar<'_, '_> {
    fn text(self, text: &str) -> Result<(), NotScalar> {
        self.0.write_str(text).map_err(|_| NotScalar)
    }

    fn display(self, value: impl Display) -> Result<(), NotScalar> {
        write!(self.0, "{value}").map_err(|_| NotScalar)
    }
}

type Refused = Impossible<(), NotScalar>;

impl ser::Serializer for Scalar<'_, '_> {
    type Ok = ();
    type Error = NotScalar;
    type SerializeSeq = Refused;
    type SerializeTuple = Refused;
    type SerializeTupleStruct = Refused;
    type SerializeTupleVariant = Refused;
    type SerializeMap = Refused;
    type SerializeStruct = Refused;
    type SerializeStructVariant = Refused;

    fn serialize_bool(self, v: bool) -> Result<(), NotScalar> {
        self.display(v)
    }
    fn serialize_i8(self, v: i8) -> Result<(), NotScalar> {
        self.display(v)
    }
    fn serialize_i16(self, v: i16) -> Result<(), NotScalar> {
        self.display(v)
    }
    fn serialize_i32(self, v: i32) -> Result<(), NotScalar> {
        self.display(v)
    }
    fn serialize_i64(self, v: i64) -> Result<(), NotScalar> {
        self.display(v)
    }
    fn serialize_i128(self, v: i128) -> Result<(), NotScalar> {
        self.display(v)
    }
    fn serialize_u8(self, v: u8) -> Result<(), NotScalar> {
        self.display(v)
    }
    fn serialize_u16(self, v: u16) -> Result<(), NotScalar> {
        self.display(v)
    }
    fn serialize_u32(self, v: u32) -> Result<(), NotScalar> {
        self.display(v)
    }
    fn serialize_u64(self, v: u64) -> Result<(), NotScalar> {
        self.display(v)
    }
    fn serialize_u128(self, v: u128) -> Result<(), NotScalar> {
        self.display(v)
    }
    fn serialize_f32(self, v: f32) -> Result<(), NotScalar> {
        self.display(v)
    }
    fn serialize_f64(self, v: f64) -> Result<(), NotScalar> {
        self.display(v)
    }
    fn serialize_char(self, v: char) -> Result<(), NotScalar> {
        self.display(v)
    }
    fn serialize_str(self, v: &str) -> Result<(), NotScalar> {
        self.text(v)
    }
    fn collect_str<T: Display + ?Sized>(self, value: &T) -> Result<(), NotScalar> {
        self.display(value)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), NotScalar> {
        value.serialize(self)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), NotScalar> {
        value.serialize(self)
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<(), NotScalar> {
        self.text(variant)
    }

    fn serialize_bytes(self, _: &[u8]) -> Result<(), NotScalar> {
        Err(NotScalar)
    }
    fn serialize_none(self) -> Result<(), NotScalar> {
        Err(NotScalar)
    }
    fn serialize_unit(self) -> Result<(), NotScalar> {
        Err(NotScalar)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), NotScalar> {
        Err(NotScalar)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<(), NotScalar> {
        Err(NotScalar)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Refused, NotScalar> {
        Err(NotScalar)
    }
    fn serialize_tuple(self, _: usize) -> Result<Refused, NotScalar> {
        Err(NotScalar)
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Refused, NotScalar> {
        Err(NotScalar)
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Refused, NotScalar> {
        Err(NotScalar)
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Refused, NotScalar> {
        Err(NotScalar)
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Refused, NotScalar> {
        Err(NotScalar)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Refused, NotScalar> {
        Err(NotScalar)
    }
}
