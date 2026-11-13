use std::fmt;

use super::Written;
use crate::secret::Secret;

/// How a value is written as text, in a path, a host, a query or a header: by `serde`.
/// Nothing (`None`), one text, or a list of texts.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be written in a path, a query or a header",
    note = "it needs serde::Serialize, and must give one text, a list of texts, or nothing"
)]
pub trait Value {
    /// Writes the value.
    fn write(&self, to: &mut Written);
}

impl<T: serde::Serialize + ?Sized> Value for T {
    fn write(&self, to: &mut Written) {
        to.store(self.serialize(TextWriter::ROOT), false);
    }
}

impl Value for Secret {
    fn write(&self, to: &mut Written) {
        to.store(Ok(Shape::One(self.expose().to_string())), true);
    }
}

impl Value for &Secret {
    fn write(&self, to: &mut Written) {
        (*self).write(to);
    }
}

/// A value before auto-reference dispatch chooses its writer.
///
/// A secret or an optional secret can be placed; a list of secrets cannot:
///
/// ```compile_fail
/// use concord_core::{Secret, __private::{AdaptValue, Piece, ValueRef}};
/// let secrets = vec![Secret::new("key")];
/// let _ = Piece::Val("tokens", &(&ValueRef(&secrets)).adapt());
/// ```
pub struct ValueRef<T>(pub T);

impl<T> fmt::Debug for ValueRef<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ValueRef(..)")
    }
}

/// The secret shapes that generated placements accept, including borrowed inputs.
pub trait SecretInput {
    /// Write this placement while keeping Secret itself outside Serde.
    fn __serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error>;
}
impl SecretInput for Secret {
    fn __serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.expose())
    }
}
impl<T: SecretInput> SecretInput for Option<T> {
    fn __serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Some(value) => serializer.serialize_some(&BodySecretValue(value)),
            None => serializer.serialize_none(),
        }
    }
}
impl<T: SecretInput + ?Sized> SecretInput for &T {
    fn __serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (**self).__serialize(serializer)
    }
}

/// A secret placement, kept outside serde.
pub struct SecretValue<'a, T>(&'a T);

impl<T> fmt::Debug for SecretValue<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue(..)")
    }
}

/// A body-only serialization view of an adapted secret placement.
#[doc(hidden)]
pub struct BodySecretValue<'a, T>(&'a T);
impl<T> fmt::Debug for BodySecretValue<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BodySecretValue(..)")
    }
}
impl<'a, T: SecretInput> crate::body::EntryValue for SecretValue<'a, T> {
    type Serialized = BodySecretValue<'a, T>;
    fn __body_value(self) -> Self::Serialized {
        BodySecretValue(self.0)
    }
}
impl<T: SecretInput> serde::Serialize for BodySecretValue<'_, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.__serialize(serializer)
    }
}

impl<T: SecretInput> Value for SecretValue<'_, T> {
    fn write(&self, to: &mut Written) {
        to.store(self.0.__serialize(TextWriter::ROOT), true);
    }
}

/// Import this trait and call `(&ValueRef(value)).adapt()` for every placement.
/// The first reference accepts secrets; one more auto-reference selects serde.
pub trait AdaptValue {
    /// What the draft's writer accepts.
    type Adapted: Value;
    /// Choose the writer without making a secret serializable.
    fn adapt(self) -> Self::Adapted;
}

impl<'a, T: SecretInput> AdaptValue for &'a ValueRef<T> {
    type Adapted = SecretValue<'a, T>;
    fn adapt(self) -> Self::Adapted {
        SecretValue(&self.0)
    }
}

impl<'a, T: serde::Serialize> AdaptValue for &&'a ValueRef<T> {
    type Adapted = &'a T;
    fn adapt(self) -> Self::Adapted {
        &self.0
    }
}

