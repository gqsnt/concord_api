//! Forms and query pairs with bracketed map keys and indexed sequences.
use concord_core::body::{Bounded, WriteError};
use concord_core::Body;
use serde::ser::{self, Serialize};
use std::{error::Error, fmt};

/// Serializes nested maps and lists as an `application/x-www-form-urlencoded` body.
///
/// Maps use `a[b]`; every list uses `a[0]`, `a[1]`, including lists of scalars.
/// An omitted element does not renumber the others: each keeps its index in the input.
/// Unit enum variants use their Serde names,
/// including renames; variants carrying data are rejected.
/// Rack and Rails read `a[0]=x` as a map with the key `"0"`, so those APIs need
/// their own writer. Keys must serialize as nonempty strings without brackets.
/// `None`, unit and empty collections emit no pairs. Newtype structs are transparent.
/// Map and struct traversal order is preserved. Keys and values are form-encoded
/// after nested keys are built; spaces become `+`.
/// Booleans are `true` or `false`, floats use Rust's `Display` representation
/// (for example, `1.5`, `2`, `-0`, `NaN` or `inf`), and percent-escapes use
/// uppercase hexadecimal digits.
/// A root that emits pairs must be a map or struct, optionally wrapped in options
/// or newtypes; an absent root writes nothing.
///
/// Uses the same 128-level nesting guard ([`Bounded`]) as [`super::form`].
/// Memory holds the pairs and encoded form, proportional to the input and output;
/// there is no byte or traversal limit. It builds bytes synchronously, sends
/// nothing and creates no task. The body can be sent again by the engine.
/// A description's `content-type` header takes precedence.
///
/// ```no_run
/// use std::collections::BTreeMap;
/// concord::api! {
///     client Customers {
///         base "https://api.example.test";
///         POST create(metadata: BTreeMap<String, String>, expand?: Vec<String>) {
///             body concord::bracket_form { metadata; expand; }
///         }
///     }
/// }
/// async fn create() -> Result<(), concord::Error> {
///     Customers::new().create(BTreeMap::from([("order".into(), "7781".into())]))
///         .expand(["customer"]).await?;
///     Ok(())
/// }
/// ```
///
/// # Errors
///
/// Returns [`WriteError`] for an unsupported shape or serialization failure.
/// Its source is [`BracketFormError::Shape`] or the original format error.
/// As with `json` and `form`, only depth failures retain an entry's name.
/// Other failures retain their cause.
pub fn bracket_form<T: Serialize>(value: T) -> Result<Body, WriteError> {
    let pairs = bracket_pairs(value)?;
    // Pairs of strings always have a form representation. Keep any format error
    // through the same public writer error used by the flat form.
    super::form(pairs)
}

/// Serializes nested maps and lists into ordered, unencoded key/value pairs.
///
/// Uses the same convention and depth guard as [`bracket_form`], which is built
/// on these pairs. It performs no URL encoding and sends no request. Fixed query
/// keys can be written literally in a description. Runtime keys can be appended
/// through a request edit; the URL library encodes them exactly once.
///
/// ```no_run
/// concord::api! {
///     client Search {
///         base "https://search.example.test";
///         GET find() { query { "page" = 1; } }
///     }
/// }
/// async fn search(filter: std::collections::BTreeMap<String, String>)
///     -> Result<(), Box<dyn std::error::Error + Send + Sync>>
/// {
///     let pairs = concord::bracket_pairs(&filter)?;
///     Search::new().find().edit_request(move |request| {
///         request.url_mut().query_pairs_mut().extend_pairs(&pairs);
///     }).await?;
///     Ok(())
/// }
/// ```
///
/// # Errors
///
/// Returns the same serialization and shape failures as [`bracket_form`].
pub fn bracket_pairs<T: Serialize>(value: T) -> Result<Vec<(String, String)>, WriteError> {
    let value = Bounded::new(&value);
    let mut pairs = Vec::new();
    value
        .serialize(Writer {
            pairs: &mut pairs,
            key: None,
        })
        .map_err(|error| match error {
            FormatError::Shape => value.error(BracketFormError::Shape),
            FormatError::Custom(source) => value.error(source),
        })?;
    Ok(pairs)
}

