//! `#[derive(concord::derive::Error)]`: formatting, conversions and `source()` chains, compared
//! with the hand-written implementations it replaces. The split of a declared error document's
//! `concord::Error<D>` is tested in `error_document.rs`; compile-fail cases are in
//! `tests/ui_derive_error/*.rs`.

use std::error::Error as _;
use std::fmt;

/// A payload whose `Display` honours width, fill, alignment, precision and `#`.
#[derive(Debug)]
struct Padded(&'static str);

impl fmt::Display for Padded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            f.write_str("alt:")?;
        }
        f.pad(self.0)
    }
}

#[derive(Debug)]
struct Other(u16);

impl fmt::Display for Other {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "other {}", self.0)
    }
}

/// The derived enum: the shape of Salesforce's `SfError` (plain, boxed and skipped payloads).
#[derive(Debug, concord::derive::Error)]
enum Derived {
    #[display("{0}")]
    Plain(Padded),
    #[display("{0}")]
    Boxed(Box<Other>),
    // Skipped conversions take no part in duplicate-source checking (`Plain` converts from `Padded`).
    #[display("{0}")]
    #[from(skip)]
    Skipped(Box<Padded>),
    #[display("refused: {0} after {1:?}")]
    Pair(u16, &'static str),
    #[display("{name} failed (HTTP {status})")]
    Named {
        name: &'static str,
        status: u16,
        hidden: &'static str,
    },
    #[display("no result")]
    Unit,
    #[display("unread {{0}} left as text")]
    Escaped,
}

/// The same enum written by hand, as the SDKs did before.
#[derive(Debug)]
enum Manual {
    Plain(Padded),
    Boxed(Box<Other>),
    Skipped(Box<Padded>),
    Pair(u16, &'static str),
    Named {
        name: &'static str,
        status: u16,
        hidden: &'static str,
    },
    Unit,
    Escaped,
}

impl fmt::Display for Manual {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Manual::Plain(e) => fmt::Display::fmt(e, f),
            Manual::Boxed(e) => fmt::Display::fmt(e, f),
            Manual::Skipped(e) => fmt::Display::fmt(e, f),
            Manual::Pair(a, b) => write!(f, "refused: {a} after {b:?}"),
            Manual::Named { name, status, .. } => write!(f, "{name} failed (HTTP {status})"),
            Manual::Unit => f.write_str("no result"),
            Manual::Escaped => f.write_str("unread {0} left as text"),
        }
    }
}

impl std::error::Error for Manual {}

fn pairs() -> Vec<(Derived, Manual)> {
    vec![
        (
            Derived::Plain(Padded("secretless")),
            Manual::Plain(Padded("secretless")),
        ),
        (
            Derived::Boxed(Box::new(Other(7))),
            Manual::Boxed(Box::new(Other(7))),
        ),
        (
            Derived::Skipped(Box::new(Padded("kept"))),
            Manual::Skipped(Box::new(Padded("kept"))),
        ),
        (Derived::Pair(429, "a while"), Manual::Pair(429, "a while")),
        (
            Derived::Named {
                name: "describe",
                status: 404,
                hidden: "x",
            },
            Manual::Named {
                name: "describe",
                status: 404,
                hidden: "x",
            },
        ),
        (Derived::Unit, Manual::Unit),
        (Derived::Escaped, Manual::Escaped),
    ]
}

#[test]
fn display_and_debug_equal_the_hand_written_impls_under_every_flag() {
    for (derived, manual) in pairs() {
        if let (Derived::Named { hidden, .. }, Manual::Named { hidden: same, .. }) =
            (&derived, &manual)
        {
            assert!(!derived.to_string().contains(hidden) && !manual.to_string().contains(same));
        }
        assert_eq!(format!("{derived}"), format!("{manual}"));
        assert_eq!(format!("{derived:#}"), format!("{manual:#}"));
        assert_eq!(format!("{derived:>80}"), format!("{manual:>80}"));
        assert_eq!(format!("{derived:*<12.4}"), format!("{manual:*<12.4}"));
        assert_eq!(
            format!("{derived:?}").replace("Derived", "Manual"),
            format!("{manual:?}")
        );
        assert_eq!(
            format!("{derived:#?}").replace("Derived", "Manual"),
            format!("{manual:#?}")
        );
    }
    // The single-field form passes the caller's flags through; a longer string does not.
    assert_eq!(format!("{:>6}", Derived::Plain(Padded("ab"))), "    ab");
    assert_eq!(format!("{:#}", Derived::Plain(Padded("ab"))), "alt:ab");
    assert_eq!(format!("{:>6}", Derived::Unit), "no result");
}

