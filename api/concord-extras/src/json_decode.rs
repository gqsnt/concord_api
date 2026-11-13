//! JSON decoding, kept separate from the engine so diagnostics can be tested directly.

use concord_core::read::Place;
use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use std::cell::RefCell;

pub(super) struct Failure {
    pub source: Source,
    pub details: String,
    pub place: Place,
    pub unexpected_label: bool,
}

/// A decoder cause containing only schema locations and fixed failure classes.
#[derive(Debug)]
pub(super) struct Source {
    place: Place,
    kind: serde_json::error::Category,
    line: usize,
    column: usize,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let place = self.place.to_string();
        write!(
            f,
            "JSON {:?} failure at {} (line {}, column {})",
            self.kind,
            if place.is_empty() { "[root]" } else { &place },
            self.line,
            self.column
        )
    }
}
impl std::error::Error for Source {}

impl Failure {
    fn new(error: serde_json::Error, place: Place, label: Option<&str>) -> Self {
        let source = Source {
            place: place.clone(),
            kind: error.classify(),
            line: error.line(),
            column: error.column(),
        };
        let details = concord_core::read::details(error.to_string());
        Self {
            source,
            details,
            place,
            unexpected_label: !is_json(label),
        }
    }
}

impl std::fmt::Debug for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Failure")
            .field("place", &self.place)
            .field("unexpected_label", &self.unexpected_label)
            .finish_non_exhaustive()
    }
}

pub(super) fn is_json(label: Option<&str>) -> bool {
    let kind = label.unwrap_or("").split(';').next().unwrap_or("").trim();
    kind.eq_ignore_ascii_case("application/json") || kind.to_ascii_lowercase().ends_with("+json")
}

pub(super) fn decode<T: de::DeserializeOwned>(
    bytes: &[u8],
    label: Option<&str>,
) -> Result<T, Failure> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let mut track = serde_path_to_error::Track::new();
    let fields = RefCell::new(Vec::new());
    let result = T::deserialize(Fields {
        inner: serde_path_to_error::Deserializer::new(&mut decoder, &mut track),
        fields: &fields,
        depth: 0,
    });
    match result {
        Ok(value) => decoder
            .end()
            .map(|()| value)
            .map_err(|source| Failure::new(source, Place::root(), label)),
        Err(source) => {
            let mut place = Place::root();
            for (depth, segment) in track.path().iter().enumerate() {
                use serde_path_to_error::Segment;
                place = match segment {
                    Segment::Seq { index } => place.index(*index),
                    Segment::Map { key } => {
                        let declared = fields.borrow().get(depth).copied().flatten();
                        if let Some(name) =
                            declared.and_then(|names| names.iter().find(|&&name| name == key))
                        {
                            place.field(name)
                        } else {
                            place.key()
                        }
                    }
                    // A tag accepted by #[serde(other)] can be arbitrary response text.
                    Segment::Enum { .. } => place.variant(),
                    Segment::Unknown => place.key(),
                };
            }
            Err(Failure::new(source, place, label))
        }
    }
}

// serde_path_to_error calls both struct fields and data keys `Map`. Record the
// declared fields at each depth while delegating decoding, so data keys never
// become part of the reported place, even when a key is also a field name.
type FieldStack = RefCell<Vec<Option<&'static [&'static str]>>>;
struct Fields<'a, D> {
    inner: D,
    fields: &'a FieldStack,
    depth: usize,
}
struct Seen<'a, V> {
    inner: V,
    fields: &'a FieldStack,
    depth: usize,
    declared: Option<&'static [&'static str]>,
}
struct Seed<'a, S> {
    inner: S,
    fields: &'a FieldStack,
    depth: usize,
}

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for Seed<'_, S> {
    type Value = S::Value;
    fn deserialize<D: de::Deserializer<'de>>(self, inner: D) -> Result<Self::Value, D::Error> {
        self.inner.deserialize(Fields {
            inner,
            fields: self.fields,
            depth: self.depth,
        })
    }
}

