//! Executable design contracts for `concord::Debug`, as Salesforce's types rely on them.
#![allow(dead_code)]
use std::fmt;

struct NoDebug;

struct Poison;

impl fmt::Debug for Poison {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("a sensitive field's Debug must never be called")
    }
}

#[derive(concord::Debug)]
struct Secret<T>(#[sensitive] T);

#[derive(concord::Debug)]
struct Optional<T> {
    public: u32,
    #[sensitive]
    value: Option<T>,
}

#[derive(concord::Debug)]
#[sensitive]
struct Opaque<T> {
    value: T,
}

#[derive(concord::Debug)]
enum Outcome<T> {
    Next(#[sensitive] T),
    #[sensitive]
    Returned {
        value: T,
    },
}

mod standard {
    #[derive(Debug)]
    pub struct Public {
        pub sid: &'static str,
        pub status: Option<u32>,
    }
}

mod concord_derive {
    #[derive(concord::Debug)]
    pub struct Public {
        pub sid: &'static str,
        pub status: Option<u32>,
    }
}

#[test]
fn unmarked_fields_match_std_in_both_modes() {
    let expected = standard::Public {
        sid: "SM_public",
        status: Some(41),
    };
    let actual = concord_derive::Public {
        sid: "SM_public",
        status: Some(41),
    };
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
    assert_eq!(format!("{actual:#?}"), format!("{expected:#?}"));
}

#[test]
fn sensitive_generics_need_no_debug_and_never_call_inner_debug() {
    assert_eq!(format!("{:?}", Secret(NoDebug)), "Secret(<redacted>)");
    assert_eq!(format!("{:?}", Secret(Poison)), "Secret(<redacted>)");
    assert_eq!(
        format!("{:#?}", Secret(Poison)),
        "Secret(\n    <redacted>,\n)"
    );
    let opaque = Opaque { value: Poison };
    assert_eq!(format!("{opaque:?}"), "Opaque { .. }");
    assert!(!format!("{opaque:#?}").contains("Poison"));
    let next = Outcome::Next(Poison);
    assert_eq!(format!("{next:?}"), "Next(<redacted>)");
    assert!(!format!("{next:#?}").contains("Poison"));
    let returned = Outcome::Returned { value: Poison };
    assert_eq!(format!("{returned:?}"), "Returned { .. }");
    assert!(!format!("{returned:#?}").contains("Poison"));
}

#[test]
fn sensitive_option_preserves_presence_without_formatting_its_value() {
    let absent = Optional::<NoDebug> {
        public: 41,
        value: None,
    };
    let present = Optional {
        public: 41,
        value: Some(Poison),
    };
    assert_eq!(
        format!("{absent:?}"),
        "Optional { public: 41, value: None }"
    );
    assert_eq!(
        format!("{present:?}"),
        "Optional { public: 41, value: Some(<redacted>) }"
    );
    for shown in [format!("{absent:#?}"), format!("{present:#?}")] {
        assert!(shown.contains("41"));
        assert!(!shown.contains("Poison"));
    }
}

#[test]
fn headers_readers_and_closures_are_withheld_without_debug_bounds() {
    #[derive(concord::Debug)]
    struct Native {
        #[sensitive]
        header: concord::reqwest::header::HeaderValue,
        #[sensitive]
        reader: Box<dyn std::io::Read>,
        #[sensitive]
        callback: Box<dyn Fn()>,
    }
    let native = Native {
        header: concord::reqwest::header::HeaderValue::from_static("unflagged-secret-canary"),
        reader: Box::new(std::io::Cursor::new(b"reader-secret-canary")),
        callback: Box::new(|| panic!("formatting must not execute callbacks")),
    };
    for shown in [format!("{native:?}"), format!("{native:#?}")] {
        assert!(!shown.contains("secret-canary"));
        assert!(!shown.contains("Cursor"));
        assert!(shown.contains("<redacted>"));
    }
}
