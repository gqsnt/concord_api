#![cfg(feature = "json")]
//! Regression probes from experience/challenge/code.md, copied into this workspace.
//! The standalone decoder still needs no engine or Incoming implementation.
#[path = "../src/json_decode.rs"]
#[allow(dead_code)]
mod json_decode;
use concord::page::{Call, IntoItems, Slot};
use concord::{http, Answer, Headers};
use concord_core as concord;
use concord_extras::page::*;

#[derive(Clone, Debug, Default)]
struct List {
    at: Option<u8>,
    limit: Option<u8>,
    url: String,
}
#[derive(Debug)]
struct Batch {
    n: usize,
}
impl IntoItems for Batch {
    type Item = ();
    fn into_items(self) -> Vec<()> {
        vec![(); self.n]
    }
    fn count(&self) -> usize {
        self.n
    }
}
impl Call for List {
    type Value = Batch;
    type Problem = ();
    fn at_url(mut self, url: &str) -> Self {
        self.url = url.to_owned();
        self
    }
}
#[derive(Clone, Copy)]
struct At;
impl Slot<List> for At {
    type Value = u8;
    fn get<'a>(&self, c: &'a List) -> Option<&'a u8> {
        c.at.as_ref()
    }
    fn set(&self, mut c: List, x: u8) -> List {
        c.at = Some(x);
        c
    }
}
#[derive(Clone, Copy)]
struct Limit;
impl Slot<List> for Limit {
    type Value = u8;
    fn get<'a>(&self, c: &'a List) -> Option<&'a u8> {
        c.limit.as_ref()
    }
    fn set(&self, mut c: List, x: u8) -> List {
        c.limit = Some(x);
        c
    }
}
// Test-only helper: an invalid HeaderValue fixture must fail the test.
#[allow(clippy::unwrap_used)]
fn answer(n: usize, links: &[&[u8]]) -> Answer<Batch> {
    let mut h = http::HeaderMap::new();
    for link in links {
        h.append("link", http::HeaderValue::from_bytes(link).unwrap());
    }
    Answer::new(http::StatusCode::OK, Headers::from(h), Batch { n })
}
fn link(fields: &[&[u8]]) -> Result<Option<String>, concord::Fail<()>> {
    by_link()(List::default(), &answer(1, fields)).map(|x| x.map(|c| c.url))
}

#[test]
fn link_malformed_field_must_not_mean_end_of_pages() {
    let mut missed = Vec::new();
    for s in [
        "nonsense",
        "rel=next",
        "</a>junk; rel=next",
        "garbage, </a>; rel=next",
        "</a>; rel=next next",
    ] {
        let got = link(&[s.as_bytes()]);
        if got.is_ok() {
            missed.push(format!("{s:?} => {got:?}"));
        }
    }
    assert!(missed.is_empty(), "{}", missed.join("\n"));
}

#[test]
fn link_valid_next_cannot_hide_later_unreadable_field() {
    let got = link(&[b"</ok>; rel=next", b"nonsense"]);
    assert!(got.is_err(), "{got:?}");
}

#[test]
fn link_bytes_quotes_anchors_and_field_order_controls() {
    for input in [
        "</x,a;b#f>; title=\"café, 日本; \\\"x\\\"\"; rel=next",
        "</bad>; rel=next; ANCHOR=\"/x\", </x,a;b#f>; REL=\"prev Next\"",
        "</bad>; rel=prev; rel=next, </x,a;b#f>; rel=next",
        "</bad>; title=\"x\\\"; rel=next\"; rel=prev, </x,a;b#f>; rel=\"n\\ext\"",
    ] {
        assert_eq!(
            link(&[input.as_bytes()]).unwrap().as_deref(),
            Some("/x,a;b"),
            "{input}"
        );
    }
    assert_eq!(
        link(&[b"</prev>; rel=prev", b"</a>; rel=next", b"</b>; rel=next"])
            .unwrap()
            .as_deref(),
        Some("/a")
    );
    for field in [
        b"</a>; rel=\"next".as_slice(),
        b"<\xff>; rel=next",
        b"</a>; rel=\"next\"junk",
        b"<unclosed",
    ] {
        assert!(link(&[b"</ok>; rel=next", field]).is_err(), "{field:?}");
    }
    for field in [
        "</a>; anchor=\"/\"; rel=next",
        "</a>; rel=next; anchor",
        "</a>; rel=prev",
    ] {
        assert_eq!(link(&[field.as_bytes()]).unwrap(), None);
    }
}

