// V3-L09 / VALUE-04 (positive half): the closed default conversion set is
// identity, `&str -> String` and `&String -> String`. Unsuffixed integers
// infer the declared width, aliases are the same type, `Vec::new()` infers,
// domain newtypes are constructed explicitly, and exact `Option<T>` controls
// take a bare `None` with one target type (VALUE-03, including the nested
// `Option<u32>` payload cases of §4.3). The negative half is
// fail/capabilities/value_conversions_closed.rs.
use concord::{AtomEncoder, AtomError, Binary, BoundedAtomWriter, Bytes, RequestAtom, api};

pub type Label = String;

pub struct ItemId(pub String);

impl RequestAtom for ItemId {
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.push_str(&self.0)
    }
}

pub struct Milestone;

impl AtomEncoder<Option<u32>> for Milestone {
    fn write(value: &Option<u32>, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        match value {
            Some(number) => output.push_str(&number.to_string()),
            None => output.push_str("none"),
        }
    }
}

api! {
    client ConversionApi {
        base "http://conversions.example.test";
        values {
            query region?: String;
            query limit?: u64;
        }

        GET item(
            id: crate::ItemId,
            query label: crate::Label,
            query("tag") each tags: Vec<String>,
            header("x-milestone") milestone?: Option<u32> via crate::Milestone,
        ) -> Bytes via Binary {
            path "items/{id}";
        }
    }
}

fn main() {
    let client = ConversionApi::builder()
        .set_limit(50)
        .set_region("eu")
        .replace_region(None)
        .set_region_if_some(None)
        .build()
        .expect("client");
    let owned = String::from("label");
    let _: conversion_api::ItemRequest = client
        .item(ItemId("42".to_owned()), &owned, Vec::new())
        .set_milestone(None)
        .replace_milestone(None)
        .replace_milestone(Some(None))
        .set_milestone_if_some(None)
        .set_milestone(Some(7));
    let label: Label = "aliased".to_owned();
    let _ = client.item(ItemId("43".to_owned()), label, vec!["a".to_owned()]);
    let _ = client.item(ItemId("44".to_owned()), "literal", Vec::new());
}
