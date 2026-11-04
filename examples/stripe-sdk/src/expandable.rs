//! Expandable fields and the list envelope.
//!
//! An expandable field ("expandable" in Stripe's reference) is an object id by default and the
//! full object when the request names it in `expand[]` (`customer`, or `data.customer` on a
//! list). [`Expandable<T>`] keeps both forms typed; [`Expandable::id`] works in either.

use std::fmt;
use std::marker::PhantomData;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

/// A Stripe object with a typed identifier.
pub trait Object {
    /// The identifier type, such as [`CustomerId`](crate::CustomerId).
    type Id: Clone + PartialEq + fmt::Display + fmt::Debug;

    /// The object's id.
    fn id(&self) -> &Self::Id;
}

/// An expandable field: the related object's id, or the object itself when it was expanded.
#[derive(Clone, Debug, PartialEq)]
pub enum Expandable<T: Object> {
    /// Not expanded: the id.
    Id(T::Id),
    /// Expanded: the object.
    Object(Box<T>),
}

impl<T: Object> Expandable<T> {
    /// The related object's id, in either form.
    pub fn id(&self) -> &T::Id {
        match self {
            Self::Id(id) => id,
            Self::Object(object) => object.id(),
        }
    }

    /// The expanded object, if the field was expanded.
    pub fn object(&self) -> Option<&T> {
        match self {
            Self::Id(_) => None,
            Self::Object(object) => Some(object),
        }
    }

    /// Whether the field was expanded.
    pub fn is_expanded(&self) -> bool {
        matches!(self, Self::Object(_))
    }
}

impl<'de, T> Deserialize<'de> for Expandable<T>
where
    T: Object + Deserialize<'de>,
    T::Id: Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ExpandableVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for ExpandableVisitor<T>
        where
            T: Object + Deserialize<'de>,
            T::Id: Deserialize<'de>,
        {
            type Value = Expandable<T>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object id or an expanded object")
            }

            fn visit_str<E: de::Error>(self, id: &str) -> Result<Self::Value, E> {
                T::Id::deserialize(de::value::StrDeserializer::<E>::new(id)).map(Expandable::Id)
            }

            fn visit_string<E: de::Error>(self, id: String) -> Result<Self::Value, E> {
                T::Id::deserialize(de::value::StringDeserializer::<E>::new(id)).map(Expandable::Id)
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                T::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(|object| Expandable::Object(Box::new(object)))
            }
        }

        deserializer.deserialize_any(ExpandableVisitor(PhantomData))
    }
}

/// The list envelope every v1 list operation returns: `{"object": "list", "data": [..],
/// "has_more": .., "url": ".."}`. Decoding fails unless `object` is `"list"`.
///
/// Consistency between `has_more` and `data` (a non-terminal page must name a next cursor) is
/// judged by the cursor adapter ([`crate::pages`]), not here: an embedded list (an invoice's
/// `lines`) is decoded with this type too.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct List<T> {
    #[serde(rename = "object")]
    _object: ListTag,
    /// The page's objects, in Stripe's order (reverse chronological for top-level lists).
    pub data: Vec<T>,
    /// Whether more objects exist after this page in the direction requested.
    pub has_more: bool,
    /// The list's path, such as `/v1/customers`.
    pub url: String,
    /// The total, present on some embedded lists (`lines`, `items`).
    #[serde(default)]
    pub total_count: Option<u64>,
}

impl<T> List<T> {
    /// A list value, for tests and adapters that build pages.
    pub fn new(data: Vec<T>, has_more: bool, url: impl Into<String>) -> Self {
        Self {
            _object: ListTag,
            data,
            has_more,
            url: url.into(),
            total_count: None,
        }
    }
}

/// The `"object": "list"` marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ListTag;

impl<'de> Deserialize<'de> for ListTag {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let tag = String::deserialize(deserializer)?;
        if tag == "list" {
            Ok(ListTag)
        } else {
            Err(de::Error::custom(
                "a list envelope must have `object: \"list\"`",
            ))
        }
    }
}