/// An adapted entry inherited across another base after binding.
///
/// A secret entry or path is bound to the base in reach where it is written:
/// its own node's base, or the nearest one above. Only if none is in reach is it
/// bound to the first base below it on each branch. Another base further down,
/// even with identical text, requires an override or removal of a secret entry.
/// A caller's with_base moves everything.
/// Path holes use the separate inherited_path gate.
///
/// Check each value before erasing it to `dyn Value`.
///
/// ```compile_fail
/// use concord_core::{Secret, __private::{AdaptValue, ValueRef, inherited}};
/// let token = Secret::new("key");
/// let _ = inherited((&ValueRef(&token)).adapt());
/// ```
///
/// ```compile_fail
/// use concord_core::{Secret, __private::{AdaptValue, ValueRef, inherited}};
/// let token = None::<Secret>;
/// let _ = inherited((&ValueRef(&token)).adapt());
/// ```
#[diagnostic::on_unimplemented(
    message = "a secret does not follow a change of base",
    label = "this inherited entry holds a Secret",
    note = "write this entry again with override, or remove it, under the new base"
)]
pub trait InheritAcrossBase {}
impl<T: serde::Serialize + ?Sized> InheritAcrossBase for &T {}

/// Check an inherited entry after its first base has already bound it.
pub fn inherited<V: InheritAcrossBase>(value: V) -> V {
    value
}

/// A path hole crossing a second base: paths cannot be overridden or removed.
///
/// ```compile_fail
/// use concord_core::{Secret, __private::{AdaptValue, ValueRef, inherited_path}};
/// let token = Secret::new("key");
/// let _ = inherited_path((&ValueRef(&token)).adapt());
/// ```
#[diagnostic::on_unimplemented(
    message = "a secret in a path does not follow a change of base",
    label = "this inherited path holds a Secret",
    note = "write this path under each base that needs it, rather than above those bases"
)]
pub trait InheritPathAcrossBase {}
impl<T: serde::Serialize + ?Sized> InheritPathAcrossBase for &T {}

/// Check each adapted path hole when it crosses a base after its first binding.
pub fn inherited_path<V: InheritPathAcrossBase>(value: V) -> V {
    value
}

/// What the description says about repeating a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Idempotent {
    /// Nothing written: the method that is really sent decides (`GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`).
    ByMethod,
    /// `idempotent;` or `idempotent true;`
    Yes,
    /// `idempotent false;`
    No,
}

impl From<bool> for Idempotent {
    fn from(value: bool) -> Self {
        if value {
            Self::Yes
        } else {
            Self::No
        }
    }
}

/// Accept the two declared forms of an `idempotent` expression.
/// Generated expressions run in a plain closure, without a `Result` or async context.
///
/// ```compile_fail
/// # async fn check() -> Result<(), ()> {
/// let _ = (|| concord_core::__private::idempotent(Ok::<_, ()>(true)?))();
/// # Ok(()) }
/// ```
///
/// ```compile_fail
/// # async fn check() {
/// let _ = (|| concord_core::__private::idempotent(async { true }.await))();
/// # }
/// ```
pub fn idempotent(value: impl BoolInput) -> Idempotent {
    Idempotent::from(value.__bool())
}

mod bool_sealed {
    pub trait Sealed {}
    impl Sealed for bool {}
    impl Sealed for &bool {}
}

/// Only a boolean or a borrowed boolean is an idempotent expression.
///
/// ```compile_fail
/// concord_core::__private::idempotent(Box::new(true));
/// ```
/// ```compile_fail
/// struct Other;
/// impl std::borrow::Borrow<bool> for Other { fn borrow(&self) -> &bool { &true } }
/// concord_core::__private::idempotent(Other);
/// ```
pub trait BoolInput: bool_sealed::Sealed {
    /// The boolean value.
    fn __bool(self) -> bool;
}
impl BoolInput for bool {
    fn __bool(self) -> bool {
        self
    }
}
impl BoolInput for &bool {
    fn __bool(self) -> bool {
        *self
    }
}

/// The three wire shapes, before a placement applies its own restrictions.
pub(super) enum Shape {
    Nothing,
    One(String),
    List(Vec<String>),
}

