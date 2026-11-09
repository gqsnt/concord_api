//! Stripe's form encoding: `application/x-www-form-urlencoded` with bracketed nested keys.
//!
//! Stripe v1 request bodies are forms whose keys spell structure with brackets (see `PROVIDER.md`
//! for the documented examples this follows):
//!
//! | Rust value | Pairs |
//! |---|---|
//! | struct field `name: "Gold"` | `name=Gold` |
//! | nested struct / map `metadata: {order_id: "6735"}` | `metadata[order_id]=6735` |
//! | sequence of scalars `expand: ["customer", "latest_invoice"]` | `expand[]=customer&expand[]=latest_invoice` (repeated field) |
//! | sequence of structs `items: [{price: "price_1"}]` | `items[0][price]=price_1` |
//! | `None` (an [`Option`] field) | nothing: the parameter is **omitted** |
//! | [`Patch::Unset`] / [`MetadataPatch::ClearAll`] | `description=` / `metadata=`: an **empty value**, which Stripe documents as "unset" |
//! | an empty sequence or map | nothing (omitted); clear a list with [`Patch::Unset`] |
//! | `bool`, integers, finite floats, unit enum variants | their text (`true`, `42`, `send_invoice`) |
//!
//! Keys and values are then percent-encoded with the `application/x-www-form-urlencoded` byte
//! serializer (brackets become `%5B`/`%5D`, space becomes `+`); Stripe decodes keys before
//! interpreting the brackets.
//!
//! Concord's built-in `Form` format (feature `form`, `serde_urlencoded`) is flat by design and
//! refuses nested values, so this SDK supplies its own [`StripeForm`] format through Concord's
//! public `Encode` trait (recorded in `FRICTION.md`). Pair order follows field declaration order
//! and map key order, so the same value always encodes to the same bytes (useful when an
//! application compares a repeated write with the original under one idempotency key).

use std::collections::BTreeMap;
use std::fmt;

use concord::{CodecError, Encode, reqwest};
use serde::ser::{self, Impossible, Serialize, SerializeMap, Serializer};

/// `Encode::CONTENT_TYPE` sends `content-type: application/x-www-form-urlencoded` with the body.
///
/// The request format of every Stripe write in this SDK: bracketed form encoding.
pub struct StripeForm;

impl<T: Serialize> Encode<T> for StripeForm {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-www-form-urlencoded");

    fn encode(value: T) -> Result<reqwest::Body, CodecError> {
        let pairs = to_pairs(&value).map_err(|error| {
            CodecError::new("the value cannot be encoded as a Stripe form").with_source(error)
        })?;
        Ok(reqwest::Body::from(encode_pairs(&pairs)))
    }
}

/// A field of an update body that can be set or explicitly unset.
///
/// Used as `Option<Patch<T>>`: `None` omits the parameter (leave it unchanged), `Some(Unset)`
/// sends an empty value (Stripe unsets the field), `Some(Set(v))` sends `v`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Patch<T> {
    /// Send this value.
    Set(T),
    /// Send an empty value: unset the field.
    Unset,
}

impl<T: Serialize> Serialize for Patch<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Set(value) => value.serialize(serializer),
            Self::Unset => serializer.serialize_str(""),
        }
    }
}

/// A change to an object's metadata: set or unset individual keys, or clear every key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetadataPatch {
    /// Change these keys: `Set` writes a value, `Unset` sends `metadata[key]=` (removes it).
    /// Keys not listed are left as they are.
    Entries(BTreeMap<String, Patch<String>>),
    /// `metadata=`: remove every key.
    ClearAll,
}

impl MetadataPatch {
    /// An empty set of changes.
    pub fn new() -> Self {
        Self::Entries(BTreeMap::new())
    }

    /// Adds `key = value` (switching from `ClearAll` to entries).
    pub fn set(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.entry(key.into(), Patch::Set(value.into()))
    }

    /// Adds `key` to the keys to remove.
    pub fn unset(self, key: impl Into<String>) -> Self {
        self.entry(key.into(), Patch::Unset)
    }

    fn entry(self, key: String, patch: Patch<String>) -> Self {
        let mut entries = match self {
            Self::Entries(entries) => entries,
            Self::ClearAll => BTreeMap::new(),
        };
        entries.insert(key, patch);
        Self::Entries(entries)
    }
}

impl Default for MetadataPatch {
    fn default() -> Self {
        Self::new()
    }
}

impl Serialize for MetadataPatch {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Entries(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
            Self::ClearAll => serializer.serialize_str(""),
        }
    }
}