macro_rules! forward {
    ($($method:ident $(($($arg:ident: $ty:ty),*))?),* $(,)?) => {$(
        fn $method<V: Visitor<'de>>(self, $($($arg: $ty,)*)? visitor: V) -> Result<V::Value, Self::Error> {
            self.inner.$method($($($arg,)*)? Seen { inner: visitor, fields: self.fields, depth: self.depth, declared: None })
        }
    )*};
}

impl<'de, D: de::Deserializer<'de>> de::Deserializer<'de> for Fields<'_, D> {
    type Error = D::Error;
    forward! {
        deserialize_any, deserialize_bool, deserialize_i8, deserialize_i16, deserialize_i32,
        deserialize_i64, deserialize_i128, deserialize_u8, deserialize_u16, deserialize_u32,
        deserialize_u64, deserialize_u128, deserialize_f32, deserialize_f64, deserialize_char,
        deserialize_str, deserialize_string, deserialize_bytes, deserialize_byte_buf,
        deserialize_option, deserialize_unit, deserialize_unit_struct(name: &'static str),
        deserialize_newtype_struct(name: &'static str), deserialize_seq,
        deserialize_tuple(len: usize), deserialize_tuple_struct(name: &'static str, len: usize),
        deserialize_map, deserialize_enum(name: &'static str, variants: &'static [&'static str]),
        deserialize_identifier, deserialize_ignored_any,
    }
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        declared: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.inner.deserialize_struct(
            name,
            declared,
            Seen {
                inner: visitor,
                fields: self.fields,
                depth: self.depth,
                declared: Some(declared),
            },
        )
    }
    fn is_human_readable(&self) -> bool {
        self.inner.is_human_readable()
    }
}

macro_rules! scalar {
    ($($method:ident($ty:ty)),* $(,)?) => {$(
        fn $method<E: de::Error>(self, value: $ty) -> Result<Self::Value, E> { self.inner.$method(value) }
    )*};
}

impl<'de, V: Visitor<'de>> Visitor<'de> for Seen<'_, V> {
    type Value = V::Value;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.expecting(f)
    }
    scalar! {
        visit_bool(bool), visit_i8(i8), visit_i16(i16), visit_i32(i32), visit_i64(i64), visit_i128(i128),
        visit_u8(u8), visit_u16(u16), visit_u32(u32), visit_u64(u64), visit_u128(u128),
        visit_f32(f32), visit_f64(f64), visit_char(char), visit_str(&str), visit_borrowed_str(&'de str),
        visit_string(String), visit_bytes(&[u8]), visit_borrowed_bytes(&'de [u8]), visit_byte_buf(Vec<u8>),
    }
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_none()
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_unit()
    }
    fn visit_some<D: de::Deserializer<'de>>(self, inner: D) -> Result<Self::Value, D::Error> {
        self.inner.visit_some(Fields {
            inner,
            fields: self.fields,
            depth: self.depth,
        })
    }
    fn visit_newtype_struct<D: de::Deserializer<'de>>(
        self,
        inner: D,
    ) -> Result<Self::Value, D::Error> {
        self.inner.visit_newtype_struct(Fields {
            inner,
            fields: self.fields,
            depth: self.depth,
        })
    }
    fn visit_seq<A: SeqAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        self.inner.visit_seq(Fields {
            inner,
            fields: self.fields,
            depth: self.depth,
        })
    }
    fn visit_map<A: MapAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        let mut fields = self.fields.borrow_mut();
        fields.resize(self.depth, None);
        fields.push(self.declared);
        drop(fields);
        self.inner.visit_map(Fields {
            inner,
            fields: self.fields,
            depth: self.depth,
        })
    }
    fn visit_enum<A: EnumAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        self.inner.visit_enum(Fields {
            inner,
            fields: self.fields,
            depth: self.depth,
        })
    }
}

impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for Fields<'_, A> {
    type Error = A::Error;
    fn next_element_seed<S: DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> Result<Option<S::Value>, A::Error> {
        self.inner.next_element_seed(Seed {
            inner: seed,
            fields: self.fields,
            // PANIC-SAFETY: Serde JSON limits input-driven container recursion to 128 before this depth can grow.
            depth: self.depth + 1,
        })
    }
    fn size_hint(&self) -> Option<usize> {
        self.inner.size_hint()
    }
}
impl<'de, A: MapAccess<'de>> MapAccess<'de> for Fields<'_, A> {
    type Error = A::Error;
    fn next_key_seed<S: DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> Result<Option<S::Value>, A::Error> {
        self.inner.next_key_seed(seed)
    }
    fn next_value_seed<S: DeserializeSeed<'de>>(&mut self, seed: S) -> Result<S::Value, A::Error> {
        self.inner.next_value_seed(Seed {
            inner: seed,
            fields: self.fields,
            // PANIC-SAFETY: Serde JSON limits input-driven container recursion to 128 before this depth can grow.
            depth: self.depth + 1,
        })
    }
    fn size_hint(&self) -> Option<usize> {
        self.inner.size_hint()
    }
}
impl<'a, 'de, A: EnumAccess<'de>> EnumAccess<'de> for Fields<'a, A> {
    type Error = A::Error;
    type Variant = Fields<'a, A::Variant>;
    fn variant_seed<S: DeserializeSeed<'de>>(
        self,
        seed: S,
    ) -> Result<(S::Value, Self::Variant), A::Error> {
        let (variant, inner) = self.inner.variant_seed(seed)?;
        Ok((
            variant,
            Fields {
                inner,
                fields: self.fields,
                // PANIC-SAFETY: Serde JSON limits input-driven container recursion to 128 before this depth can grow.
                depth: self.depth + 1,
            },
        ))
    }
}

