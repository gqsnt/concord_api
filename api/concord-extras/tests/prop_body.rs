//! C1.21/C2.16 ordered Entries and an independent RFC 7578 multipart parser.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use concord_core::{
    __private::{AdaptValue as _, ValueRef},
    body::Entries,
    Secret,
};
use proptest::prelude::*;

#[cfg(feature = "json")]
fn value() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(Into::into),
        any::<i64>().prop_map(Into::into),
        any::<u64>().prop_map(Into::into),
        proptest::collection::vec(any::<char>(), 0..40)
            .prop_map(|v| serde_json::Value::String(v.into_iter().collect()))
    ]
    .prop_recursive(8, 96, 5, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..5).prop_map(serde_json::Value::Array),
            proptest::collection::btree_map(".{0,12}", inner, 0..5)
                .prop_map(|v| serde_json::Value::Object(v.into_iter().collect()))
        ]
    })
}
#[cfg(feature = "json")]
#[derive(Debug, PartialEq)]
struct Pairs(Vec<(String, serde_json::Value)>);
#[cfg(feature = "json")]
impl<'de> serde::Deserialize<'de> for Pairs {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Pairs;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an ordered object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Pairs, A::Error> {
                let mut out = Vec::new();
                while let Some(pair) = map.next_entry()? {
                    out.push(pair);
                }
                Ok(Pairs(out))
            }
        }
        d.deserialize_map(Visitor)
    }
}
const KEYS: [&str; 6] = ["", "x", "x", "a&b", "é", "quote\"\\\n"];
#[cfg(feature = "json")]
proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn c1_21_entries_json_preserves_order_presence_and_nested_values(
        input in proptest::collection::vec((0usize..KEYS.len(),prop::option::of(value())),0..16),secret in ".{0,80}",
    ){
        let secret = Secret::new(format!("SECRET_BEGIN{secret}SECRET_END"));
        let adapted = ValueRef(secret.clone());
        let mut entries = Entries::new();
        let mut expected = Vec::new();
        for (k, v) in &input {
            entries.__push(KEYS[*k], v);
            if let Some(v) = v {
                expected.push((KEYS[*k].to_owned(), v.clone()));
            }
        }
        entries.__push("secret", (&adapted).adapt());
        expected.push((
            "secret".into(),
            serde_json::Value::String(secret.expose().to_string()),
        ));
        assert!(!format!("{entries:?}").contains(&*secret.expose()));
        let body = concord_extras::body::json(entries).unwrap();
        prop_assert_eq!(
            serde_json::from_slice::<Pairs>(body.as_bytes().unwrap()).unwrap(),
            Pairs(expected)
        );
        assert!(!format!("{body:?}").contains(&*secret.expose()));
    }

    #[test]
    fn c2_4_finite_float_text_matches_json_digits(bits in any::<u64>()){
        let v = f64::from_bits(bits);
        let mut d = concord_core::__private::Draft::new(concord_core::http::Method::GET);
        d.base(&[concord_core::__private::Piece::Lit("https://example.test")]);
        d.query("v", &[concord_core::__private::Piece::Val("v", &v)]);
        let r = d.finish::<()>(&Default::default(), &Default::default(), "props.float");
        if v.is_finite() {
            prop_assert_eq!(
                r.unwrap()
                    .url()
                    .query_pairs()
                    .next()
                    .unwrap()
                    .1
                    .into_owned(),
                serde_json::to_string(&v).unwrap()
            );
        } else {
            prop_assert_eq!(r.unwrap_err().kind(), concord_core::error::Kind::Build);
        }
    }
}
#[cfg(feature = "form")]
proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn c1_21_entries_form_preserves_each_present_pair(input in proptest::collection::vec((0usize..KEYS.len(),prop::option::of(".{0,64}")),0..16)){
        let mut entries = Entries::new();
        let mut expected = Vec::new();
        for (k, v) in &input {
            entries.__push(KEYS[*k], v);
            if let Some(v) = v {
                expected.push((KEYS[*k].to_owned(), v.clone()));
            }
        }
        let body = concord_extras::body::form(entries).unwrap();
        let mut url: concord_core::reqwest::Url = "https://example.test".parse().unwrap();
        url.set_query(Some(std::str::from_utf8(body.as_bytes().unwrap()).unwrap()));
        prop_assert_eq!(
            url.query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[cfg(feature = "multipart")]
mod multipart {
    use super::*;
    #[derive(Debug, Clone, PartialEq)]
    struct Part {
        name: String,
        file: Option<(String, String)>,
        bytes: Vec<u8>,
    }
    fn text() -> impl Strategy<Value = String> {
        proptest::collection::vec(any::<char>(), 0..40).prop_map(|v| v.into_iter().collect())
    }
    fn parts() -> impl Strategy<Value = Vec<Part>> {
        proptest::collection::vec(
            (
                text(),
                prop::option::of((
                    text(),
                    prop_oneof![
                        text(),
                        Just("application/octet-stream".into()),
                        Just("text/plain".into())
                    ],
                )),
                proptest::collection::vec(any::<u8>(), 0..512),
            )
                .prop_map(|(name, file, bytes)| Part { name, file, bytes }),
            0..8,
        )
    }
    fn find(bytes: &[u8], needle: &[u8]) -> Option<usize> {
        bytes.windows(needle.len()).position(|v| v == needle)
    }
    // A50 keeps percent text and backslashes literal, and writes quotes as %22.
    // Compare the canonical header spelling; percent text is never decoded.
    fn quoted(input: &str) -> Result<(String, &str), String> {
        let chars = input
            .strip_prefix('"')
            .ok_or("missing opening quote")?
            .char_indices();
        let mut out = String::new();
        for (i, c) in chars {
            match c {
                '"' => return Ok((out, &input[i + 2..])),
                c if c == '\0'
                    || c == '\r'
                    || c == '\n'
                    || c == '\u{7f}'
                    || (c < ' ' && c != '\t') =>
                {
                    return Err("illegal quoted control".into())
                }
                c => out.push(c),
            }
        }
        Err("unclosed quote".into())
    }
    fn parse(bytes: &[u8], boundary: &str) -> Result<Vec<Part>, String> {
        let marker = format!("--{boundary}");
        let mut rest = bytes
            .strip_prefix(marker.as_bytes())
            .ok_or("first boundary")?;
        let mut parts = Vec::new();
        loop {
            if rest == b"--\r\n" {
                return Ok(parts);
            }
            rest = rest.strip_prefix(b"\r\n").ok_or("boundary CRLF")?;
            let end = find(rest, b"\r\n\r\n").ok_or("header terminator")?;
            let head = std::str::from_utf8(&rest[..end]).map_err(|_| "header UTF8")?;
            let mut lines = head.split("\r\n");
            let disp = lines
                .next()
                .ok_or("disposition")?
                .strip_prefix("Content-Disposition: form-data; name=")
                .ok_or("disposition prefix")?;
            let (name, tail) = quoted(disp)?;
            let file = if tail.is_empty() {
                None
            } else {
                let (file, tail) = quoted(tail.strip_prefix("; filename=").ok_or("filename")?)?;
                if !tail.is_empty() {
                    return Err("extra disposition".into());
                }
                let ct = lines
                    .next()
                    .ok_or("content type")?
                    .strip_prefix("Content-Type: ")
                    .ok_or("content type prefix")?;
                Some((file, ct.to_owned()))
            };
            if lines.next().is_some() {
                return Err("extra headers".into());
            }
            rest = &rest[end + 4..];
            let delimiter = format!("\r\n{marker}");
            let end = find(rest, delimiter.as_bytes()).ok_or("next boundary")?;
            parts.push(Part {
                name,
                file,
                bytes: rest[..end].to_vec(),
            });
            rest = &rest[end + delimiter.len()..];
        }
    }
    fn check(parts: Vec<Part>) {
        let mut writer = concord_extras::multipart();
        for part in &parts {
            writer = if let Some((file, ct)) = &part.file {
                writer.file(&part.name, file, ct, part.bytes.clone())
            } else {
                // Text parts must be UTF8; byte parts use the file API.
                writer.text(&part.name, String::from_utf8_lossy(&part.bytes))
            };
        }
        let result = writer.finish();
        let illegal = parts.iter().any(|p| {
            p.name.chars().any(char::is_control)
                || p.file
                    .as_ref()
                    .is_some_and(|(name, _)| name.chars().any(char::is_control))
                || p.file
                    .as_ref()
                    .is_some_and(|(_, ct)| concord_core::http::HeaderValue::from_str(ct).is_err())
        });
        if illegal {
            assert!(result.is_err());
            return;
        }
        let body = result.unwrap();
        let boundary = body
            .__content_type()
            .unwrap()
            .strip_prefix("multipart/form-data; boundary=")
            .unwrap();
        let bytes = body.as_bytes().unwrap();
        let mut expected = parts;
        for p in &mut expected {
            p.name = p.name.replace('"', "%22");
            if let Some((name, _)) = &mut p.file {
                *name = name.replace('"', "%22");
            }
            if p.file.is_none() {
                p.bytes = String::from_utf8_lossy(&p.bytes).into_owned().into_bytes();
            }
            assert!(find(&p.bytes, boundary.as_bytes()).is_none());
            assert!(!p.name.contains(boundary));
            if let Some((file, ct)) = &p.file {
                assert!(!file.contains(boundary));
                assert!(!ct.contains(boundary));
            }
        }
        assert_eq!(parse(bytes, boundary).unwrap(), expected);
    }
    #[test]

    fn c2_16_regression_nul_field_name() {
        check(vec![Part {
            name: "\0".into(),
            file: None,
            bytes: vec![],
        }]);
    }
    #[test]

    fn c2_16_regression_newline_aliases_literal_percent_0a() {
        let newline = concord_extras::multipart().text("\n", "").finish();
        let literal = concord_extras::multipart()
            .text("%0A", "")
            .finish()
            .unwrap();
        // No parser can recover both distinct names from identical bytes.
        assert!(newline.is_err());
        assert!(literal.as_bytes().unwrap().windows(3).any(|w| w == b"%0A"));
    }
    proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
        #[test]

        fn c2_16_multipart_arbitrary_parts_round_trip(parts in parts()){
            check(parts);
        }

        #[test]
        fn c2_16_multipart_valid_disposition_parameters_round_trip(mut parts in parts()){
            for p in &mut parts {
                p.name.retain(|c| !c.is_control());
                if let Some((name, _)) = &mut p.file {
                    name.retain(|c| !c.is_control());
                }
            }
            check(parts);
        }
        #[test]
        fn c2_16_multipart_any_input_finishes_or_errors_never_panics(parts in parts()){
            let mut writer = concord_extras::multipart();
            for p in parts {
                writer = match p.file {
                    Some((name, ct)) => writer.file(&p.name, &name, &ct, p.bytes),
                    None => writer.text(&p.name, String::from_utf8_lossy(&p.bytes)),
                };
            }
            let _ = writer.finish();
        }
        #[test]
        fn c2_16_multipart_boundary_is_absent_even_when_candidates_are_supplied(n in 0u8..12,prefix in ".{0,32}",suffix in ".{0,32}"){
            let collision = (0..n)
                .map(|i| format!("concord-{i:032x}"))
                .collect::<String>();
            check(vec![Part {
                name: "field".into(),
                file: Some(("file".into(), "application/octet-stream".into())),
                bytes: format!("{prefix}{collision}{suffix}").into_bytes(),
            }]);
        }
    }
}

#[test]
fn c2_4_a49_entries_bound_options_newtypes_and_collections_with_a_named_build_error() {
    use concord_core::__private::{Core, Draft, Options, Piece};
    use serde::{Serialize, Serializer};
    struct Deep(usize, u8);
    impl Serialize for Deep {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            if self.0 == 0 {
                return s.serialize_u8(1);
            }
            let next = Deep(self.0 - 1, self.1);
            match self.1 {
                0 => s.serialize_some(&next),
                1 => s.serialize_newtype_struct("Wrapper", &next),
                2 => {
                    use serde::ser::SerializeSeq;
                    let mut seq = s.serialize_seq(Some(1))?;
                    seq.serialize_element(&next)?;
                    seq.end()
                }
                3 => {
                    use serde::ser::SerializeMap;
                    let mut map = s.serialize_map(Some(1))?;
                    map.serialize_entry("child", &next)?;
                    map.end()
                }
                4 => {
                    use serde::ser::SerializeStruct;
                    let mut value = s.serialize_struct("Node", 1)?;
                    value.serialize_field("child", &next)?;
                    value.end()
                }
                _ => s.serialize_newtype_struct("__concord_body_entries", &next),
            }
        }
    }
    for mode in 0..6 {
        for depth in [127, 128, 129, 16_294, 32_768] {
            let mut entries = concord_core::body::Entries::new();
            entries.__push("nested", Deep(depth, mode));
            let body = concord_extras::body::json(&entries);
            if depth <= 128 {
                assert!(body.is_ok(), "depth={depth}, mode={mode}");
            } else {
                assert!(body.is_err());
                let mut draft = Draft::new(concord_core::http::Method::POST);
                draft.base(&[Piece::Lit("https://example.test")]);
                draft.body(body);
                let error = draft
                    .finish::<()>(&Core::default(), &Options::default(), "props.depth")
                    .unwrap_err();
                assert_eq!(
                    error.to_string(),
                    "build error: input nested cannot be written in a body, in props.depth",
                    "depth={depth}, mode={mode}"
                );
            }
        }
    }
    // Form accepts transparent option/newtype values; its depth limit has the
    // same named Build failure even though collections are unsupported.
    for mode in 0..2 {
        let mut entries = concord_core::body::Entries::new();
        entries.__push("nested", Deep(32_768, mode));
        let mut draft = Draft::new(concord_core::http::Method::POST);
        draft.base(&[Piece::Lit("https://example.test")]);
        draft.body(concord_extras::body::form(entries));
        let error = draft
            .finish::<()>(&Core::default(), &Options::default(), "props.depth")
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "build error: input nested cannot be written in a body, in props.depth"
        );
    }
}