/// Why a value could not be encoded. `Display` names the rule, never a key or value.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormError {
    /// The body is not a struct or a map (a form needs named fields).
    TopLevel,
    /// A value of this serde kind has no form spelling here (bytes, unit, tuple or struct enum
    /// variants, a newtype variant).
    Unsupported(&'static str),
    /// A key is empty or contains `[` or `]` (Stripe documents that metadata keys cannot
    /// contain square brackets; they would also be read as structure).
    Key,
    /// A map key is not text or an integer.
    KeyType,
    /// A float is NaN or infinite.
    NonFinite,
    /// A `Serialize` implementation reported its own error (its message is kept for deliberate
    /// inspection through [`FormError::custom_message`], not shown by `Display`).
    Custom(String),
}

impl FormError {
    /// The message of a [`FormError::Custom`] error, unredacted.
    pub fn custom_message(&self) -> Option<&str> {
        match self {
            Self::Custom(message) => Some(message),
            _ => None,
        }
    }
}

impl fmt::Display for FormError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TopLevel => f.write_str("a form body must be a struct or a map"),
            Self::Unsupported(kind) => write!(f, "a {kind} value has no form encoding"),
            Self::Key => f.write_str("a form key must be non-empty and contain no `[` or `]`"),
            Self::KeyType => f.write_str("a form map key must be text or an integer"),
            Self::NonFinite => f.write_str("a float in a form must be finite"),
            Self::Custom(_) => f.write_str("a value's serializer failed"),
        }
    }
}

impl std::error::Error for FormError {}

impl ser::Error for FormError {
    fn custom<M: fmt::Display>(message: M) -> Self {
        Self::Custom(message.to_string())
    }
}

/// The decoded `(key, value)` pairs of a body, in order (keys with their brackets, values as
/// text). Used by [`StripeForm`]; public so that applications and tests can inspect exactly what
/// a body will contain.
///
/// # Errors
///
/// [`FormError`] when the value has no form spelling (see the module table).
pub fn to_pairs<T: Serialize + ?Sized>(value: &T) -> Result<Vec<(String, String)>, FormError> {
    let mut out = Vec::new();
    value.serialize(TopLevel { out: &mut out })?;
    Ok(out)
}

/// Percent-encodes pairs as an `application/x-www-form-urlencoded` body.
pub fn encode_pairs(pairs: &[(String, String)]) -> String {
    let mut body = String::new();
    for (index, (key, value)) in pairs.iter().enumerate() {
        if index > 0 {
            body.push('&');
        }
        encode_component(&mut body, key);
        body.push('=');
        encode_component(&mut body, value);
    }
    body
}

/// The `application/x-www-form-urlencoded` byte serializer: `A-Z a-z 0-9 * - . _` unchanged,
/// space as `+`, every other byte as `%XX` (uppercase).
fn encode_component(out: &mut String, text: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(char::from(byte));
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(char::from(HEX[usize::from(byte >> 4)]));
                out.push(char::from(HEX[usize::from(byte & 0x0F)]));
            }
        }
    }
}

fn check_key(key: &str) -> Result<(), FormError> {
    if key.is_empty() || key.contains(['[', ']']) {
        Err(FormError::Key)
    } else {
        Ok(())
    }
}

/// `prefix[name]`, or `name` at the top level.
fn child(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}[{name}]")
    }
}

/// The top-level serializer: only a struct or a map has named fields.
struct TopLevel<'a> {
    out: &'a mut Vec<(String, String)>,
}

macro_rules! refuse_top_level {
    ($($method:ident($($arg:ty),*);)*) => {
        $(fn $method(self, $(_: $arg),*) -> Result<(), FormError> { Err(FormError::TopLevel) })*
    };
}

impl<'a> Serializer for TopLevel<'a> {
    type Ok = ();
    type Error = FormError;
    type SerializeSeq = Impossible<(), FormError>;
    type SerializeTuple = Impossible<(), FormError>;
    type SerializeTupleStruct = Impossible<(), FormError>;
    type SerializeTupleVariant = Impossible<(), FormError>;
    type SerializeMap = MapFields<'a>;
    type SerializeStruct = MapFields<'a>;
    type SerializeStructVariant = Impossible<(), FormError>;