/// A shape that bracketed form keys cannot represent.
///
/// It has no lower-level cause. Formatting prints no key, value or Serde type name.
/// A failure of the serializer itself remains instead the format cause of `WriteError`.
///
/// ```no_run
/// fn inspect(error: &concord::Error) {
///     if let Some(concord::BracketFormError::Shape) = concord::BracketFormError::find(error) {
///         // Change the input shape or use the provider's own writer.
///     }
/// }
/// ```
#[non_exhaustive]
#[derive(Debug)]
pub enum BracketFormError {
    /// A root, key or payload-bearing variant has an unsupported Serde shape.
    Shape,
}
impl BracketFormError {
    /// Finds a `BracketFormError` in this error or its chain of sources.
    ///
    /// ```no_run
    /// fn inspect(error: &concord::Error) {
    ///     let _cause = concord::BracketFormError::find(error);
    /// }
    /// ```
    pub fn find<'a>(error: &'a (dyn Error + 'static)) -> Option<&'a Self> {
        super::find_error(error)
    }
}
impl fmt::Display for BracketFormError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the bracketed form has an unsupported shape")
    }
}
impl Error for BracketFormError {}

// Keep Serde's format failure behind WriteError, not in a public signature.
#[derive(Debug)]
enum FormatError {
    Shape,
    Custom(serde_urlencoded::ser::Error),
}
impl ser::Error for FormatError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self::Custom(ser::Error::custom(message))
    }
}
impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the bracketed form could not be serialized")
    }
}
impl Error for FormatError {}