impl Shape {
    pub(super) fn is_empty(&self) -> bool {
        matches!(self, Self::Nothing) || matches!(self, Self::List(values) if values.is_empty())
    }
}
impl<'a> IntoIterator for &'a Shape {
    type Item = &'a String;
    type IntoIter =
        std::iter::Chain<std::option::IntoIter<&'a String>, std::slice::Iter<'a, String>>;
    fn into_iter(self) -> Self::IntoIter {
        let (one, many) = match self {
            Shape::Nothing => (None, [].as_slice()),
            Shape::One(value) => (Some(value), [].as_slice()),
            Shape::List(values) => (None, values.as_slice()),
        };
        one.into_iter().chain(many)
    }
}
impl IntoIterator for Shape {
    type Item = String;
    type IntoIter = std::iter::Chain<std::option::IntoIter<String>, std::vec::IntoIter<String>>;
    fn into_iter(self) -> Self::IntoIter {
        let (one, many) = match self {
            Self::Nothing => (None, Vec::new()),
            Self::One(value) => (Some(value), Vec::new()),
            Self::List(values) => (None, values),
        };
        one.into_iter().chain(many)
    }
}

/// A safe classification; never retains a serializer's potentially secret message.
#[derive(Clone, Copy, Debug)]
pub(super) enum WriteFailure {
    Shape,
    Depth,
    Nonfinite,
}
impl fmt::Display for WriteFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("value cannot be written")
    }
}
impl std::error::Error for WriteFailure {}
impl serde::ser::Error for WriteFailure {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self::Shape
    }
}

struct TextWriter {
    list: bool,
    depth: u8,
}
impl TextWriter {
    const ROOT: Self = Self {
        list: true,
        depth: 0,
    };
    fn nested(self) -> Result<Self, WriteFailure> {
        if self.depth == 128 {
            return Err(WriteFailure::Depth);
        }
        Ok(Self {
            // PANIC-SAFETY: nested() rejects depth 128 before incrementing this private u8 counter.
            depth: self.depth + 1,
            ..self
        })
    }
}
struct TextList(Vec<String>, u8);
impl TextList {
    fn push<T: serde::Serialize + ?Sized>(&mut self, value: &T) -> Result<(), WriteFailure> {
        match value.serialize(TextWriter {
            list: false,
            depth: self.1,
        })? {
            Shape::One(text) => {
                self.0.push(text);
                Ok(())
            }
            _ => Err(WriteFailure::Shape),
        }
    }
}

