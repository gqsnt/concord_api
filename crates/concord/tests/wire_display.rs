//! `#[derive(concord::WireDisplay)]`: `Display` is the scalar serde spelling, and a value that is
//! not one scalar fails to build the request that places it.

use std::collections::BTreeMap;
use std::fmt::{self, Display};

use concord::{Delivery, ErrorKind, reqwest};
use serde::{Serialize, Serializer};

fn text(value: impl Display) -> Result<String, fmt::Error> {
    let mut out = String::new();
    fmt::write(&mut out, format_args!("{value}"))?;
    Ok(out)
}

#[derive(Serialize, concord::WireDisplay)]
#[serde(rename_all = "kebab-case")]
enum Sort {
    Created,
    LongRunning,
    #[serde(rename = "a b/c")]
    Renamed,
}

#[derive(Serialize, concord::WireDisplay)]
#[serde(transparent)]
struct Transparent(String);

#[derive(Serialize, concord::WireDisplay)]
struct Newtype(u64);

#[derive(Serialize, concord::WireDisplay)]
#[serde(transparent)]
struct Named {
    value: i64,
}

#[derive(Serialize, concord::WireDisplay)]
#[serde(untagged)]
enum Either {
    Id(Newtype),
    Name(String),
}

#[derive(Serialize, concord::WireDisplay)]
enum Open {
    #[serde(rename = "known")]
    Known,
    #[serde(untagged)]
    Other(String),
}

#[derive(Serialize, concord::WireDisplay)]
#[serde(transparent)]
struct Generic<T>(T);

#[test]
fn scalars_are_written_as_their_serde_spelling() {
    assert_eq!(text(Sort::Created).unwrap(), "created");
    assert_eq!(text(Sort::LongRunning).unwrap(), "long-running");
    assert_eq!(text(Sort::Renamed).unwrap(), "a b/c");
    assert_eq!(text(Transparent("x/y z%".into())).unwrap(), "x/y z%");
    assert_eq!(text(Newtype(u64::MAX)).unwrap(), "18446744073709551615");
    assert_eq!(text(Named { value: -1 }).unwrap(), "-1");
    assert_eq!(text(Either::Id(Newtype(7))).unwrap(), "7");
    assert_eq!(text(Either::Name("ci.yml".into())).unwrap(), "ci.yml");
    assert_eq!(text(Open::Known).unwrap(), "known");
    assert_eq!(text(Open::Other("later".into())).unwrap(), "later");
    assert_eq!(text(Generic(true)).unwrap(), "true");
    assert_eq!(text(Generic('é')).unwrap(), "é");
    assert_eq!(text(Generic(i128::MIN)).unwrap(), i128::MIN.to_string());
    assert_eq!(text(Generic(u128::MAX)).unwrap(), u128::MAX.to_string());
    assert_eq!(text(Generic(1.5_f64)).unwrap(), "1.5");
    assert_eq!(text(Generic(Some(3_u8))).unwrap(), "3");
}

#[test]
fn formatter_flags_do_not_change_the_wire_text() {
    assert_eq!(format!("{:>6}", Newtype(7)), "7");
    assert_eq!(format!("{:<6}|", Sort::Created), "created|");
}

#[derive(Serialize)]
struct Point {
    x: i32,
}

#[derive(Serialize)]
struct Unit;

#[derive(Serialize)]
enum Tagged {
    Payload(String),
    Pair(u8, u8),
}

#[test]
fn every_other_shape_fails_to_format() {
    assert!(text(Generic(vec!["a"])).is_err());
    assert!(text(Generic(BTreeMap::from([("k", "v")]))).is_err());
    assert!(text(Generic(Point { x: 1 })).is_err());
    assert!(text(Generic(Unit)).is_err());
    assert!(text(Generic(())).is_err());
    assert!(text(Generic(None::<u8>)).is_err());
    assert!(text(Generic(Tagged::Payload("p".into()))).is_err());
    assert!(text(Generic(Tagged::Pair(1, 2))).is_err());
    assert!(text(Generic(serde_bytes_like())).is_err());
}

struct Bytes;

impl Serialize for Bytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(b"raw")
    }
}

fn serde_bytes_like() -> Bytes {
    Bytes
}

/// A serializer that writes part of a secret, then fails.
struct Partial;

impl Serialize for Partial {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Half;
        impl Display for Half {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("WIRE_DISPLAY_CANARY")?;
                Err(fmt::Error)
            }
        }
        serializer.collect_str(&Half)
    }
}

#[derive(Serialize, concord::WireDisplay)]
#[serde(transparent)]
struct Values(Vec<String>);

#[derive(Serialize, concord::WireDisplay)]
#[serde(transparent)]
struct Secret(Partial);

concord::api! {
    client Scalars {
        base "http://127.0.0.1:9";
        GET sort(query sort: Sort);
        GET list(#[sensitive] query values: Values);
        GET segment(#[sensitive] secret: Secret) { path "items/{secret}"; }
    }
}

#[test]
fn a_placed_value_is_the_derived_text() {
    let client = Scalars::new(reqwest::Client::new());
    let prepared = client.sort(Sort::LongRunning).prepare().unwrap();
    assert_eq!(
        prepared.request().url().as_str(),
        "http://127.0.0.1:9/?sort=long-running"
    );
}

#[test]
fn a_non_scalar_is_a_build_error_naming_the_placement_and_nothing_of_the_value() {
    let client = Scalars::new(reqwest::Client::new());
    let error = client
        .list(Values(vec!["WIRE_DISPLAY_CANARY".into()]))
        .prepare()
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert_eq!(error.delivery(), Delivery::NotSent);
    let shown = format!("{error} {error:?}");
    assert!(shown.contains("query parameter `values`"), "{shown}");
    assert!(!shown.contains("WIRE_DISPLAY_CANARY"), "{shown}");

    let error = client.segment(Secret(Partial)).prepare().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    let shown = format!("{error} {error:?}");
    assert!(shown.contains("path segment `{secret}`"), "{shown}");
    assert!(!shown.contains("WIRE_DISPLAY_CANARY"), "{shown}");
}