    refuse_top_level! {
        serialize_bool(bool); serialize_i8(i8); serialize_i16(i16); serialize_i32(i32);
        serialize_i64(i64); serialize_u8(u8); serialize_u16(u16); serialize_u32(u32);
        serialize_u64(u64); serialize_f32(f32); serialize_f64(f64); serialize_char(char);
        serialize_str(&str); serialize_bytes(&[u8]); serialize_none(); serialize_unit();
        serialize_unit_struct(&'static str);
        serialize_unit_variant(&'static str, u32, &'static str);
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), FormError> {
        value.serialize(self)
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), FormError> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<(), FormError> {
        Err(FormError::TopLevel)
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, FormError> {
        Err(FormError::TopLevel)
    }

    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, FormError> {
        Err(FormError::TopLevel)
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, FormError> {
        Err(FormError::TopLevel)
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, FormError> {
        Err(FormError::TopLevel)
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, FormError> {
        Ok(MapFields::new(self.out, String::new()))
    }

    fn serialize_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, FormError> {
        Ok(MapFields::new(self.out, String::new()))
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, FormError> {
        Err(FormError::TopLevel)
    }
}

/// A value at one position: `scalar` is the key a scalar is written under, `compound` the
/// prefix a struct, map or sequence extends. They differ only for sequence elements
/// (`items[]` for a scalar, `items[0]` for an object).
struct Value<'a> {
    out: &'a mut Vec<(String, String)>,
    scalar: String,
    compound: String,
}

impl Value<'_> {
    fn push(self, value: String) -> Result<(), FormError> {
        self.out.push((self.scalar, value));
        Ok(())
    }
}

macro_rules! scalar_to_string {
    ($($method:ident($ty:ty);)*) => {
        $(fn $method(self, value: $ty) -> Result<(), FormError> { self.push(value.to_string()) })*
    };
}

impl<'a> Serializer for Value<'a> {
    type Ok = ();
    type Error = FormError;
    type SerializeSeq = Elements<'a>;
    type SerializeTuple = Elements<'a>;
    type SerializeTupleStruct = Elements<'a>;
    type SerializeTupleVariant = Impossible<(), FormError>;
    type SerializeMap = MapFields<'a>;
    type SerializeStruct = MapFields<'a>;
    type SerializeStructVariant = Impossible<(), FormError>;

    scalar_to_string! {
        serialize_bool(bool); serialize_i8(i8); serialize_i16(i16); serialize_i32(i32);
        serialize_i64(i64); serialize_i128(i128); serialize_u8(u8); serialize_u16(u16);
        serialize_u32(u32); serialize_u64(u64); serialize_u128(u128); serialize_char(char);
    }

    fn serialize_f32(self, value: f32) -> Result<(), FormError> {
        self.serialize_f64(f64::from(value))
    }

    fn serialize_f64(self, value: f64) -> Result<(), FormError> {
        if value.is_finite() {
            self.push(value.to_string())
        } else {
            Err(FormError::NonFinite)
        }
    }

    fn serialize_str(self, value: &str) -> Result<(), FormError> {
        self.push(value.to_owned())
    }

    fn serialize_bytes(self, _: &[u8]) -> Result<(), FormError> {
        Err(FormError::Unsupported("bytes"))
    }

    /// `None` is an omitted parameter.
    fn serialize_none(self) -> Result<(), FormError> {
        Ok(())
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), FormError> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<(), FormError> {
        Err(FormError::Unsupported("unit"))
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<(), FormError> {
        Err(FormError::Unsupported("unit struct"))
    }

    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<(), FormError> {
        self.push(variant.to_owned())
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), FormError> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<(), FormError> {
        Err(FormError::Unsupported("newtype variant"))
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Elements<'a>, FormError> {
        Ok(Elements {
            out: self.out,
            prefix: self.compound,
            index: 0,
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<Elements<'a>, FormError> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        len: usize,
    ) -> Result<Elements<'a>, FormError> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, FormError> {
        Err(FormError::Unsupported("tuple variant"))
    }

    fn serialize_map(self, _: Option<usize>) -> Result<MapFields<'a>, FormError> {
        Ok(MapFields::new(self.out, self.compound))
    }

    fn serialize_struct(self, _: &'static str, _: usize) -> Result<MapFields<'a>, FormError> {
        Ok(MapFields::new(self.out, self.compound))
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, FormError> {
        Err(FormError::Unsupported("struct variant"))
    }
}

/// The fields of a struct or the entries of a map under `prefix`.
struct MapFields<'a> {
    out: &'a mut Vec<(String, String)>,
    prefix: String,
    key: Option<String>,
}

impl<'a> MapFields<'a> {
    fn new(out: &'a mut Vec<(String, String)>, prefix: String) -> Self {
        Self {
            out,
            prefix,
            key: None,
        }
    }

    fn field<T: Serialize + ?Sized>(&mut self, name: &str, value: &T) -> Result<(), FormError> {
        check_key(name)?;
        let key = child(&self.prefix, name);
        value.serialize(Value {
            out: self.out,
            scalar: key.clone(),
            compound: key,
        })
    }
}

impl SerializeMap for MapFields<'_> {
    type Ok = ();
    type Error = FormError;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), FormError> {
        self.key = Some(key.serialize(KeyText)?);
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), FormError> {
        let key = self.key.take().ok_or(FormError::KeyType)?;
        self.field(&key, value)
    }

    fn end(self) -> Result<(), FormError> {
        Ok(())
    }
}

impl ser::SerializeStruct for MapFields<'_> {
    type Ok = ();
    type Error = FormError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        name: &'static str,
        value: &T,
    ) -> Result<(), FormError> {
        self.field(name, value)
    }

    fn end(self) -> Result<(), FormError> {
        Ok(())
    }
}

/// The elements of a sequence under `prefix`.
struct Elements<'a> {
    out: &'a mut Vec<(String, String)>,
    prefix: String,
    index: usize,
}

impl Elements<'_> {
    fn element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), FormError> {
        if self.prefix.is_empty() {
            return Err(FormError::TopLevel);
        }
        let scalar = format!("{}[]", self.prefix);
        let compound = format!("{}[{}]", self.prefix, self.index);
        self.index += 1;
        value.serialize(Value {
            out: self.out,
            scalar,
            compound,
        })
    }
}