// Rust and JSON both use shortest round-tripping digits, but Rust breaks an
// exact decimal tie away from zero and JSON breaks it to even. Compare that
// midpoint to the exact IEEE binary value before choosing the last digit.
fn float_text(scientific: String, small: bool, binary: u64, binary_exp: i32) -> String {
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let sign = if mantissa.starts_with('-') { "-" } else { "" };
    let mut digits: String = mantissa
        .trim_start_matches('-')
        .chars()
        .filter(|&c| c != '.')
        .collect();
    let coefficient: u64 = digits.parse().unwrap_or(0);
    // PANIC-SAFETY: Finite IEEE shortest formatting has at most 17 digits and exponent -324..308; i32 is ample.
    let scale = exponent + 1 - digits.len() as i32;
    // PANIC-SAFETY: The divisor is the nonzero constant two.
    if coefficient % 2 == 1 && decimal_midpoint(coefficient, scale, binary, binary_exp) {
        // PANIC-SAFETY: This branch requires an odd coefficient, hence at least one.
        digits = (coefficient - 1).to_string();
    }
    // PANIC-SAFETY: These are small constant signed range bounds.
    if !(if small { -6..=12 } else { -5..=15 }).contains(&exponent) {
        let mut chars = digits.chars();
        let first = chars.next().unwrap_or('0');
        let tail: String = chars.collect();
        let dot = if tail.is_empty() { "" } else { "." };
        return format!("{sign}{first}{dot}{tail}e{exponent:+}");
    }
    let mut text = String::from(sign);
    // PANIC-SAFETY: Finite IEEE decimal exponents lie between -324 and 308.
    let point = exponent + 1;
    if point <= 0 {
        text.push_str("0.");
        for _ in point..0 {
            text.push('0');
        }
        text.push_str(&digits);
    } else {
        let mut chars = digits.chars();
        for _ in 0..point {
            text.push(chars.next().unwrap_or('0'));
        }
        text.push('.');
        let tail: String = chars.collect();
        text.push_str(if tail.is_empty() { "0" } else { &tail });
    }
    text
}
fn decimal_midpoint(coefficient: u64, scale: i32, binary: u64, binary_exp: i32) -> bool {
    if binary == 0 {
        return false;
    }
    let zeros = binary.trailing_zeros();
    // PANIC-SAFETY: binary == 0 returned earlier, so trailing_zeros is below the u64 width.
    let odd = binary >> zeros;
    // PANIC-SAFETY: A nonzero finite decimal coefficient is at most 17 digits; doubling and subtracting one fit u64.
    let mut midpoint = coefficient * 2 - 1;
    if scale < 0 {
        for _ in scale..0 {
            if !midpoint.is_multiple_of(5) {
                return false;
            }
            // PANIC-SAFETY: The divisor is the nonzero constant five.
            midpoint /= 5;
        }
    } else {
        for _ in 0..scale {
            let Some(next) = midpoint.checked_mul(5) else {
                return false;
            };
            midpoint = next;
            if midpoint > odd {
                return false;
            }
        }
    }
    // PANIC-SAFETY: Finite IEEE decimal exponents lie between -324 and 308.
    midpoint == odd && scale - 1 == binary_exp + zeros as i32
}
fn float32(value: f32) -> String {
    let bits = value.to_bits();
    // PANIC-SAFETY: The masked IEEE exponent is at most 2047, and the fixed shift is below the word width.
    let exp = ((bits >> 23) & 255) as i32;
    // PANIC-SAFETY: The fixed IEEE significand shift is below the word width and fits its unsigned result.
    let sig = u64::from(bits & 0x7fffff) | if exp == 0 { 0 } else { 1 << 23 };
    float_text(
        format!("{value:e}"),
        true,
        sig,
        // PANIC-SAFETY: The masked IEEE exponent is 0..2047; subtracting at most 1075 fits i32.
        if exp == 0 { -149 } else { exp - 150 },
    )
}
fn float64(value: f64) -> String {
    let bits = value.to_bits();
    // PANIC-SAFETY: The masked IEEE exponent is at most 2047, and the fixed shift is below the word width.
    let exp = ((bits >> 52) & 2047) as i32;
    // PANIC-SAFETY: The fixed IEEE significand shift is below the word width and fits its unsigned result.
    let sig = (bits & 0x000f_ffff_ffff_ffff) | if exp == 0 { 0 } else { 1 << 52 };
    float_text(
        format!("{value:e}"),
        false,
        sig,
        // PANIC-SAFETY: The masked IEEE exponent is 0..2047; subtracting at most 1075 fits i32.
        if exp == 0 { -1074 } else { exp - 1075 },
    )
}