#[test]
fn there_is_no_source_chain() {
    for (derived, _) in pairs() {
        assert!(derived.source().is_none());
        let boxed: Box<dyn std::error::Error> = Box::new(derived);
        assert!(boxed.source().is_none());
    }
}

#[test]
fn single_field_variants_convert_and_boxes_are_built() {
    assert!(matches!(
        Derived::from(Padded("p")),
        Derived::Plain(Padded("p"))
    ));
    assert!(matches!(Derived::from(Other(3)), Derived::Boxed(b) if b.0 == 3));
    let converted: Derived = Other(4).into();
    assert_eq!(converted.to_string(), "other 4");
    fn question_mark() -> Result<(), Derived> {
        Err(Padded("q"))?
    }
    assert_eq!(question_mark().unwrap_err().to_string(), "q");
}

/// Discord's shape: one enum-wide formatter, a named-field variant, conversions withheld.
#[derive(Debug, concord::derive::Error)]
#[display(with = fmt_hooked)]
enum Hooked {
    #[from(skip)]
    Gate(Box<u32>),
    Wrapped {
        error: Other,
        resource: Option<u64>,
    },
}

fn fmt_hooked(error: &Hooked, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match error {
        Hooked::Gate(seconds) => write!(f, "gated for {seconds}s"),
        Hooked::Wrapped {
            error,
            resource: Some(r),
        } => write!(f, "{error} (on {r})"),
        Hooked::Wrapped {
            error,
            resource: None,
        } => fmt::Display::fmt(error, f),
    }
}

#[test]
fn an_enum_wide_formatter_formats_every_variant() {
    assert_eq!(Hooked::Gate(Box::new(2)).to_string(), "gated for 2s");
    let wrapped = |resource| Hooked::Wrapped {
        error: Other(5),
        resource,
    };
    assert_eq!(wrapped(Some(8)).to_string(), "other 5 (on 8)");
    assert_eq!(wrapped(None).to_string(), "other 5");
    assert!(Hooked::Gate(Box::new(2)).source().is_none());
    assert!(wrapped(None).source().is_none());
}

/// `concord::Error` the type and `concord::derive::Error` in one enum, generic too.
#[derive(Debug, concord::derive::Error)]
enum Generic<T: fmt::Debug + fmt::Display> {
    #[display("{0}")]
    Concord(concord::Error),
    #[display("model: {0}")]
    #[from(skip)]
    Model(T),
}

#[test]
fn generic_enums_and_concord_payloads_derive() {
    let e: Generic<Other> = Generic::Model(Other(9));
    assert_eq!(e.to_string(), "model: other 9");
    fn converts<E: From<concord::Error> + std::error::Error>() {}
    converts::<Generic<Other>>();
}

/// Field names cannot reach the derive's own bindings: a field named like the formatter, and a
/// constant named like a field, format as the field.
mod hygiene {
    // Every generated local has an ambient constant of the same name, including field aliases.
    #[allow(non_upper_case_globals, dead_code)]
    const status: u16 = 7;
    #[allow(non_upper_case_globals, dead_code)]
    mod constants {
        pub const cause: u8 = 0;
        pub const __source: u8 = 0;
        pub const __formatter: u8 = 0;
        pub const __field__0: u8 = 0;
        pub const __field__1: u8 = 0;
        pub const __field_status: u8 = 0;
        pub const __field___formatter: u8 = 0;
        pub const __field___field___formatter: u8 = 0;
        pub const value: u8 = 0;
        pub const error: u8 = 0;
        pub const failure: u8 = 0;
    }
    #[allow(unused_imports)]
    use constants::*;

    pub(super) struct Problem;

    impl From<concord::Failure<Problem>> for super::Other {
        fn from(answer: concord::Failure<Problem>) -> Self {
            Self(answer.status().as_u16())
        }
    }