impl ser::SerializeSeq for Elements<'_> {
    type Ok = ();
    type Error = FormError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), FormError> {
        self.element(value)
    }
    fn end(self) -> Result<(), FormError> {
        Ok(())
    }
}

impl ser::SerializeTuple for Elements<'_> {
    type Ok = ();
    type Error = FormError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), FormError> {
        self.element(value)
    }
    fn end(self) -> Result<(), FormError> {
        Ok(())
    }
}

impl ser::SerializeTupleStruct for Elements<'_> {
    type Ok = ();
    type Error = FormError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), FormError> {
        self.element(value)
    }
    fn end(self) -> Result<(), FormError> {
        Ok(())
    }
}

/// A map key: text or an integer.
struct KeyText;

macro_rules! key_refuse {
    ($($method:ident($($arg:ty),*);)*) => {
        $(fn $method(self, $(_: $arg),*) -> Result<String, FormError> { Err(FormError::KeyType) })*
    };
}

macro_rules! key_to_string {
    ($($method:ident($ty:ty);)*) => {
        $(fn $method(self, value: $ty) -> Result<String, FormError> { Ok(value.to_string()) })*
    };
}

impl Serializer for KeyText {
    type Ok = String;
    type Error = FormError;
    type SerializeSeq = Impossible<String, FormError>;
    type SerializeTuple = Impossible<String, FormError>;
    type SerializeTupleStruct = Impossible<String, FormError>;
    type SerializeTupleVariant = Impossible<String, FormError>;
    type SerializeMap = Impossible<String, FormError>;
    type SerializeStruct = Impossible<String, FormError>;
    type SerializeStructVariant = Impossible<String, FormError>;

    key_to_string! {
        serialize_i8(i8); serialize_i16(i16); serialize_i32(i32); serialize_i64(i64);
        serialize_u8(u8); serialize_u16(u16); serialize_u32(u32); serialize_u64(u64);
        serialize_char(char); serialize_str(&str);
    }

    key_refuse! {
        serialize_bool(bool); serialize_f32(f32); serialize_f64(f64); serialize_bytes(&[u8]);
        serialize_none(); serialize_unit(); serialize_unit_struct(&'static str);
    }

    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<String, FormError> {
        Ok(variant.to_owned())
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<String, FormError> {
        value.serialize(self)
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<String, FormError> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<String, FormError> {
        Err(FormError::KeyType)
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, FormError> {
        Err(FormError::KeyType)
    }

    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, FormError> {
        Err(FormError::KeyType)
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, FormError> {
        Err(FormError::KeyType)
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, FormError> {
        Err(FormError::KeyType)
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, FormError> {
        Err(FormError::KeyType)
    }

    fn serialize_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, FormError> {
        Err(FormError::KeyType)
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, FormError> {
        Err(FormError::KeyType)
    }
}