impl<'de, A: VariantAccess<'de>> VariantAccess<'de> for Fields<'_, A> {
    type Error = A::Error;
    fn unit_variant(self) -> Result<(), A::Error> {
        self.inner.unit_variant()
    }
    fn newtype_variant_seed<S: DeserializeSeed<'de>>(self, seed: S) -> Result<S::Value, A::Error> {
        self.inner.newtype_variant_seed(Seed {
            inner: seed,
            fields: self.fields,
            depth: self.depth,
        })
    }
    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, A::Error> {
        self.inner.tuple_variant(
            len,
            Seen {
                inner: visitor,
                fields: self.fields,
                depth: self.depth,
                declared: None,
            },
        )
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        declared: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, A::Error> {
        self.inner.struct_variant(
            declared,
            Seen {
                inner: visitor,
                fields: self.fields,
                depth: self.depth,
                declared: Some(declared),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct List {
        items: Vec<Item>,
    }
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct Item {
        kind: u32,
    }

    #[test]
    fn skips_a_utf8_byte_order_mark() {
        let got: List = decode(
            b"\xef\xbb\xbf{\"items\":[{\"kind\":7}]}",
            Some("application/json"),
        )
        .unwrap();
        assert_eq!(got.items[0].kind, 7);
    }

    #[test]
    fn a_decode_failure_names_fields_and_indexes() {
        let error = decode::<List>(
            br#"{"items":[{"kind":1},{"kind":"bad"}]}"#,
            Some("application/json"),
        )
        .unwrap_err();
        assert_eq!(error.place.to_string(), "items[1].kind");
        assert!(!error.unexpected_label);
    }

    #[test]
    fn a_data_key_is_redacted_even_when_it_matches_a_field_name() {
        #[derive(Debug, serde::Deserialize)]
        #[allow(dead_code)]
        struct Maps {
            items: BTreeMap<String, Item>,
        }
        for key in ["private-key", "items", "kind"] {
            let input = format!(r#"{{"items":{{"{key}":{{"kind":"bad"}}}}}}"#);
            let error = decode::<Maps>(input.as_bytes(), Some("application/json")).unwrap_err();
            assert_eq!(error.place.to_string(), "items[key].kind");
        }
    }

    #[test]
    fn paths_pass_through_options_newtypes_and_enum_variants() {
        #[derive(Debug, serde::Deserialize)]
        #[allow(dead_code)]
        struct Wrapped(Option<BTreeMap<String, Shape>>);
        #[derive(Debug, serde::Deserialize)]
        #[allow(dead_code)]
        enum Shape {
            Named { kind: u32 },
            Tuple(u32, Item),
            Newtype(Item),
        }
        for (input, expected) in [
            (
                r#"{"private":{"Named":{"kind":"bad"}}}"#,
                "[key][variant].kind",
            ),
            (
                r#"{"private":{"Tuple":[1,{"kind":"bad"}]}}"#,
                "[key][variant][1].kind",
            ),
            (
                r#"{"private":{"Newtype":{"kind":"bad"}}}"#,
                "[key][variant].kind",
            ),
        ] {
            assert_eq!(
                decode::<Wrapped>(input.as_bytes(), Some("application/json"))
                    .unwrap_err()
                    .place
                    .to_string(),
                expected
            );
        }
    }

    #[test]
    fn an_undeclared_enum_tag_never_appears_in_the_failure_place() {
        #[derive(Debug, serde::Deserialize)]
        #[allow(dead_code)]
        struct Tagged {
            value: Tag,
        }
        #[derive(Debug, serde::Deserialize)]
        enum Tag {
            Known,
            #[serde(other)]
            Unknown,
        }
        let error = decode::<Tagged>(
            br#"{"value":{"PRIVATE_VARIANT":5}}"#,
            Some("application/json"),
        )
        .unwrap_err();
        assert_eq!(error.place.to_string(), "value[variant]");
        assert!(!format!("{error:?}").contains("PRIVATE_VARIANT"));
        assert!(!format!("{error:#?}").contains("PRIVATE_VARIANT"));
    }

    #[test]
    fn a_failed_decode_says_when_the_label_was_not_json() {
        assert_eq!(decode::<u32>(b"7", Some("text/plain")).unwrap(), 7);
        for label in [None, Some("text/plain")] {
            let error = decode::<u32>(b"bad", label).unwrap_err();
            assert!(error.unexpected_label);
        }
        for label in [
            "application/json",
            "Application/Problem+JSON; charset=utf-8",
        ] {
            assert!(
                !decode::<u32>(b"bad", Some(label))
                    .unwrap_err()
                    .unexpected_label
            );
        }
    }

    #[test]
    fn a_large_value_is_bounded_in_details_and_absent_from_the_source() {
        let value = "PRIVATE_VALUE".repeat(2 * 1024 * 1024 / 13 + 1);
        let input = format!(r#"{{"items":[{{"kind":"{value}"}}]}}"#);
        assert!(input.len() > 2_000_000);
        let error = decode::<List>(input.as_bytes(), Some("application/json")).unwrap_err();
        assert_eq!(error.place.to_string(), "items[0].kind");
        assert!(error.details.len() <= 1024 + "… [cut]".len());
        assert!(error.details.capacity() < 2048);
        assert!(error.details.ends_with("… [cut]"));
        assert!(error.details.contains("PRIVATE_VALUE"));
        assert!(error.source.to_string().contains("items[0].kind"));
        assert!(error.source.to_string().contains("Data"));
        assert!(error.source.to_string().contains("line 1, column"));
        assert!(!error.source.to_string().contains("PRIVATE_VALUE"));
        assert!(!format!("{:?}", error.source).contains("PRIVATE_VALUE"));
        assert!(std::error::Error::source(&error.source).is_none());
    }

    #[test]
    fn trailing_data_is_still_rejected() {
        assert!(decode::<u32>(b"7 8", Some("application/json")).is_err());
    }
}
