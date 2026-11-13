//! C2.4 Serde's data model at every text placement, including recursive values.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use concord_core::{
    __private::{Core, Draft, Options, Piece},
    error::Kind,
    http::Method,
};
use proptest::prelude::*;
use serde::{
    ser::{
        SerializeMap, SerializeStruct, SerializeStructVariant, SerializeTuple,
        SerializeTupleStruct, SerializeTupleVariant,
    },
    Serialize, Serializer,
};

#[derive(Debug, Clone)]
enum Data {
    None,
    Unit,
    UnitStruct,
    Char(char),
    Bool(bool),
    I(i128),
    U(u128),
    F32(u32),
    F64(u64),
    Text(String),
    Bytes(Vec<u8>),
    Some(Box<Data>),
    Newtype(Box<Data>),
    List(Vec<Data>),
    Tuple(Vec<Data>),
    TupleStruct(Vec<Data>),
    TupleVariant(Vec<Data>),
    NewtypeVariant(Box<Data>),
    StructVariant(Box<Data>),
    Map(Vec<(String, Data)>),
    Struct(Box<Data>),
    Variant,
}
impl Serialize for Data {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::None => s.serialize_none(),
            Self::Unit => s.serialize_unit(),
            Self::UnitStruct => s.serialize_unit_struct("Unit"),
            Self::Char(v) => s.serialize_char(*v),
            Self::Bool(v) => s.serialize_bool(*v),
            Self::I(v) => s.serialize_i128(*v),
            Self::U(v) => s.serialize_u128(*v),
            Self::F32(v) => s.serialize_f32(f32::from_bits(*v)),
            Self::F64(v) => s.serialize_f64(f64::from_bits(*v)),
            Self::Text(v) => s.serialize_str(v),
            Self::Bytes(v) => s.serialize_bytes(v),
            Self::Some(v) => s.serialize_some(v),
            Self::Newtype(v) => s.serialize_newtype_struct("Wrapper", v),
            Self::List(v) => v.serialize(s),
            Self::Tuple(v) => {
                let mut tuple = s.serialize_tuple(v.len())?;
                for v in v {
                    tuple.serialize_element(v)?;
                }
                tuple.end()
            }
            Self::TupleStruct(v) => {
                let mut tuple = s.serialize_tuple_struct("Tuple", v.len())?;
                for v in v {
                    tuple.serialize_field(v)?;
                }
                tuple.end()
            }
            Self::TupleVariant(v) => {
                let mut tuple = s.serialize_tuple_variant("Enum", 0, "Tuple", v.len())?;
                for v in v {
                    tuple.serialize_field(v)?;
                }
                tuple.end()
            }
            Self::NewtypeVariant(v) => s.serialize_newtype_variant("Enum", 0, "Wrapped", v),
            Self::StructVariant(v) => {
                let mut record = s.serialize_struct_variant("Enum", 0, "Record", 1)?;
                record.serialize_field("value", v)?;
                record.end()
            }
            Self::Map(v) => {
                let mut m = s.serialize_map(Some(v.len()))?;
                for (k, v) in v {
                    m.serialize_entry(k, v)?;
                }
                m.end()
            }
            Self::Struct(v) => {
                let mut m = s.serialize_struct("Record", 1)?;
                m.serialize_field("value", v)?;
                m.end()
            }
            Self::Variant => s.serialize_unit_variant("Choice", 0, "Ready"),
        }
    }
}
fn data() -> impl Strategy<Value = Data> {
    prop_oneof![
        Just(Data::None),
        Just(Data::Unit),
        Just(Data::UnitStruct),
        any::<char>().prop_map(Data::Char),
        any::<bool>().prop_map(Data::Bool),
        prop_oneof![
            any::<i128>(),
            prop::sample::select(vec![
                i128::MIN,
                i128::MAX,
                i64::MIN as i128,
                i64::MAX as i128,
                0,
                -1
            ])
        ]
        .prop_map(Data::I),
        prop_oneof![
            any::<u128>(),
            prop::sample::select(vec![u128::MAX, u64::MAX as u128, 0, 1])
        ]
        .prop_map(Data::U),
        prop_oneof![
            any::<u32>(),
            prop::sample::select(vec![
                0,
                1,
                u32::MAX,
                f32::MAX.to_bits(),
                f32::MIN_POSITIVE.to_bits(),
                f32::INFINITY.to_bits()
            ])
        ]
        .prop_map(Data::F32),
        prop_oneof![
            any::<u64>(),
            prop::sample::select(vec![
                0,
                1,
                u64::MAX,
                f64::MAX.to_bits(),
                f64::MIN_POSITIVE.to_bits(),
                f64::INFINITY.to_bits()
            ])
        ]
        .prop_map(Data::F64),
        proptest::collection::vec(any::<char>(), 0..48)
            .prop_map(|v| Data::Text(v.into_iter().collect())),
        proptest::collection::vec(any::<u8>(), 0..48).prop_map(Data::Bytes),
        Just(Data::Variant)
    ]
    .prop_recursive(10, 128, 6, |inner| {
        prop_oneof![
            inner.clone().prop_map(|v| Data::Some(Box::new(v))),
            inner.clone().prop_map(|v| Data::Newtype(Box::new(v))),
            proptest::collection::vec(inner.clone(), 0..6).prop_map(Data::List),
            proptest::collection::vec((".{0,12}", inner.clone()), 0..6).prop_map(Data::Map),
            inner.clone().prop_map(|v| Data::Struct(Box::new(v))),
            inner
                .clone()
                .prop_map(|v| Data::NewtypeVariant(Box::new(v))),
            inner.clone().prop_map(|v| Data::StructVariant(Box::new(v))),
            proptest::collection::vec(inner.clone(), 0..6).prop_map(Data::Tuple),
            proptest::collection::vec(inner.clone(), 0..6).prop_map(Data::TupleStruct),
            proptest::collection::vec(inner, 0..6).prop_map(Data::TupleVariant)
        ]
    })
}
#[derive(Debug, PartialEq)]
enum Shape {
    Absent,
    One(String),
    List(Vec<String>),
    Invalid,
    Nonfinite,
}
fn shape(v: &Data, list: bool) -> Shape {
    match v {
        Data::None | Data::Unit => Shape::Absent,
        Data::Some(v) | Data::Newtype(v) => shape(v, list),
        Data::Bool(v) => Shape::One(v.to_string()),
        Data::Char(v) => Shape::One(v.to_string()),
        Data::I(v) => Shape::One(v.to_string()),
        Data::U(v) => Shape::One(v.to_string()),
        Data::Text(v) => Shape::One(v.clone()),
        Data::Variant => Shape::One("Ready".into()),
        Data::F32(v) => {
            if f32::from_bits(*v).is_finite() {
                Shape::One(f32::from_bits(*v).to_string())
            } else {
                Shape::Nonfinite
            }
        }
        Data::F64(v) => {
            if f64::from_bits(*v).is_finite() {
                Shape::One(f64::from_bits(*v).to_string())
            } else {
                Shape::Nonfinite
            }
        }
        Data::List(values) | Data::Tuple(values) if list => {
            let mut result = Vec::new();
            for v in values {
                match shape(v, false) {
                    Shape::One(v) => result.push(v),
                    Shape::Nonfinite => return Shape::Nonfinite,
                    _ => return Shape::Invalid,
                }
            }
            Shape::List(result)
        }
        _ => Shape::Invalid,
    }
}
fn request(value: &Data, place: u8) -> Result<concord_core::reqwest::Request, concord_core::Error> {
    let mut d = Draft::new(Method::GET);
    d.base(&[Piece::Lit("https://example.test")]);
    match place {
        0 => d.segment(&[Piece::Lit(""), Piece::Val("input", value)]),
        1 => d.segments("input", value),
        2 => d.base(&[
            Piece::Lit("https://"),
            Piece::Val("input", value),
            Piece::Lit(".test"),
        ]),
        3 => d.base(&[
            Piece::Lit("https://example.test:"),
            Piece::Val("input", value),
        ]),
        4 => d.query("value", &[Piece::Val("input", value)]),
        5 => d.header("x-value", &[Piece::Val("input", value)]),
        6 => d.query("value", &[Piece::Lit(""), Piece::Val("input", value)]),
        _ => d.header("x-value", &[Piece::Lit(""), Piece::Val("input", value)]),
    }
    d.finish(&Core::default(), &Options::default(), "props.values")
}
// Placement validation is independent of the engine. Finite float spelling is checked
// against serde_json in extras; here its query text is additionally checked bit for bit.
fn scalar_matches(value: &Data, text: &str) {
    match leaf(value) {
        Data::F32(bits) => assert_eq!(text.parse::<f32>().unwrap().to_bits(), *bits),
        Data::F64(bits) => assert_eq!(text.parse::<f64>().unwrap().to_bits(), *bits),
        value => assert_eq!(shape(value, false), Shape::One(text.to_owned())),
    }
}
fn leaf(mut value: &Data) -> &Data {
    while let Data::Some(v) | Data::Newtype(v) = value {
        value = v;
    }
    value
}
fn header_legal(text: &str) -> bool {
    text.bytes().all(|b| b == b'\t' || (32..=126).contains(&b))
}
fn failure(expected: &Shape, place: u8) -> Option<String> {
    let slot = match place {
        0 | 1 => "path",
        2 | 3 => "host",
        4 | 6 => "query",
        _ => "header",
    };
    let shape_error = matches!(expected, Shape::Invalid)
        || place < 4 && matches!(expected, Shape::Absent | Shape::List(_))
        || place >= 6 && matches!(expected,Shape::List(v) if !v.is_empty());
    let message = if matches!(expected, Shape::Nonfinite) {
        Some("input input is not a finite number".to_owned())
    } else if shape_error {
        Some(format!("input input cannot be written in a {slot}"))
    } else {
        match expected {
            Shape::One(text)
                if place == 0 && (text.is_empty() || text == "." || text == "..")
                    || place == 1
                        && (text.is_empty() || text.split('/').any(|s| s == "." || s == "..")) =>
            {
                Some("input input gives an empty or dot path segment".into())
            }
            Shape::One(text)
                if place == 2
                    && (text.is_empty()
                        || text.len() > 63
                        || text.starts_with('-')
                        || text.ends_with('-')
                        || !text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')) =>
            {
                Some("input input is not one label of a host".into())
            }
            Shape::One(text)
                if place == 3
                    && (text.is_empty()
                        || !text.bytes().all(|b| b.is_ascii_digit())
                        || text.parse::<u16>().is_err()) =>
            {
                Some("the base is not a URL that can be used".into())
            }
            Shape::One(text) if matches!(place, 5 | 7) && !header_legal(text) => {
                Some("the value of header x-value is not a legal header value".into())
            }
            Shape::List(texts) if place == 5 && texts.iter().any(|s| !header_legal(s)) => {
                Some("the value of header x-value is not a legal header value".into())
            }
            _ => None,
        }
    };
    message.map(|s| format!("build error: {s}, in props.values"))
}
proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn c2_4_nested_serde_values_are_accepted_or_the_right_build_error(value in data()) {
        let mut expected = shape(&value, true);
        if matches!(expected, Shape::One(_) | Shape::List(_)) {
            let reference = request(&value, 4).unwrap();
            let texts: Vec<_> = reference
                .url()
                .query_pairs()
                .map(|(key, v)| {
                    assert_eq!(key, "value");
                    v.into_owned()
                })
                .collect();
            match leaf(&value) {
                Data::List(items) | Data::Tuple(items) => {
                    assert_eq!(texts.len(), items.len());
                    for (item, text) in items.iter().zip(&texts) {
                        scalar_matches(item, text);
                    }
                    expected = Shape::List(texts);
                }
                _ => {
                    assert_eq!(texts.len(), 1);
                    scalar_matches(&value, &texts[0]);
                    expected = Shape::One(texts[0].clone());
                }
            }
        }
        for place in 0..8 {
            let result = request(&value, place);
            match failure(&expected, place) {
                Some(message) => {
                    let e = result.unwrap_err();
                    prop_assert_eq!(e.kind(), Kind::Build);
                    prop_assert_eq!(e.to_string(), message);
                }
                None => {
                    let r = result.unwrap();
                    prop_assert_eq!(r.url().fragment(), None);
                }
            }
        }
    }
}