#[test]
fn link_quoted_obs_text_is_not_required_to_be_utf8() {
    // RFC 8288 imports RFC 7230 quoted-string: qdtext includes obs-text (%x80-FF).
    let result = link(&[b"</a>; rel=next; title=\"caf\xe9\""]);
    assert_eq!(result.unwrap().as_deref(), Some("/a"));
}

#[test]
fn counting_pagers_exhaust_u8_boundaries_and_custom_list() {
    let offset = by_offset(At);
    let limited = by_offset_limit(At, Limit);
    let page = by_page(At, 3);
    for at in 0..=255u8 {
        for n in [0, 1, 2, 254, 255, 256, usize::MAX] {
            let call = List {
                at: Some(at),
                limit: Some(4),
                ..List::default()
            };
            let ans = answer(n, &[]);
            let actual = offset(call.clone(), &ans);
            if n == 0 {
                assert!(actual.unwrap().is_none());
            } else if n <= usize::from(255 - at) {
                assert_eq!(actual.unwrap().unwrap().at, Some(at + n as u8));
            } else {
                assert!(actual.is_err());
            }
            let actual = limited(call.clone(), &ans);
            if n < 4 {
                assert!(actual.unwrap().is_none());
            } else if n <= usize::from(255 - at) {
                assert_eq!(actual.unwrap().unwrap().at, Some(at + n as u8));
            } else {
                assert!(actual.is_err());
            }
            let actual = page(call, &ans);
            if n == 0 {
                assert!(actual.unwrap().is_none());
            } else if at == 255 {
                assert!(actual.is_err());
            } else {
                assert_eq!(actual.unwrap().unwrap().at, Some(at + 1));
            }
        }
    }
    assert_eq!(
        offset(List::default(), &answer(2, &[]))
            .unwrap()
            .unwrap()
            .at,
        Some(2)
    );
    assert_eq!(
        page(List::default(), &answer(2, &[])).unwrap().unwrap().at,
        Some(4)
    );
    assert_eq!(
        limited(List::default(), &answer(2, &[]))
            .unwrap()
            .unwrap()
            .at,
        Some(2)
    );
    let v = vec![1, 2, 3];
    assert_eq!(IntoItems::count(&v), 3);
    assert_eq!(IntoItems::into_items(v), [1, 2, 3]);
}

#[test]
fn fragment_split_preserves_all_nonfragment_bytes() {
    for (input, expected) in [
        ("#only", ""),
        ("/a?x=%23foo#bar#baz", "/a?x=%23foo"),
        ("?q=a%2523b#", "?q=a%2523b"),
        ("", ""),
        ("//a/x#y", "//a/x"),
        ("/日本#一", "/日本"),
    ] {
        assert_eq!(concord::__private::without_fragment(input), expected);
        let pager = by_next_url(move |_: &Batch| Some(input));
        assert_eq!(
            pager(List::default(), &answer(0, &[])).unwrap().url,
            expected
        );
        assert_eq!(
            link(&[format!("<{input}>; rel=next").as_bytes()])
                .unwrap()
                .unwrap(),
            expected
        );
    }
}

#[test]
// The reference is intentional: this test covers a borrowed optional input.
#[allow(clippy::needless_borrows_for_generic_args)]
fn arg_positive_controls_and_secret_snapshot() {
    use concord::arg::Arg;
    fn milestone(x: impl Arg<Option<Option<u64>>>) -> Option<Option<u64>> {
        x.into_input()
    }
    assert_eq!(milestone(5), Some(Some(5)));
    assert_eq!(milestone(Some(5)), Some(Some(5)));
    assert_eq!(milestone(None::<u64>), Some(None));
    assert_eq!(milestone(None::<Option<u64>>), None);
    assert_eq!(milestone(&Some(5)), Some(Some(5)));
    let s = concord::Secret::new("before");
    let hold = s.expose();
    let c = s.clone();
    std::thread::spawn(move || {
        for _ in 0..1000 {
            c.replace("after");
        }
    })
    .join()
    .unwrap();
    assert_eq!(&*hold, "before");
    assert_eq!(&*s.expose(), "after");
    assert_eq!(format!("{s:#?}"), "Secret(…)");
}

fn safe_debug<T: std::fmt::Debug>(t: &T) {
    for text in [format!("{t:?}"), format!("{t:#?}")] {
        assert!(!text.contains("PRIVATE_SENTINEL"), "{text}");
    }
}
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct Row {
    kind: u64,
}