macro_rules! scalar {
    ($($name:ident($ty:ty);)*) => {$(
        fn $name(self, value: $ty) -> Result<Shape, WriteFailure> { Ok(Shape::One(value.to_string())) }
    )*};
}
macro_rules! unsupported {
    ($($name:ident($($arg:ident: $ty:ty),*) -> $result:ty;)*) => {$(
        fn $name(self, $(_: $ty),*) -> Result<$result, WriteFailure> { Err(WriteFailure::Shape) }
    )*};
}
impl serde::Serializer for TextWriter {
    type Ok = Shape;
    type Error = WriteFailure;
    type SerializeSeq = TextList;
    type SerializeTuple = TextList;
    type SerializeTupleStruct = serde::ser::Impossible<Shape, WriteFailure>;
    type SerializeTupleVariant = serde::ser::Impossible<Shape, WriteFailure>;
    type SerializeMap = serde::ser::Impossible<Shape, WriteFailure>;
    type SerializeStruct = serde::ser::Impossible<Shape, WriteFailure>;
    type SerializeStructVariant = serde::ser::Impossible<Shape, WriteFailure>;
    scalar! {
        serialize_bool(bool); serialize_i8(i8); serialize_i16(i16); serialize_i32(i32);
        serialize_i64(i64); serialize_i128(i128); serialize_u8(u8); serialize_u16(u16);
        serialize_u32(u32); serialize_u64(u64); serialize_u128(u128); serialize_char(char);
        serialize_str(&str);
    }
    fn collect_str<T: fmt::Display + ?Sized>(self, value: &T) -> Result<Shape, WriteFailure> {
        use fmt::Write;
        let mut text = String::new();
        write!(&mut text, "{value}").map_err(|_| WriteFailure::Shape)?;
        Ok(Shape::One(text))
    }
    fn serialize_f32(self, value: f32) -> Result<Shape, WriteFailure> {
        if !value.is_finite() {
            return Err(WriteFailure::Nonfinite);
        }
        Ok(Shape::One(float32(value)))
    }
    fn serialize_f64(self, value: f64) -> Result<Shape, WriteFailure> {
        if !value.is_finite() {
            return Err(WriteFailure::Nonfinite);
        }
        Ok(Shape::One(float64(value)))
    }
    fn serialize_none(self) -> Result<Shape, WriteFailure> {
        Ok(Shape::Nothing)
    }
    fn serialize_unit(self) -> Result<Shape, WriteFailure> {
        Ok(Shape::Nothing)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<Shape, WriteFailure> {
        Err(WriteFailure::Shape)
    }
    fn serialize_some<T: serde::Serialize + ?Sized>(
        self,
        value: &T,
    ) -> Result<Shape, WriteFailure> {
        value.serialize(self.nested()?)
    }
    fn serialize_newtype_struct<T: serde::Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<Shape, WriteFailure> {
        value.serialize(self.nested()?)
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<Shape, WriteFailure> {
        Ok(Shape::One(variant.to_owned()))
    }
    fn serialize_newtype_variant<T: serde::Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<Shape, WriteFailure> {
        Err(WriteFailure::Shape)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<TextList, WriteFailure> {
        if self.list {
            Ok(TextList(Vec::new(), self.nested()?.depth))
        } else {
            Err(WriteFailure::Shape)
        }
    }
    fn serialize_tuple(self, _: usize) -> Result<TextList, WriteFailure> {
        self.serialize_seq(None)
    }
    unsupported! {
        serialize_bytes(value: &[u8]) -> Shape;
        serialize_tuple_struct(name: &'static str, len: usize) -> Self::SerializeTupleStruct;
        serialize_tuple_variant(name: &'static str, index: u32, variant: &'static str, len: usize) -> Self::SerializeTupleVariant;
        serialize_map(len: Option<usize>) -> Self::SerializeMap;
        serialize_struct(name: &'static str, len: usize) -> Self::SerializeStruct;
        serialize_struct_variant(name: &'static str, index: u32, variant: &'static str, len: usize) -> Self::SerializeStructVariant;
    }
}
impl serde::ser::SerializeSeq for TextList {
    type Ok = Shape;
    type Error = WriteFailure;
    fn serialize_element<T: serde::Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<(), WriteFailure> {
        self.push(value)
    }
    fn end(self) -> Result<Shape, WriteFailure> {
        Ok(Shape::List(self.0))
    }
}
impl serde::ser::SerializeTuple for TextList {
    type Ok = Shape;
    type Error = WriteFailure;
    fn serialize_element<T: serde::Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<(), WriteFailure> {
        self.push(value)
    }
    fn end(self) -> Result<Shape, WriteFailure> {
        Ok(Shape::List(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // The reference is intentional: this test covers the borrowed-bool implementation.
    #[allow(clippy::needless_borrows_for_generic_args)]
    fn idempotent_accepts_owned_and_borrowed_bools() {
        for value in [true, false] {
            assert_eq!(idempotent(value), Idempotent::from(value));
            assert_eq!(idempotent(&value), Idempotent::from(value));
        }
    }
    fn written(value: &dyn Value) -> (Result<Shape, WriteFailure>, bool) {
        let mut to = Written::default();
        value.write(&mut to);
        to.into_parts()
    }
    fn text(value: &dyn Value) -> String {
        match written(value).0.unwrap() {
            Shape::One(text) => text,
            _ => panic!("expected one text"),
        }
    }
    #[test]
    fn c2_4_scalar_serde_names_newtypes_options_and_integer_limits() {
        #[derive(serde::Serialize)]
        enum Enum {
            #[serde(rename = "wire-name")]
            Unit,
        }
        #[derive(serde::Serialize)]
        struct Newtype<T>(T);
        for (value, expected) in [
            (&true as &dyn Value, "true".to_owned()),
            (&false, "false".to_owned()),
            (&'é', "é".to_owned()),
            (&"text", "text".to_owned()),
            (&i128::MIN, i128::MIN.to_string()),
            (&u128::MAX, u128::MAX.to_string()),
            (&i64::MIN, i64::MIN.to_string()),
            (&u64::MAX, u64::MAX.to_string()),
            (&Enum::Unit, "wire-name".to_owned()),
            (&Newtype(Some(Newtype(5))), "5".to_owned()),
        ] {
            assert_eq!(text(value), expected);
        }
        for value in [
            &None::<u8> as &dyn Value,
            &(),
            &Some(None::<u8>),
            &Newtype(None::<u8>),
        ] {
            assert!(matches!(written(value).0, Ok(Shape::Nothing)));
        }
        assert!(
            matches!(written(&(1, "two", true)).0, Ok(Shape::List(values)) if values == ["1", "two", "true"])
        );
    }
    #[test]
    fn c2_4_float_spelling_matches_json_including_exact_decimal_ties() {
        for (value, expected) in [
            (1.0f64, "1.0"),
            (-0.0, "-0.0"),
            (1e-5, "0.00001"),
            (1e-6, "1e-6"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (f64::from_bits(0xc2e7_03c1_5756_ea84), "-202439168538452.12"),
            (f64::from_bits(0x4302_fc10_c056_663a), "667962307169479.2"),
            (f64::MAX, "1.7976931348623157e+308"),
            (f64::MIN_POSITIVE, "2.2250738585072014e-308"),
            (f64::from_bits(1), "5e-324"),
        ] {
            assert_eq!(text(&value), expected);
        }
        for (value, expected) in [
            (1.0f32, "1.0"),
            (-0.0, "-0.0"),
            (1e-6, "0.000001"),
            (1e-7, "1e-7"),
            (1e12, "1000000000000.0"),
            (1e13, "1e+13"),
            (f32::from_bits(0xc867_59c8), "-236903.12"),
            (f32::from_bits(0xc5e4_3d40), "-7303.6562"),
            (f32::MAX, "3.4028235e+38"),
            (f32::MIN_POSITIVE, "1.1754944e-38"),
            (f32::from_bits(1), "1e-45"),
        ] {
            assert_eq!(text(&value), expected);
        }
        for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            assert!(matches!(written(&value).0, Err(WriteFailure::Nonfinite)));
        }
        for value in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            assert!(matches!(written(&value).0, Err(WriteFailure::Nonfinite)));
        }
    }
    #[test]
    fn c2_4_nontext_shapes_and_nothing_inside_lists_are_rejected() {
        #[derive(serde::Serialize)]
        struct Record {
            n: u8,
        }
        #[derive(serde::Serialize)]
        struct Tuple(u8, u8);
        #[derive(serde::Serialize)]
        struct Unit;
        #[derive(serde::Serialize)]
        enum Enum {
            New(u8),
            Tuple(u8, u8),
            Struct { n: u8 },
        }
        struct Bytes;
        impl serde::Serialize for Bytes {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_bytes(b"bytes")
            }
        }
        for value in [
            &Record { n: 1 } as &dyn Value,
            &Tuple(1, 2),
            &Unit,
            &Enum::New(1),
            &Enum::Tuple(1, 2),
            &Enum::Struct { n: 1 },
            &Bytes,
            &std::collections::BTreeMap::from([("a", 1)]),
            &vec![vec![1]],
            &vec![None::<u8>],
            &vec![()],
        ] {
            assert!(matches!(written(value).0, Err(WriteFailure::Shape)));
        }
    }
    #[test]
    fn c2_4_serializer_errors_do_not_format_author_messages() {
        struct Message;
        impl fmt::Display for Message {
            fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
                panic!("message must not be formatted")
            }
        }
        struct Fails;
        impl serde::Serialize for Fails {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom(Message))
            }
        }
        assert!(matches!(written(&Fails).0, Err(WriteFailure::Shape)));
    }
    #[test]
    fn c2_4_secret_adapter_handles_borrowed_optional_nested_and_absent_values() {
        let secret = Secret::new("key");
        assert_eq!(text(&&secret), "key");
        let value = ValueRef(&secret);
        let adapted = (&value).adapt();
        assert_eq!(text(&adapted), "key");
        assert!(written(&adapted).1);
        let value = ValueRef(Some(Some(&secret)));
        let adapted = (&value).adapt();
        assert_eq!(text(&adapted), "key");
        assert!(written(&adapted).1);
        let value = ValueRef(None::<Secret>);
        let adapted = (&value).adapt();
        assert!(matches!(written(&adapted), (Ok(Shape::Nothing), true)));
        let value = ValueRef(Some(None::<Secret>));
        let adapted = (&value).adapt();
        assert!(matches!(written(&adapted), (Ok(Shape::Nothing), true)));
        let value = ValueRef(&5);
        let adapted = (&value).adapt();
        assert_eq!(text(&adapted), "5");
        assert!(!written(&adapted).1);
    }
    #[test]
    fn c2_4_collect_str_handles_a_formatting_error_without_panicking() {
        struct Fails;
        impl fmt::Display for Fails {
            fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
                Err(fmt::Error)
            }
        }
        impl serde::Serialize for Fails {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }
        assert!(matches!(written(&Fails).0, Err(WriteFailure::Shape)));
    }

    #[test]
    fn c2_5_m1_e1_nested_lists_stop_before_recursing_at_every_depth() {
        use std::cell::Cell;
        struct Nested<'a> {
            depth: usize,
            tuple: bool,
            calls: &'a Cell<usize>,
        }
        impl serde::Serialize for Nested<'_> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.calls.set(self.calls.get() + 1);
                if self.depth == 0 {
                    return serializer.serialize_u8(7);
                }
                let child = Nested {
                    depth: self.depth - 1,
                    ..*self
                };
                if self.tuple {
                    use serde::ser::SerializeTuple;
                    let mut list = serializer.serialize_tuple(1)?;
                    list.serialize_element(&child)?;
                    list.end()
                } else {
                    use serde::ser::SerializeSeq;
                    let mut list = serializer.serialize_seq(Some(1))?;
                    list.serialize_element(&child)?;
                    list.end()
                }
            }
        }
        // This generated depth property uses no recursive allocation or destruction.
        // The first failing shallow case also keeps the old implementation from aborting.
        for depth in (2..=256).chain([1024, 100_000, usize::MAX]) {
            for tuple in [false, true] {
                let calls = Cell::new(0);
                assert!(matches!(
                    written(&Nested {
                        depth,
                        tuple,
                        calls: &calls
                    })
                    .0,
                    Err(WriteFailure::Shape)
                ));
                assert_eq!(calls.get(), 2, "depth {depth}, tuple {tuple}");
            }
        }
    }
}