// A tiny value describes deep Serde Some nesting without allocating or recursively dropping a tree.
struct Deep(usize);
impl Serialize for Deep {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.0 == 0 {
            s.serialize_u8(1)
        } else {
            s.serialize_some(&Deep(self.0 - 1))
        }
    }
}
#[test]
fn c2_4_depth_child() {
    if let Ok(depth) = std::env::var("CONCORD_PROP_DEPTH") {
        let depth: usize = depth.parse().unwrap();
        let mut d = Draft::new(Method::GET);
        d.base(&[Piece::Lit("https://example.test")]);
        d.query("v", &[Piece::Val("depth", &Deep(depth))]);
        let result = d.finish::<()>(&Core::default(), &Options::default(), "props.depth");
        if depth > 128 {
            let error = result.unwrap_err();
            assert_eq!(error.kind(), Kind::Build);
            assert_eq!(
                error.to_string(),
                "build error: input depth cannot be written in a query, in props.depth"
            );
        } else {
            assert!(result.is_ok());
        }
    }
}
fn probe(depth: usize) -> bool {
    std::process::Command::new("bash")
        .args(["-c", "ulimit -c 0\nexec \"$@\"", "props-depth"])
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", "c2_4_depth_child", "--nocapture"])
        .env("CONCORD_PROP_DEPTH", depth.to_string())
        .env("RUST_BACKTRACE", "0")
        .current_dir(std::env::current_exe().unwrap().parent().unwrap())
        .output()
        .unwrap()
        .status
        .success()
}
proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]

    fn c2_4_serde_option_depth_never_overflows(depth in 0usize..131073){
        prop_assert!(
            probe(depth),
            "Some nesting depth {} overflows the engine stack",
            depth
        );
    }
}