    #[derive(Debug, concord::derive::Error)]
    #[allow(non_snake_case)]
    pub(super) enum Shadowing {
        #[display("{__formatter}")]
        Alone { __formatter: &'static str },
        #[display("<{__formatter}> {__field___formatter}")]
        Text {
            __formatter: &'static str,
            __field___formatter: &'static str,
        },
        #[display("HTTP {status}")]
        Status { status: u16 },
        #[display("[{0:$>6}] {0} {1:?}")]
        Fill(&'static str, u8),
        #[display("{0}")]
        Plain(#[source] super::Other),
        #[display("{0}")]
        #[from(skip)]
        Boxed(#[source] Box<super::Other>),
        #[display("optional")]
        Maybe {
            #[source]
            cause: Option<super::Other>,
        },
        #[display("{0}")]
        Concord(#[source] concord::Error),
        #[display("{0}")]
        #[from(concord::Error<Problem>)]
        Api(Box<super::Other>),
    }
}

#[test]
fn field_names_never_shadow_the_generated_bindings() {
    use hygiene::Shadowing;
    let alone = Shadowing::Alone {
        __formatter: "field",
    };
    assert_eq!(format!("{alone:>7}"), "  field");
    let text = Shadowing::Text {
        __formatter: "a",
        __field___formatter: "b",
    };
    assert_eq!(text.to_string(), "<a> b");
    assert_eq!(Shadowing::Status { status: 404 }.to_string(), "HTTP 404");
    assert_eq!(Shadowing::Fill("ab", 3).to_string(), "[$$$$ab] ab 3");
}

#[test]
fn ambient_constants_never_shadow_the_generated_bindings() {
    use hygiene::Shadowing;
    let plain = Shadowing::from(Other(1));
    assert_eq!(plain.to_string(), "other 1");
    assert_eq!(plain.source().unwrap().to_string(), "other 1");
    let boxed = Shadowing::Boxed(Box::new(Other(2)));
    assert_eq!(boxed.source().unwrap().to_string(), "other 2");
    let optional = Shadowing::Maybe {
        cause: Some(Other(3)),
    };
    assert_eq!(optional.source().unwrap().to_string(), "other 3");
    assert!(Shadowing::Maybe { cause: None }.source().is_none());
    // Compiling the split conversion exercises both match bindings, including its fallback.
    fn converts<T: From<concord::Error<hygiene::Problem>>>() {}
    converts::<Shadowing>();
}

/// Field names are Rust identifiers, not only ASCII ones: delegation and composite strings.
#[derive(Debug, concord::derive::Error)]
enum Unicode {
    #[display("{détail}")]
    Alone { détail: Padded },
    #[display("échec {détail} ({コード})")]
    Composite {
        détail: &'static str, コード: u16
    },
}

#[test]
fn non_ascii_field_names_format() {
    let alone = Unicode::Alone {
        détail: Padded("ab"),
    };
    assert_eq!(format!("{alone:>4}"), "  ab");
    let composite = Unicode::Composite {
        détail: "quota",
        コード: 429,
    };
    assert_eq!(composite.to_string(), "échec quota (429)");
}

/// `#[source]` on one field per variant: plain, boxed, optional and named fields.
#[derive(Debug, concord::derive::Error)]
enum Caused {
    #[display("shared")]
    #[from(skip)]
    Shared(#[source] std::sync::Arc<Other>),
    #[display("plain")]
    Plain(#[source] Other),
    #[display("boxed after {0} tries")]
    Boxed(u8, #[source] Box<Other>),
    #[display("maybe")]
    Maybe {
        #[source]
        cause: Option<Other>,
    },
    #[display("{0}")]
    #[from(skip)]
    Uncaused(Other),
}

impl std::error::Error for Other {}

#[test]
fn marked_fields_are_the_source() {
    let shared = Caused::Shared(std::sync::Arc::new(Other(6)));
    assert_eq!(
        shared.source().unwrap().downcast_ref::<Other>().unwrap().0,
        6
    );
    let source = |e: &Caused| e.source().map(ToString::to_string);
    assert_eq!(source(&Caused::Plain(Other(1))).as_deref(), Some("other 1"));
    assert_eq!(
        source(&Caused::Boxed(0, Box::new(Other(2)))).as_deref(),
        Some("other 2")
    );
    let maybe = |cause| Caused::Maybe { cause };
    assert_eq!(source(&maybe(Some(Other(3)))).as_deref(), Some("other 3"));
    assert_eq!(source(&maybe(None)), None);
    assert_eq!(source(&Caused::Uncaused(Other(4))), None);
    // The boxed source is the payload itself: one level, not the box and then the payload.
    let boxed = Caused::Boxed(0, Box::new(Other(5)));
    assert!(boxed.source().unwrap().source().is_none());
}

/// Authored mistakes fail at the attribute, variant or field that was written.
#[test]
fn ui() {
    trybuild::TestCases::new().compile_fail("tests/ui_derive_error/*.rs");
}

#[derive(Debug, concord::derive::Error)]
enum Shared {
    #[display("{0}")]
    Cause(#[source] std::sync::Arc<Other>),
}

#[test]
fn shared_payloads_convert_and_expose_the_inner_source() {
    let error = Shared::from(Other(7));
    assert_eq!(error.to_string(), "other 7");
    assert_eq!(
        error.source().unwrap().downcast_ref::<Other>().unwrap().0,
        7
    );
}
