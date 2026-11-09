//! `#[derive(concord::Error)]`: formatting, conversions and the empty `source()` chain, compared
//! with the hand-written implementations it replaces. Compile-fail cases are in
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
#[derive(Debug, concord::Error)]
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
#[derive(Debug, concord::Error)]
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

/// `concord::Error` the type and `concord::Error` the derive in one enum, generic too.
#[derive(Debug, concord::Error)]
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

/// Authored mistakes fail at the attribute, variant or field that was written.
#[test]
fn ui() {
    trybuild::TestCases::new().compile_fail("tests/ui_derive_error/*.rs");
}