#[test]

fn c2_4_regression_deep_some_stack_overflow() {
    // 16294 was the smallest observed shrink; the threshold varies with the
    // feature build and thread stack layout. Keep a stable larger witness too.
    for depth in [16_294, 32_768] {
        assert!(probe(depth));
    }
}
proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn c2_4_bounded_option_depth_never_panics(depth in 0usize..1025){
        assert!(probe(depth));
    }
}

#[test]
fn c2_4_a49_the_depth_limit_applies_at_every_text_placement_and_in_lists() {
    #[derive(serde::Serialize)]
    struct Newtype<T>(T);
    for depth in [127, 128, 129, 32_768] {
        for place in ["host", "path", "query", "header"] {
            let mut d = Draft::new(Method::GET);
            if place == "host" {
                d.base(&[
                    Piece::Lit("https://"),
                    Piece::Val("nested", &Newtype(Deep(depth))),
                    Piece::Lit(".test"),
                ]);
            } else {
                d.base(&[Piece::Lit("https://example.test")]);
                let value = Newtype(Deep(depth));
                let pieces = [Piece::Val("nested", &value)];
                match place {
                    "path" => d.segment(&pieces),
                    "query" => d.query("v", &pieces),
                    _ => d.header("x-value", &pieces),
                }
            }
            let result = d.finish::<()>(&Core::default(), &Options::default(), "props.depth");
            if depth >= 128 {
                assert_eq!(
                    result.unwrap_err().to_string(),
                    format!(
                        "build error: input nested cannot be written in a {place}, in props.depth"
                    )
                );
            } else {
                assert!(result.is_ok());
            }
        }
    }
    for depth in [127, 128, 32_768] {
        let mut d = Draft::new(Method::GET);
        d.base(&[Piece::Lit("https://example.test")]);
        d.query("v", &[Piece::Val("nested", &vec![Deep(depth)])]);
        assert_eq!(
            d.finish::<()>(&Core::default(), &Options::default(), "props.depth")
                .is_ok(),
            depth == 127
        );
    }
}
