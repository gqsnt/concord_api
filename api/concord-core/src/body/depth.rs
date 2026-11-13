// Bound recursive Serde operations while forwarding the format's semantics.
use serde::ser::{self, Serialize};
use std::cell::{Cell, RefCell};

// This one-shot channel is set only by Entries, before it calls serialize_entry.
// Serde names and serialized keys never establish diagnostic provenance. Consuming
// it before serializing the key/value also isolates nested or reentrant writers.
thread_local! {
    static ENTRY_INPUT: Cell<Option<&'static str>> = const { Cell::new(None) };
}
pub(super) fn with_input<T>(input: &'static str, write: impl FnOnce() -> T) -> T {
    struct Restore(Option<&'static str>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ENTRY_INPUT.set(self.0);
        }
    }
    let _restore = Restore(ENTRY_INPUT.replace(Some(input)));
    write()
}
#[derive(Default)]
pub(super) struct Context {
    pub input: RefCell<Option<String>>,
}

pub(super) struct Value<'a, T: ?Sized>(
    pub &'a T,
    pub u16,
    pub Option<&'a Context>,
    pub Option<&'a str>,
);
impl<T: Serialize + ?Sized> Serialize for Value<'_, T> {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.1 > 128 {
            if let Some(context) = self.2 {
                *context.input.borrow_mut() = self.3.map(str::to_owned);
            }
            return Err(ser::Error::custom("value nested more than 128 levels deep"));
        }
        self.0.serialize(Writer(serializer, self.1, self.2, self.3))
    }
}
struct Writer<'a, S>(S, u16, Option<&'a Context>, Option<&'a str>);
struct Compound<'a, S>(S, u16, Option<&'a Context>, Option<&'a str>);
impl<'a, S: ser::Serializer> ser::Serializer for Writer<'a, S> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = Compound<'a, S::SerializeSeq>;
    type SerializeTuple = Compound<'a, S::SerializeTuple>;
    type SerializeTupleStruct = Compound<'a, S::SerializeTupleStruct>;
    type SerializeTupleVariant = Compound<'a, S::SerializeTupleVariant>;
    type SerializeMap = Compound<'a, S::SerializeMap>;
    type SerializeStruct = Compound<'a, S::SerializeStruct>;
    type SerializeStructVariant = Compound<'a, S::SerializeStructVariant>;
    fn serialize_bool(self, value: bool) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_bool(value)
    }
    fn serialize_i8(self, value: i8) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_i8(value)
    }
    fn serialize_i16(self, value: i16) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_i16(value)
    }
    fn serialize_i32(self, value: i32) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_i32(value)
    }
    fn serialize_i64(self, value: i64) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_i64(value)
    }
    fn serialize_i128(self, value: i128) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_i128(value)
    }
    fn serialize_u8(self, value: u8) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_u8(value)
    }
    fn serialize_u16(self, value: u16) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_u16(value)
    }
    fn serialize_u32(self, value: u32) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_u32(value)
    }
    fn serialize_u64(self, value: u64) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_u64(value)
    }
    fn serialize_u128(self, value: u128) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_u128(value)
    }
    fn serialize_f32(self, value: f32) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_f32(value)
    }
    fn serialize_f64(self, value: f64) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_f64(value)
    }
    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_char(value)
    }
    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_str(value)
    }
    fn serialize_bytes(self, value: &[u8]) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_bytes(value)
    }
    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
    fn collect_str<T: std::fmt::Display + ?Sized>(
        self,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.collect_str(value)
    }
    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_none()
    }
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit()
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit_struct(name)
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit_variant(name, index, variant)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error> {
        self.0
            // PANIC-SAFETY: Value::serialize rejects depths above 128, so this private u16 increment is at most 129.
            .serialize_some(&Value(value, self.1 + 1, self.2, self.3))
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.0
            // PANIC-SAFETY: Value::serialize rejects depths above 128, so this private u16 increment is at most 129.
            .serialize_newtype_struct(name, &Value(value, self.1 + 1, self.2, self.3))
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_newtype_variant(
            name,
            index,
            variant,
            // PANIC-SAFETY: Value::serialize rejects depths above 128, so this private u16 increment is at most 129.
            &Value(value, self.1 + 1, self.2, self.3),
        )
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        self.0
            .serialize_seq(len)
            // PANIC-SAFETY: Value::serialize rejects depths above 128, so this private u16 increment is at most 129.
            .map(|inner| Compound(inner, self.1 + 1, self.2, self.3))
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.0
            .serialize_tuple(len)
            // PANIC-SAFETY: Value::serialize rejects depths above 128, so this private u16 increment is at most 129.
            .map(|inner| Compound(inner, self.1 + 1, self.2, self.3))
    }
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.0
            .serialize_tuple_struct(name, len)
            // PANIC-SAFETY: Value::serialize rejects depths above 128, so this private u16 increment is at most 129.
            .map(|inner| Compound(inner, self.1 + 1, self.2, self.3))
    }
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        self.0
            .serialize_tuple_variant(name, index, variant, len)
            // PANIC-SAFETY: Value::serialize rejects depths above 128, so this private u16 increment is at most 129.
            .map(|inner| Compound(inner, self.1 + 1, self.2, self.3))
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        self.0
            .serialize_map(len)
            // PANIC-SAFETY: Value::serialize rejects depths above 128, so this private u16 increment is at most 129.
            .map(|inner| Compound(inner, self.1 + 1, self.2, self.3))
    }
    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.0
            .serialize_struct(name, len)
            // PANIC-SAFETY: Value::serialize rejects depths above 128, so this private u16 increment is at most 129.
            .map(|inner| Compound(inner, self.1 + 1, self.2, self.3))
    }
    fn serialize_struct_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        self.0
            .serialize_struct_variant(name, index, variant, len)
            // PANIC-SAFETY: Value::serialize rejects depths above 128, so this private u16 increment is at most 129.
            .map(|inner| Compound(inner, self.1 + 1, self.2, self.3))
    }
}
impl<S: ser::SerializeSeq> ser::SerializeSeq for Compound<'_, S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.0
            .serialize_element(&Value(value, self.1, self.2, self.3))
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}
impl<S: ser::SerializeTuple> ser::SerializeTuple for Compound<'_, S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.0
            .serialize_element(&Value(value, self.1, self.2, self.3))
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}
impl<S: ser::SerializeTupleStruct> ser::SerializeTupleStruct for Compound<'_, S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.0
            .serialize_field(&Value(value, self.1, self.2, self.3))
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}
impl<S: ser::SerializeTupleVariant> ser::SerializeTupleVariant for Compound<'_, S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.0
            .serialize_field(&Value(value, self.1, self.2, self.3))
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}
impl<S: ser::SerializeMap> ser::SerializeMap for Compound<'_, S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.0.serialize_key(&Value(value, self.1, self.2, self.3))
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.0
            .serialize_value(&Value(value, self.1, self.2, self.3))
    }
    fn serialize_entry<K: Serialize + ?Sized, V: Serialize + ?Sized>(
        &mut self,
        key: &K,
        value: &V,
    ) -> Result<(), Self::Error> {
        let input = ENTRY_INPUT.take();
        if let Some(input) = input.filter(|_| self.3.is_none()) {
            self.0
                .serialize_entry(key, &Value(value, 0, self.2, Some(input)))
        } else {
            self.0.serialize_entry(
                &Value(key, self.1, self.2, self.3),
                &Value(value, self.1, self.2, self.3),
            )
        }
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}
impl<S: ser::SerializeStruct> ser::SerializeStruct for Compound<'_, S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        self.0
            .serialize_field(key, &Value(value, self.1, self.2, self.3))
    }
    fn skip_field(&mut self, key: &'static str) -> Result<(), Self::Error> {
        self.0.skip_field(key)
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}
impl<S: ser::SerializeStructVariant> ser::SerializeStructVariant for Compound<'_, S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        self.0
            .serialize_field(key, &Value(value, self.1, self.2, self.3))
    }
    fn skip_field(&mut self, key: &'static str) -> Result<(), Self::Error> {
        self.0.skip_field(key)
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}