#[test]
fn json_details_message_budget_is_one_kib() {
    let input = format!("{{\"kind\":\"{}\"}}", "🦀PRIVATE_SENTINEL".repeat(1000));
    let e =
        crate::json_decode::decode::<Row>(input.as_bytes(), Some("application/json")).unwrap_err();
    let message = e
        .details
        .strip_suffix("… [cut]")
        .expect("a long message has a cut marker");
    assert!(message.len() <= 1024, "message bytes={}", message.len());
    assert!(
        e.details.len() <= 1024,
        "kept text and its mark fit in one KiB"
    );
    assert!(
        e.details.capacity() < 2048,
        "the original allocation is not retained"
    );
}

#[test]
fn json_struct_map_enum_source_and_utf8_cut_controls() {
    use std::collections::BTreeMap;
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct Outer {
        // The decoder regression deliberately traverses Box, Option, Vec and map layers.
        #[allow(clippy::box_collection)]
        rows: Vec<Option<Box<BTreeMap<String, Row>>>>,
    }
    for key in ["PRIVATE_SENTINEL", "rows", "kind", "a.b[0]"] {
        let input = format!("{{\"rows\":[null,{{\"{key}\":{{\"kind\":\"PRIVATE_SENTINEL\"}}}}]}}");
        let e = crate::json_decode::decode::<Outer>(input.as_bytes(), Some("application/json"))
            .unwrap_err();
        assert_eq!(e.place.to_string(), "rows[1][key].kind");
        safe_debug(&e);
        safe_debug(&e.source);
        assert!(!e.source.to_string().contains("PRIVATE_SENTINEL"));
        assert!(std::error::Error::source(&e.source).is_none());
    }
    for input in [
        b"".as_slice(),
        b"7 8",
        b"\xef\xbb\xbf\xef\xbb\xbf7",
        b"\xff",
    ] {
        assert!(crate::json_decode::decode::<u32>(input, None).is_err());
    }
    assert_eq!(
        crate::json_decode::decode::<u32>(b"\xef\xbb\xbf7", Some("text/plain")).unwrap(),
        7
    );
    for size in 990..1050 {
        let input = format!("{{\"kind\":\"{}\"}}", "🦀".repeat(size));
        let e = crate::json_decode::decode::<Row>(input.as_bytes(), None).unwrap_err();
        assert!(e.details.is_char_boundary(e.details.len()));
        assert!(e.details.len() <= 1033);
        safe_debug(&e.source);
    }
}

#[test]
fn json_path_exploration() {
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct Flat {
        #[serde(flatten)]
        row: Row,
    }
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    #[serde(tag = "type")]
    enum Internal {
        A { rows: Vec<Row> },
    }
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    #[serde(tag = "type", content = "data")]
    enum Adjacent {
        A { rows: Vec<Row> },
    }
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    enum External {
        A { rows: Vec<Row> },
    }
    for (name, e) in [
        (
            "flatten",
            crate::json_decode::decode::<Flat>(br#"{"kind":"PRIVATE_SENTINEL"}"#, None)
                .unwrap_err(),
        ),
        (
            "internal",
            crate::json_decode::decode::<Internal>(
                br#"{"type":"A","rows":[{"kind":"PRIVATE_SENTINEL"}]}"#,
                None,
            )
            .unwrap_err(),
        ),
        (
            "adjacent",
            crate::json_decode::decode::<Adjacent>(
                br#"{"type":"A","data":{"rows":[{"kind":"PRIVATE_SENTINEL"}]}}"#,
                None,
            )
            .unwrap_err(),
        ),
        (
            "external",
            crate::json_decode::decode::<External>(
                br#"{"A":{"rows":[{"kind":"PRIVATE_SENTINEL"}]}}"#,
                None,
            )
            .unwrap_err(),
        ),
    ] {
        let expected = match name {
            "flatten" | "internal" => "",
            "adjacent" => "data[key][0].kind",
            "external" => "[variant].rows[0].kind",
            _ => unreachable!(),
        };
        assert_eq!(e.place.to_string(), expected, "{name}");
        if name == "internal" {
            assert!(e.source.to_string().contains("line 0, column 0"));
        }
        safe_debug(&e);
        safe_debug(&e.source);
    }
}

#[test]
fn json_enum_path_contains_declared_fields() {
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    #[serde(tag = "type", content = "data")]
    enum Adjacent {
        A { rows: Vec<Row> },
    }
    let e = crate::json_decode::decode::<Adjacent>(
        br#"{"type":"A","data":{"rows":[{"kind":"PRIVATE_SENTINEL"}]}}"#,
        None,
    )
    .unwrap_err();
    assert_eq!(e.place.to_string(), "data[key][0].kind");
}
