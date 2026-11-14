//! JSON and form writers, and an optional multipart writer, producing a `concord_core::Body`.

#[cfg(any(feature = "json", feature = "form"))]
use concord_core::Body;

/// Serializes the value as JSON, with the content type `application/json`: `body json(task);`.
///
/// A secret can be placed through a body entry's adapter, but cannot be serialized
/// inside an arbitrary struct:
///
/// ```compile_fail
/// # mod concord { pub use concord_core::Secret; pub use concord_extras::body::json; }
/// #[derive(serde::Serialize)]
/// struct Login { client_secret: concord::Secret }
/// let _ = concord::json(Login {
///     client_secret: concord::Secret::new("key"),
/// });
/// ```
///
/// # Errors
///
/// Returns an error when `serde` cannot serialize the value; the call then fails with a Build
/// error. [`concord_core::body::WriteError`] keeps the format's error as its source.
/// Values nested more than 128 levels deep
/// are rejected. An entry's depth failure keeps its input name in the Build error.
#[cfg(feature = "json")]
pub fn json<T: serde::Serialize>(value: T) -> Result<Body, concord_core::body::WriteError> {
    let value = concord_core::body::Bounded::new(&value);
    let bytes = serde_json::to_vec(&value).map_err(|error| value.error(error))?;
    Ok(Body::new(bytes, "application/json"))
}

/// Serializes the value as a form, with the content type `application/x-www-form-urlencoded`:
/// `body form(login);`.
///
/// A form is flat: pairs of strings. An inner `None` omits the pair too, because forms have no
/// `null`. A list or a struct as an entry value is an error.
/// A nested form is the convention of a particular API, and calls for your own writer.
///
/// # Errors
///
/// Returns an error when the value is not a flat set of pairs, or nests more than 128 levels.
/// [`concord_core::body::WriteError`] keeps the original form error as its source
/// and gives an entry's depth failure its input name in the Build error.
#[cfg(feature = "form")]
pub fn form<T: serde::Serialize>(value: T) -> Result<Body, concord_core::body::WriteError> {
    let value = concord_core::body::Bounded::new(&value);
    let text = serde_urlencoded::to_string(&value).map_err(|error| value.error(error))?;
    Ok(Body::new(text, "application/x-www-form-urlencoded"))
}

#[cfg(feature = "multipart")]
#[path = "multipart.rs"]
mod multipart_writer;
#[cfg(feature = "multipart")]
pub use multipart_writer::{multipart, Multipart, MultipartError};

#[cfg(test)]
mod tests {
    use concord_core::body::Entries;

    #[cfg(feature = "json")]
    #[test]
    fn entries_json_omits_unset_inputs_and_keeps_null_and_nested_values() {
        #[derive(serde::Serialize)]
        struct Nested {
            title: &'static str,
            rows: Vec<u64>,
        }
        let nested = Nested {
            title: "nested",
            rows: vec![1, 2],
        };
        let mut entries = Entries::new();
        entries.__optional("unset", None::<u32>);
        entries.__optional("present_null", Some(None::<u64>));
        entries.__push("value_null", None::<String>);
        entries.__push("nested", &nested);
        let body = super::json(entries).unwrap();
        assert_eq!(body.__content_type(), Some("application/json"));
        assert_eq!(
            body.__bytes().unwrap(),
            br#"{"present_null":null,"nested":{"title":"nested","rows":[1,2]}}"#
        );
    }

    #[cfg(feature = "form")]
    #[test]
    fn entries_form_omits_unset_inputs_and_keeps_entry_order() {
        let mut entries = Entries::new();
        entries.__push("z", "hello there");
        entries.__optional("unset", None::<u32>);
        entries.__optional("count", Some(7));
        entries.__push("a", true);
        let body = super::form(entries).unwrap();
        assert_eq!(
            body.__content_type(),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(body.__bytes().unwrap(), b"z=hello+there&count=7&a=true");
    }

    #[cfg(feature = "form")]
    #[test]
    fn form_preserves_serdes_none_behavior_and_rejects_nested_values() {
        #[derive(serde::Serialize)]
        struct Nested {
            count: u64,
        }
        let mut null = Entries::new();
        null.__push("null", None::<u64>);
        assert_eq!(super::form(null).unwrap().__bytes(), Some(b"".as_slice()));
        let mut nested = Entries::new();
        nested.__push("nested", Nested { count: 1 });
        assert!(
            super::form(nested).is_err(),
            "a flat form has no nested map"
        );
    }

    #[cfg(feature = "json")]
    #[test]
    fn none_presence_probe_must_not_format_collect_str_values() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };

