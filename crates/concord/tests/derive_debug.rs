//! `#[derive(concord::Debug)]`: unmarked output equals std's derive in both modes, a marked value
//! never reaches the output and its `Debug` is never called, and misplaced markers fail at the
//! attribute (`tests/ui_debug/*.stderr`; regenerate with `TRYBUILD=overwrite` and review).
#![allow(dead_code)]

use std::fmt;
use std::marker::PhantomData;

/// Panics when formatted: proves a withheld value is never formatted.
struct Poison;

impl fmt::Debug for Poison {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("a withheld value was formatted")
    }
}

/// Has no `Debug`: proves a withheld value needs none.
struct NoDebug;

const CANARY: &str = "concord-derive-canary";

/// The same shapes twice: `std` derives them, `ours` uses `concord::Debug` without markers.
macro_rules! both {
    ($($item:item)*) => {
        mod std_derive { $(#[derive(Debug)] $item)* }
        mod ours { $(#[derive(concord::Debug)] $item)* }
    };
}

both! {
    pub struct Named { pub sid: &'static str, pub status: Option<u32>, pub r#type: Vec<u8> }
    pub struct Tuple(pub u8, pub &'static str);
    pub struct Unit;
    pub struct Empty {}
    pub struct Generic<'a, T, const N: usize> { pub items: [T; N], pub name: &'a str }
    pub struct Marker<T>(pub std::marker::PhantomData<T>);
    pub enum Shape<T> { Unit, Tuple(T, u8), Named { a: T, r#b: Option<bool> }, Empty {} }
    pub enum Never {}
}

#[test]
fn unmarked_output_equals_std_in_both_modes() {
    fn same<A: fmt::Debug, B: fmt::Debug>(std: A, ours: B) {
        assert_eq!(format!("{ours:?}"), format!("{std:?}"));
        assert_eq!(format!("{ours:#?}"), format!("{std:#?}"));
    }
    same(
        std_derive::Named {
            sid: "SM1",
            status: Some(4),
            r#type: vec![1],
        },
        ours::Named {
            sid: "SM1",
            status: Some(4),
            r#type: vec![1],
        },
    );
    same(std_derive::Tuple(1, "a"), ours::Tuple(1, "a"));
    same(std_derive::Unit, ours::Unit);
    same(std_derive::Empty {}, ours::Empty {});
    same(
        std_derive::Generic {
            items: [1, 2],
            name: "n",
        },
        ours::Generic {
            items: [1, 2],
            name: "n",
        },
    );
    same(
        std_derive::Marker::<u8>(PhantomData),
        ours::Marker::<u8>(PhantomData),
    );
    same(std_derive::Shape::<u8>::Unit, ours::Shape::<u8>::Unit);
    same(std_derive::Shape::Tuple(7, 8), ours::Shape::Tuple(7, 8));
    same(
        std_derive::Shape::Named { a: "x", r#b: None },
        ours::Shape::Named { a: "x", r#b: None },
    );
    same(
        std_derive::Shape::<u8>::Empty {},
        ours::Shape::<u8>::Empty {},
    );
}

#[derive(concord::Debug)]
struct Secret<T>(#[sensitive] T);

#[derive(concord::Debug)]
struct Record<T> {
    id: u32,
    #[sensitive]
    name: String,
    #[sensitive]
    value: Option<T>,
    #[sensitive]
    full_path: std::option::Option<String>,
    #[sensitive]
    core_path: core::option::Option<NoDebug>,
    #[sensitive]
    nested: Vec<Option<String>>,
}

type MaybeText = Option<String>;

#[derive(concord::Debug)]
struct Aliased {
    #[sensitive]
    alias: MaybeText,
}

#[test]
fn a_sensitive_field_prints_redacted_and_is_never_formatted() {
    assert_eq!(format!("{:?}", Secret(NoDebug)), "Secret(<redacted>)");
    assert_eq!(format!("{:?}", Secret(Poison)), "Secret(<redacted>)");
    assert_eq!(
        format!("{:#?}", Secret(Poison)),
        "Secret(\n    <redacted>,\n)"
    );
    let record = Record {
        id: 7,
        name: CANARY.into(),
        value: Some(Poison),
        full_path: Some(CANARY.into()),
        core_path: None,
        nested: vec![Some(CANARY.into())],
    };
    assert_eq!(
        format!("{record:?}"),
        "Record { id: 7, name: <redacted>, value: Some(<redacted>), \
         full_path: Some(<redacted>), core_path: None, nested: <redacted> }"
    );
    let pretty = format!("{record:#?}");
    assert_eq!(
        pretty,
        "Record {\n    id: 7,\n    name: <redacted>,\n    value: Some(\n        <redacted>,\n    ),\n    \
         full_path: Some(\n        <redacted>,\n    ),\n    core_path: None,\n    nested: <redacted>,\n}"
    );
    // A type alias is opaque: one marker, presence not shown.
    assert_eq!(
        format!("{:?}", Aliased { alias: None }),
        "Aliased { alias: <redacted> }"
    );
    // Formatting changes no field: the value is read as written.
    assert_eq!(record.name, CANARY);
}

#[derive(concord::Debug)]
#[sensitive]
struct Opaque<T> {
    value: T,
    other: Poison,
}

#[derive(concord::Debug)]
#[sensitive]
struct OpaqueTuple(String, Poison);

#[derive(concord::Debug)]
enum Outcome<T> {
    Next(#[sensitive] T),
    #[sensitive]
    Returned {
        value: T,
    },
    #[sensitive]
    Hidden(T, Poison),
    Done,
}

#[derive(concord::Debug)]
#[sensitive]
enum AllWithheld {
    First(Poison),
    Second { value: Poison },
    Third,
}

#[test]
fn a_sensitive_type_or_variant_prints_its_name_only() {
    let opaque = Opaque {
        value: Poison,
        other: Poison,
    };
    assert_eq!(format!("{opaque:?}"), "Opaque { .. }");
    assert_eq!(format!("{opaque:#?}"), "Opaque { .. }");
    assert_eq!(
        format!("{:?}", OpaqueTuple(CANARY.into(), Poison)),
        "OpaqueTuple { .. }"
    );
    assert_eq!(format!("{:?}", Outcome::Next(Poison)), "Next(<redacted>)");
    assert_eq!(
        format!("{:?}", Outcome::Returned { value: Poison }),
        "Returned { .. }"
    );
    assert_eq!(
        format!("{:#?}", Outcome::Hidden(Poison, Poison)),
        "Hidden { .. }"
    );
    assert_eq!(format!("{:?}", Outcome::<NoDebug>::Done), "Done");
    assert_eq!(format!("{:?}", AllWithheld::First(Poison)), "First { .. }");
    assert_eq!(
        format!("{:?}", AllWithheld::Second { value: Poison }),
        "Second { .. }"
    );
    assert_eq!(format!("{:?}", AllWithheld::Third), "Third { .. }");
}

/// D4: inside a `#[sensitive]` type, `#[public]` named fields alone are shown; a field added
/// later stays withheld until marked.
#[derive(concord::Debug)]
#[sensitive]
struct Message<T> {
    #[public]
    sid: &'static str,
    #[public]
    status: Option<u32>,
    future_provider_field: T,
}

#[derive(concord::Debug)]
enum Event {
    #[sensitive]
    Delivered {
        #[public]
        id: u32,
        body: String,
    },
}

#[test]
fn public_fields_of_a_sensitive_type_are_shown_before_the_ellipsis() {
    let message = Message {
        sid: "SM1",
        status: Some(3),
        future_provider_field: Poison,
    };
    assert_eq!(
        format!("{message:?}"),
        "Message { sid: \"SM1\", status: Some(3), .. }"
    );
    assert_eq!(
        format!("{message:#?}"),
        "Message {\n    sid: \"SM1\",\n    status: Some(\n        3,\n    ),\n    ..\n}"
    );
    let unprintable = Message {
        sid: "SM1",
        status: Some(3),
        future_provider_field: NoDebug,
    };
    assert_eq!(format!("{unprintable:?}"), format!("{message:?}"));
    let event = Event::Delivered {
        id: 4,
        body: CANARY.into(),
    };
    assert_eq!(format!("{event:?}"), "Delivered { id: 4, .. }");
}

#[test]
fn native_headers_readers_and_closures_need_no_debug() {
    #[derive(concord::Debug)]
    struct Native {
        #[sensitive]
        header: concord::reqwest::header::HeaderValue,
        #[sensitive]
        reader: Box<dyn std::io::Read>,
        #[sensitive]
        callback: Box<dyn Fn()>,
        #[sensitive]
        later: Option<Box<dyn Fn()>>,
    }
    let native = Native {
        header: concord::reqwest::header::HeaderValue::from_static(CANARY),
        reader: Box::new(std::io::Cursor::new(CANARY.as_bytes())),
        callback: Box::new(|| panic!("formatting ran a callback")),
        later: Some(Box::new(|| panic!("formatting ran a callback"))),
    };
    assert_eq!(
        format!("{native:?}"),
        "Native { header: <redacted>, reader: <redacted>, callback: <redacted>, \
         later: Some(<redacted>) }"
    );
    assert!(!format!("{native:#?}").contains(CANARY));
}

/// A type parameter used only by withheld fields gets no `Debug` bound; one used by a shown field
/// does, as with std. A projection (`T::Item`) is bounded itself.
#[derive(concord::Debug)]
struct Bounds<S, T: Iterator> {
    #[sensitive]
    secret: S,
    shown: Vec<T::Item>,
}

#[test]
fn bounds_follow_the_shown_fields() {
    let value = Bounds::<NoDebug, std::vec::IntoIter<u8>> {
        secret: NoDebug,
        shown: vec![1],
    };
    assert_eq!(
        format!("{value:?}"),
        "Bounds { secret: <redacted>, shown: [1] }"
    );
}

#[test]
fn compile_errors_point_at_the_marker() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui_debug/*.rs");
}