struct Writer<'a> {
    pairs: &'a mut Vec<(String, String)>,
    key: Option<String>,
}
impl Writer<'_> {
    fn scalar(self, value: impl ToString) -> Result<(), FormatError> {
        let key = self.key.ok_or(FormatError::Shape)?;
        self.pairs.push((key, value.to_string()));
        Ok(())
    }
}
impl<'a> ser::Serializer for Writer<'a> {
    type Ok = ();
    type Error = FormatError;
    type SerializeSeq = Sequence<'a>;
    type SerializeTuple = Sequence<'a>;
    type SerializeTupleStruct = Sequence<'a>;
    type SerializeTupleVariant = ser::Impossible<(), FormatError>;
    type SerializeMap = Mapping<'a>;
    type SerializeStruct = Mapping<'a>;
    type SerializeStructVariant = ser::Impossible<(), FormatError>;
    fn serialize_bool(self, value: bool) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_i8(self, value: i8) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_i16(self, value: i16) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_i32(self, value: i32) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_i64(self, value: i64) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_i128(self, value: i128) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_u8(self, value: u8) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_u16(self, value: u16) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_u32(self, value: u32) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_u64(self, value: u64) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_u128(self, value: u128) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_f32(self, value: f32) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_f64(self, value: f64) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_char(self, value: char) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_str(self, value: &str) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn collect_str<T: fmt::Display + ?Sized>(self, value: &T) -> Result<(), FormatError> {
        self.scalar(value)
    }
    fn serialize_bytes(self, value: &[u8]) -> Result<(), FormatError> {
        use ser::SerializeSeq;
        let mut sequence = self.serialize_seq(Some(value.len()))?;
        for byte in value {
            sequence.serialize_element(byte)?;
        }
        sequence.end()
    }
    fn serialize_none(self) -> Result<(), FormatError> {
        Ok(())
    }
    fn serialize_unit(self) -> Result<(), FormatError> {
        Ok(())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), FormatError> {
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), FormatError> {
        value.serialize(self)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), FormatError> {
        value.serialize(self)
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<(), FormatError> {
        self.scalar(variant)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<(), FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, FormatError> {
        Ok(Sequence {
            pairs: self.pairs,
            key: self.key.ok_or(FormatError::Shape)?,
            index: 0,
        })
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, FormatError> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, FormatError> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, FormatError> {
        Ok(Mapping {
            pairs: self.pairs,
            prefix: self.key,
            pending: None,
        })
    }
    fn serialize_struct(
        self,
        _: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, FormatError> {
        self.serialize_map(Some(len))
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, FormatError> {
        Err(FormatError::Shape)
    }
}
struct Sequence<'a> {
    pairs: &'a mut Vec<(String, String)>,
    key: String,
    index: usize,
}
impl Sequence<'_> {
    fn element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), FormatError> {
        let key = format!("{}[{}]", self.key, self.index);
        self.index = self.index.saturating_add(1);
        value.serialize(Writer {
            pairs: self.pairs,
            key: Some(key),
        })
    }
}
impl ser::SerializeSeq for Sequence<'_> {
    type Ok = ();
    type Error = FormatError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), FormatError> {
        self.element(value)
    }
    fn end(self) -> Result<(), FormatError> {
        Ok(())
    }
}
impl ser::SerializeTuple for Sequence<'_> {
    type Ok = ();
    type Error = FormatError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), FormatError> {
        self.element(value)
    }
    fn end(self) -> Result<(), FormatError> {
        Ok(())
    }
}
impl ser::SerializeTupleStruct for Sequence<'_> {
    type Ok = ();
    type Error = FormatError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), FormatError> {
        self.element(value)
    }
    fn end(self) -> Result<(), FormatError> {
        Ok(())
    }
}
struct Mapping<'a> {
    pairs: &'a mut Vec<(String, String)>,
    prefix: Option<String>,
    pending: Option<String>,
}
impl Mapping<'_> {
    fn path(&self, key: String) -> String {
        match &self.prefix {
            Some(prefix) => format!("{prefix}[{key}]"),
            None => key,
        }
    }
    fn entry<T: Serialize + ?Sized>(&mut self, key: String, value: &T) -> Result<(), FormatError> {
        let key = self.path(key);
        value.serialize(Writer {
            pairs: self.pairs,
            key: Some(key),
        })
    }
}
impl ser::SerializeMap for Mapping<'_> {
    type Ok = ();
    type Error = FormatError;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), FormatError> {
        if self.pending.is_some() {
            return Err(FormatError::Shape);
        }
        self.pending = Some(key.serialize(Key)?);
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), FormatError> {
        let key = self.pending.take().ok_or(FormatError::Shape)?;
        self.entry(key, value)
    }
    fn serialize_entry<K: Serialize + ?Sized, V: Serialize + ?Sized>(
        &mut self,
        key: &K,
        value: &V,
    ) -> Result<(), FormatError> {
        if self.pending.is_some() {
            return Err(FormatError::Shape);
        }
        self.entry(key.serialize(Key)?, value)
    }
    fn end(self) -> Result<(), FormatError> {
        if self.pending.is_some() {
            Err(FormatError::Shape)
        } else {
            Ok(())
        }
    }
}
impl ser::SerializeStruct for Mapping<'_> {
    type Ok = ();
    type Error = FormatError;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), FormatError> {
        self.entry(valid_key(key)?, value)
    }
    fn end(self) -> Result<(), FormatError> {
        Ok(())
    }
}
fn valid_key(key: &str) -> Result<String, FormatError> {
    if key.is_empty() || key.contains(['[', ']']) {
        Err(FormatError::Shape)
    } else {
        Ok(key.to_owned())
    }
}
struct Key;
impl ser::Serializer for Key {
    type Ok = String;
    type Error = FormatError;
    type SerializeSeq = ser::Impossible<String, FormatError>;
    type SerializeTuple = ser::Impossible<String, FormatError>;
    type SerializeTupleStruct = ser::Impossible<String, FormatError>;
    type SerializeTupleVariant = ser::Impossible<String, FormatError>;
    type SerializeMap = ser::Impossible<String, FormatError>;
    type SerializeStruct = ser::Impossible<String, FormatError>;
    type SerializeStructVariant = ser::Impossible<String, FormatError>;
    fn serialize_str(self, value: &str) -> Result<String, FormatError> {
        valid_key(value)
    }
    fn collect_str<T: fmt::Display + ?Sized>(self, value: &T) -> Result<String, FormatError> {
        valid_key(&value.to_string())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<String, FormatError> {
        value.serialize(self)
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<String, FormatError> {
        valid_key(variant)
    }
    fn serialize_bool(self, _: bool) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_i8(self, _: i8) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_i16(self, _: i16) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_i32(self, _: i32) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_i64(self, _: i64) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_i128(self, _: i128) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_u8(self, _: u8) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_u16(self, _: u16) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_u32(self, _: u32) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_u64(self, _: u64) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_u128(self, _: u128) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_f32(self, _: f32) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_f64(self, _: f64) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_char(self, _: char) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_none(self) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_unit(self) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<String, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, FormatError> {
        Err(FormatError::Shape)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, FormatError> {
        Err(FormatError::Shape)
    }
}