        // The formatter must run only when writing the body.
        struct Text(Arc<AtomicUsize>);
        impl std::fmt::Display for Text {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let count = self.0.fetch_add(1, Ordering::SeqCst) + 1;
                write!(f, "value-{count}")
            }
        }
        impl serde::Serialize for Text {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }
        let count = Arc::new(AtomicUsize::new(0));
        let mut entries = Entries::new();
        entries.__push("text", Text(count.clone()));
        let after_insert = count.load(Ordering::SeqCst);
        let body = super::json(entries).unwrap();
        assert_eq!(after_insert, 0, "presence probe formatted the value");
        assert_eq!(body.as_bytes().unwrap(), br#"{"text":"value-1"}"#);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[cfg(feature = "json")]
    #[test]
    fn entries_debug_never_formats_keys_or_values() {
        let mut entries = Entries::new();
        entries.__push("private-name", "private-value");
        assert_eq!(format!("{entries:?}"), "Entries { len: 1 }");
    }
}

#[cfg(test)]
mod entry_regressions {
    use super::*;
    use concord_core::{
        __private::{AdaptValue as _, ValueRef},
        body::Entries,
        Secret,
    };

    #[cfg(feature = "json")]
    #[test]
    // The reference is intentional: omission must work for a borrowed None too.
    #[allow(clippy::needless_borrows_for_generic_args)]
    fn json_omits_top_level_none_by_serde_operation_and_preserves_order() {
        struct Nothing;
        impl serde::Serialize for Nothing {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_none()
            }
        }
        let x = None::<u64>;
        let mut e = Entries::new();
        e.__push("first", 1);
        e.__push("x", x);
        e.__push("borrowed", &x);
        e.__push("optional_borrow", None::<&u64>);
        e.__push("custom", Nothing);
        e.__push("k", Some(None::<u64>));
        e.__push("unit", ());
        e.__push("last", 2);
        assert_eq!(
            json(e).unwrap().as_bytes().unwrap(),
            br#"{"first":1,"k":null,"unit":null,"last":2}"#
        );
    }

    #[cfg(feature = "form")]
    #[test]
    // The reference is intentional: omission must work for a borrowed None too.
    #[allow(clippy::needless_borrows_for_generic_args)]
    fn form_omits_all_top_level_none_shapes_and_rejects_lists() {
        let x = None::<u64>;
        let mut e = Entries::new();
        e.__push("first", 1);
        e.__push("x", x);
        e.__push("borrowed", &x);
        e.__push("optional_borrow", None::<&u64>);
        e.__push("last", 2);
        assert_eq!(form(e).unwrap().as_bytes().unwrap(), b"first=1&last=2");
        let mut e = Entries::new();
        e.__push("list", vec![1, 2]);
        assert!(form(e).is_err());
    }

    #[cfg(all(feature = "json", feature = "form"))]
    #[test]
    fn optional_milestone_has_three_json_states_and_two_form_states() {
        for (milestone, json_bytes, form_bytes) in [
            (None, "{}", ""),
            (Some(None), r#"{"milestone":null}"#, ""),
            (Some(Some(5u64)), r#"{"milestone":5}"#, "milestone=5"),
        ] {
            let value = ValueRef(&milestone);
            let mut e = Entries::new();
            e.__push("milestone", (&value).adapt());
            assert_eq!(json(&e).unwrap().as_bytes().unwrap(), json_bytes.as_bytes());
            assert_eq!(form(&e).unwrap().as_bytes().unwrap(), form_bytes.as_bytes());
        }
    }

    #[cfg(all(feature = "json", feature = "form"))]
    #[test]
    fn generated_form_entries_write_secrets_and_omit_unset_secrets() {
        // body form { grant_type; client_id; client_secret; optional_secret; }
        let grant_type = ValueRef("client_credentials");
        let client_id = ValueRef("my client");
        let client_secret = ValueRef(Secret::new("a&b"));
        let optional_secret = ValueRef(None::<Secret>);
        let set_secret = Some(Secret::new("refresh key"));
        let borrowed_secret = ValueRef(&set_secret);
        let mut e = Entries::new();
        e.__push("grant_type", (&grant_type).adapt());
        e.__push("client_id", (&client_id).adapt());
        e.__push("client_secret", (&client_secret).adapt());
        e.__push("optional_secret", (&optional_secret).adapt());
        e.__push("refresh", (&borrowed_secret).adapt());
        assert_eq!(form(&e).unwrap().as_bytes().unwrap(),
            b"grant_type=client_credentials&client_id=my+client&client_secret=a%26b&refresh=refresh+key");
        assert_eq!(json(&e).unwrap().as_bytes().unwrap(),
            br#"{"grant_type":"client_credentials","client_id":"my client","client_secret":"a&b","refresh":"refresh key"}"#);
        // Debug counts stored entries; presence is deferred to the writer.
        assert_eq!(format!("{e:?}"), "Entries { len: 5 }");
    }
}

#[cfg(test)]
#[cfg(feature = "json")]
mod provenance_tests {
    use concord_core::__private::{Core, Draft, Options, Piece};
    use concord_core::body::Entries;
    use serde::{Serialize, Serializer};
    use std::collections::BTreeMap;

    struct Nested(usize);
    impl Serialize for Nested {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            if self.0 == 0 {
                s.serialize_str("x")
            } else {
                s.serialize_some(&Nested(self.0 - 1))
            }
        }
    }
    #[derive(Serialize)]
    #[serde(rename = "__concord_body_entries")]
    struct Ordinary<T>(BTreeMap<String, T>);

    fn build_error(value: impl Serialize) -> concord_core::Error {
        let mut draft = Draft::new(concord_core::reqwest::Method::GET);
        draft.base(&[Piece::Lit("https://safe.test/")]);
        draft.body(super::json(value));
        draft
            .finish(&Core::default(), &Options::default(), "provenance.get")
            .unwrap_err()
    }

    #[test]
    fn c6_2_serde_newtype_names_and_map_keys_cannot_supply_input_names() {
        let ordinary = || {
            Ordinary(BTreeMap::from([(
                "PRIVATE_MAP_KEY".to_owned(),
                Nested(129),
            )]))
        };
        let error = build_error(ordinary());
        assert!(!error.to_string().contains("PRIVATE_MAP_KEY"));
        assert!(!format!("{error:?}").contains("PRIVATE_MAP_KEY"));
        assert!(!error.to_string().contains("input "));

        let mut entries = Entries::new();
        entries.__push("description_name", ordinary());
        let error = build_error(entries);
        assert!(
            error.to_string().contains("input description_name"),
            "{error}"
        );
        assert!(!format!("{error} {error:?}").contains("PRIVATE_MAP_KEY"));

        // Nested Entries must keep the outer entry name and depth, too.
        let mut inner = Entries::new();
        inner.__push("inner_name", Nested(129));
        let mut outer = Entries::new();
        outer.__push("outer_name", inner);
        let error = build_error(outer);
        assert!(error.to_string().contains("input outer_name"), "{error}");
    }

    #[test]
    fn c6_2_a_serde_rename_does_not_change_map_key_semantics() {
        #[derive(Serialize)]
        #[serde(rename = "__concord_body_entries")]
        struct Ordinary(BTreeMap<u8, u8>);
        let body = super::json(Ordinary(BTreeMap::from([(0, 0)]))).unwrap();
        assert_eq!(body.as_bytes().unwrap(), br#"{"0":0}"#);
    }
}

#[cfg(test)]
mod writer_error_tests {
    use std::error::Error as _;
    struct Invalid;
    impl serde::Serialize for Invalid {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("original format failure"))
        }
    }
    #[cfg(feature = "json")]
    #[test]
    fn c2_14_json_keeps_one_format_error_below_fixed_writer_text() {
        let error = super::json(Invalid).unwrap_err();
        assert_eq!(error.to_string(), "the body could not be written");
        let cause = error.source().unwrap();
        let format = cause.downcast_ref::<serde_json::Error>().unwrap();
        assert_eq!(format.classify(), serde_json::error::Category::Data);
        assert!(!format.is_io());
        assert_eq!(cause.to_string(), "original format failure");
        assert!(cause.source().is_none());
    }
    #[cfg(feature = "form")]
    #[test]
    fn c2_14_form_keeps_one_format_error_below_fixed_writer_text() {
        let error = super::form(Invalid).unwrap_err();
        assert_eq!(error.to_string(), "the body could not be written");
        let cause = error.source().unwrap();
        assert_eq!(cause.to_string(), "original format failure");
        assert!(cause.source().is_none());
    }
}
