//! C2.4 to C2.8 and C2.13: the text of a value, where each shape may go, the query, the
//! headers, a `None`, and the name in a Build error (docs/reference/contracts.md, group C2;
//! docs/internals/engine.md, "How a value is written"; docs/reference/rules.md rules 7, 8 and 10).

#[macro_use]
mod common;

use common::*;
use concord::__private::AdaptValue as _;
use concord::__private::{Piece, Value, ValueRef};
use concord::Secret;
use serde::Serialize;

const SHAPE: &str = "build error: input filter cannot be written in a query, in tasks.get";
const FINITE: &str = "build error: input ratio is not a finite number, in tasks.get";
const ILLEGAL: &str =
    "build error: the value of header x-note is not a legal header value, in tasks.get";

/// One entry `"filter" = value;` in `query`.
fn in_query(value: &dyn Value) -> Built {
    let mut draft = get();
    draft.query("filter", &[Piece::Val("filter", value)]);
    finish(draft, "tasks.get")
}

/// One entry `"x-note" = value;` in `headers`.
fn in_header(value: &dyn Value) -> Built {
    let mut draft = get();
    draft.header("x-note", &[Piece::Val("x-note", value)]);
    finish(draft, "tasks.get")
}

/// The text of a value, as a header gives it back (a header is never percent-encoded).
fn text_of(value: &dyn Value) -> String {
    let request = ok(in_header(value));
    let values = header(&request, "x-note");
    assert_eq!(values.len(), 1, "one text");
    values.into_iter().next().unwrap_or_default()
}

#[derive(Serialize)]
enum Region {
    #[serde(rename = "eu-west")]
    EuWest,
    Asia,
}

#[derive(Serialize)]
struct Id(u64);

#[derive(Serialize)]
struct Filter {
    done: bool,
}

#[derive(Serialize)]
enum Shape {
    Newtype(u32),
    Tuple(u32, u32),
    Struct { a: u32 },
}

struct Raw;
impl Serialize for Raw {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(b"raw")
    }
}

// C2.4 -------------------------------------------------------------------------------

#[test]
fn c2_4_a_string_a_char_a_bool_and_a_number_are_one_text() {
    assert_eq!(text_of(&(&ValueRef(&"text")).adapt()), "text");
    assert_eq!(
        text_of(&(&ValueRef(&String::from("owned"))).adapt()),
        "owned"
    );
    assert_eq!(text_of(&(&ValueRef(&'c')).adapt()), "c");
    assert_eq!(text_of(&(&ValueRef(&true)).adapt()), "true");
    assert_eq!(text_of(&(&ValueRef(&false)).adapt()), "false");
    assert_eq!(text_of(&(&ValueRef(&42u8)).adapt()), "42");
    assert_eq!(text_of(&(&ValueRef(&-7i64)).adapt()), "-7");
    assert_eq!(
        text_of(&(&ValueRef(&u128::MAX)).adapt()),
        u128::MAX.to_string()
    );
}

#[test]
fn c2_4_a_float_is_written_as_serde_json_writes_a_number() {
    assert_eq!(text_of(&(&ValueRef(&1.0f64)).adapt()), "1.0");
    for value in [
        0.1f64,
        1.5,
        -2.25,
        1e21,
        1.5e-7,
        f64::MAX,
        f64::MIN_POSITIVE,
        0.0,
        -0.0,
    ] {
        let json = serde_json::to_string(&value).expect("a finite number");
        assert_eq!(text_of(&(&ValueRef(&value)).adapt()), json, "{value:?}");
    }
    for value in [1.0f32, 0.1, 3.4e38] {
        let json = serde_json::to_string(&value).expect("a finite number");
        assert_eq!(text_of(&(&ValueRef(&value)).adapt()), json, "{value:?}");
    }
}

#[test]
fn c2_4_a_float_that_is_not_finite_is_a_build_error() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut draft = get();
        draft.query("ratio", &[val!("ratio", value)]);
        assert_eq!(build_error(finish(draft, "tasks.get")), FINITE, "{value:?}");
    }
    let mut draft = get();
    draft.query("ratio", &[val!("ratio", f32::NAN)]);
    assert_eq!(build_error(finish(draft, "tasks.get")), FINITE);
}

#[test]
fn c2_4_a_variant_with_no_data_is_its_serde_name() {
    assert_eq!(text_of(&(&ValueRef(&Region::EuWest)).adapt()), "eu-west");
    assert_eq!(text_of(&(&ValueRef(&Region::Asia)).adapt()), "Asia");
}

#[test]
fn c2_4_a_newtype_or_a_some_around_one_text_is_that_text() {
    assert_eq!(text_of(&(&ValueRef(&Id(7))).adapt()), "7");
    assert_eq!(text_of(&(&ValueRef(&Some("x"))).adapt()), "x");
    assert_eq!(text_of(&(&ValueRef(&Some(Id(8)))).adapt()), "8");
}

#[test]
fn c2_4_nothing_is_a_none_or_the_unit() {
    for request in [
        ok(in_query(&(&ValueRef(&None::<u32>)).adapt())),
        ok(in_query(&(&ValueRef(&())).adapt())),
    ] {
        assert_eq!(query(&request), "");
    }
    for request in [
        ok(in_header(&(&ValueRef(&None::<u32>)).adapt())),
        ok(in_header(&(&ValueRef(&())).adapt())),
    ] {
        assert!(header(&request, "x-note").is_empty());
    }
}

#[test]
fn c2_4_a_sequence_or_a_tuple_of_texts_is_several() {
    let request = ok(in_query(&(&ValueRef(&vec!["a", "b"])).adapt()));
    assert_eq!(query(&request), "filter=a&filter=b");
    let request = ok(in_query(&(&ValueRef(&(1u32, "b", true))).adapt()));
    assert_eq!(query(&request), "filter=1&filter=b&filter=true");
    let request = ok(in_query(
        &(&ValueRef(&[Region::EuWest, Region::Asia])).adapt(),
    ));
    assert_eq!(query(&request), "filter=eu-west&filter=Asia");
}

#[test]
fn c2_4_any_other_shape_is_a_build_error() {
    let mut map = std::collections::BTreeMap::new();
    map.insert("a", 1u32);
    let cases: [(&str, Built); 7] = [
        ("a map", in_query(&(&ValueRef(&map)).adapt())),
        (
            "a struct",
            in_query(&(&ValueRef(&Filter { done: true })).adapt()),
        ),
        (
            "a variant with data",
            in_query(&(&ValueRef(&Shape::Newtype(1))).adapt()),
        ),
        (
            "a tuple variant",
            in_query(&(&ValueRef(&Shape::Tuple(1, 2))).adapt()),
        ),
        (
            "a struct variant",
            in_query(&(&ValueRef(&Shape::Struct { a: 1 })).adapt()),
        ),
        ("bytes", in_query(&(&ValueRef(&Raw)).adapt())),
        (
            "a list inside a list",
            in_query(&(&ValueRef(&vec![vec![1u32], vec![2]])).adapt()),
        ),
    ];
    for (shape, built) in cases {
        assert_eq!(build_error(built), SHAPE, "{shape}");
    }
}

#[test]
fn c2_4_a_secret_and_an_optional_secret_are_written_as_their_text() {
    let key = Secret::new("k-1");
    let request = ok(in_query(&(&ValueRef(&key)).adapt()));
    assert_eq!(query(&request), "filter=k-1");
    let request = ok(in_query(&(&ValueRef(&&key)).adapt()));
    assert_eq!(query(&request), "filter=k-1");
    let set = Some(Secret::new("k-2"));
    let request = ok(in_query(&(&ValueRef(&set)).adapt()));
    assert_eq!(query(&request), "filter=k-2");
    let unset = None::<Secret>;
    let request = ok(in_query(&(&ValueRef(&unset)).adapt()));
    assert_eq!(query(&request), "");
}

#[test]
fn c2_4_a_secret_replaced_before_the_request_is_built_is_read_then() {
    let key = Secret::new("old");
    key.replace("new");
    let request = ok(in_query(&(&ValueRef(&key)).adapt()));
    assert_eq!(query(&request), "filter=new");
}

// C2.5 -------------------------------------------------------------------------------

#[test]
fn c2_5_a_whole_entry_of_a_query_or_of_headers_takes_nothing_one_or_several() {
    assert_eq!(query(&ok(in_query(&(&ValueRef(&None::<u8>)).adapt()))), "");
    assert_eq!(query(&ok(in_query(&(&ValueRef(&5u8)).adapt()))), "filter=5");
    assert_eq!(
        query(&ok(in_query(&(&ValueRef(&[5u8, 6])).adapt()))),
        "filter=5&filter=6"
    );
    assert!(header(&ok(in_header(&(&ValueRef(&None::<u8>)).adapt())), "x-note").is_empty());
    assert_eq!(
        header(&ok(in_header(&(&ValueRef(&5u8)).adapt())), "x-note"),
        ["5"]
    );
    assert_eq!(
        header(&ok(in_header(&(&ValueRef(&[5u8, 6])).adapt())), "x-note"),
        ["5", "6"]
    );
}

#[test]
fn c2_5_a_template_hole_takes_one_and_nothing_or_an_empty_list_leaves_the_entry_out() {
    let template = |value: &dyn Value| {
        let mut draft = get();
        draft.query("q", &[Piece::Lit("by-"), Piece::Val("filter", value)]);
        draft.header("x-note", &[Piece::Lit("by-"), Piece::Val("filter", value)]);
        finish(draft, "tasks.get")
    };
    let request = ok(template(&(&ValueRef(&"ann")).adapt()));
    assert_eq!(query(&request), "q=by-ann");
    assert_eq!(header(&request, "x-note"), ["by-ann"]);
    for request in [
        ok(template(&(&ValueRef(&None::<String>)).adapt())),
        ok(template(&(&ValueRef(&Vec::<String>::new())).adapt())),
    ] {
        assert_eq!(query(&request), "");
        assert!(header(&request, "x-note").is_empty());
    }
}

#[test]
fn c2_5_several_in_a_template_hole_is_a_build_error() {
    let mut draft = get();
    draft.query("q", &[lit!("by-"), val!("filter", ["a", "b"])]);
    assert_eq!(build_error(finish(draft, "tasks.get")), SHAPE);
    let mut draft = get();
    draft.header("x-note", &[lit!("by-"), val!("filter", ["a", "b"])]);
    assert_eq!(
        build_error(finish(draft, "tasks.get")),
        cannot_be_written("filter", "header", "tasks.get")
    );
    // A list of one is several too, and so is a tuple of one (C2.4).
    for one in [
        &(&ValueRef(&vec!["a"])).adapt() as &dyn Value,
        &(&ValueRef(&("a",))).adapt(),
    ] {
        let mut draft = get();
        draft.query("q", &[Piece::Lit("by-"), Piece::Val("filter", one)]);
        assert_eq!(build_error(finish(draft, "tasks.get")), SHAPE);
        let mut draft = get();
        draft.header("x-note", &[Piece::Lit("by-"), Piece::Val("filter", one)]);
        assert_eq!(
            build_error(finish(draft, "tasks.get")),
            cannot_be_written("filter", "header", "tasks.get")
        );
    }
    // A template that is one hole and nothing else, `"{filter}"`: the generated code gives
    // the draft the same pieces as a whole entry (`[Val]`), so it is not a template here.
    // A single-hole template uses the same value-writing path as a whole entry.
}

#[test]
fn c2_5_a_host_hole_takes_one_and_nothing_or_several_is_a_build_error() {
    let host = |value: &dyn Value| {
        let mut draft = concord::__private::Draft::new(http::Method::GET);
        draft.base(&[
            Piece::Lit("https://"),
            Piece::Val("region", value),
            Piece::Lit(".api.example.com"),
        ]);
        draft.segment(&[Piece::Lit("ids")]);
        finish(draft, "tasks.get")
    };
    assert_eq!(
        ok(host(&(&ValueRef(&"eu")).adapt())).url().host_str(),
        Some("eu.api.example.com")
    );
    for built in [
        host(&(&ValueRef(&None::<String>)).adapt()),
        host(&(&ValueRef(&())).adapt()),
        host(&(&ValueRef(&["eu", "us"])).adapt()),
        host(&(&ValueRef(&Vec::<String>::new())).adapt()),
        host(&(&ValueRef(&vec!["eu"])).adapt()),
        host(&(&ValueRef(&("eu",))).adapt()),
    ] {
        assert_eq!(
            build_error(built),
            cannot_be_written("region", "host", "tasks.get")
        );
    }
}

#[test]
fn c2_5_an_empty_text_is_one_text_and_its_entry_is_sent() {
    // an empty text in a query writes `k=`; an empty header value is legal.
    let request = ok(in_query(&(&ValueRef(&"")).adapt()));
    assert_eq!(request.url().query(), Some("filter="));
    let request = ok(in_header(&(&ValueRef(&"")).adapt()));
    assert_eq!(header(&request, "x-note"), [""]);
    // A list that holds an empty text sends it too.
    let request = ok(in_query(&(&ValueRef(&["", "a"])).adapt()));
    assert_eq!(request.url().query(), Some("filter=&filter=a"));
}

#[test]
fn c2_5_entries_that_all_leave_themselves_out_leave_no_question_mark() {
    //
    let mut draft = get();
    draft.query("a", &[val!("a", None::<u32>)]);
    draft.query("b", &[val!("b", Vec::<u32>::new())]);
    draft.query("c", &[lit!("by-"), val!("c", None::<String>)]);
    draft.query("d", &[val!("d", ())]);
    let request = ok(finish(draft, "tasks.get"));
    assert_eq!(request.url().query(), None);
    assert_eq!(url(&request), "https://api.example.com/v1");
}

#[test]
fn c2_4_an_optional_input_of_an_optional_secret_has_its_three_states() {
    // `key?: Option<Secret>`: the generated code gives `Option<Option<Secret>>` (C1.7).
    let set: Option<Option<Secret>> = Some(Some(Secret::new("k-1")));
    let inner_none: Option<Option<Secret>> = Some(None);
    let unset: Option<Option<Secret>> = None;
    let secret = Secret::new("k-2");
    let by_reference: Option<Option<&Secret>> = Some(Some(&secret));
    let mut draft = get();
    draft.query("set", &[val!("set", set)]);
    draft.query("inner", &[val!("inner", inner_none)]);
    draft.query("unset", &[val!("unset", unset)]);
    draft.query("by-ref", &[val!("by-ref", &by_reference)]);
    draft.header("x-set", &[val!("x-set", Some(Some(Secret::new("k-3"))))]);
    draft.header("x-inner", &[val!("x-inner", Some(None::<Secret>))]);
    draft.header(
        "x-template",
        &[lit!("Bearer "), val!("token", by_reference)],
    );
    let request = ok(finish(draft, "tasks.get"));
    assert_eq!(query(&request), "set=k-1&by-ref=k-2");
    assert_eq!(header(&request, "x-set"), ["k-3"]);
    assert!(sensitive(&request, "x-set"));
    assert!(header(&request, "x-inner").is_empty());
    assert_eq!(header(&request, "x-template"), ["Bearer k-2"]);
    assert!(sensitive(&request, "x-template"));
}

// C2.6 -------------------------------------------------------------------------------

#[test]
fn c2_6_query_pairs_are_in_the_order_of_the_declaration() {
    let mut draft = get();
    draft.query("b", &[val!("b", 1u8)]);
    draft.query("a", &[val!("a", 2u8)]);
    draft.query("c", &[val!("c", 3u8)]);
    draft.query("a2", &[lit!("fixed")]);
    assert_eq!(
        query(&ok(finish(draft, "tasks.get"))),
        "b=1&a=2&c=3&a2=fixed"
    );
}

#[test]
fn c2_6_the_items_of_a_list_are_in_their_order() {
    let mut draft = get();
    draft.query("first", &[val!("first", 0u8)]);
    draft.query("ids", &[val!("ids", vec![3u32, 1, 2])]);
    draft.query("last", &[val!("last", 9u8)]);
    assert_eq!(
        query(&ok(finish(draft, "tasks.get"))),
        "first=0&ids=3&ids=1&ids=2&last=9"
    );
}

#[test]
fn c2_6_a_value_is_percent_encoded_outside_the_unreserved_set() {
    for c in (0u8..=127).map(char::from).chain(['é', '€']) {
        let value = format!("a{c}b");
        let request = ok(in_query(&(&ValueRef(&value)).adapt()));
        assert_eq!(
            query(&request),
            format!("filter=a{}b", encoded(c)),
            "the character {c:?}"
        );
    }
    // A space is `%20`, never `+`; a `+` is `%2B`; `%` too.
    let request = ok(in_query(&(&ValueRef(&"a b+c%d")).adapt()));
    assert_query(&request, "filter=a%20b%2Bc%25d");
}

#[test]
fn c2_6_a_key_is_percent_encoded_outside_the_unreserved_set() {
    let mut draft = get();
    draft.query("filter[name]", &[val!("filter[name]", "x")]);
    draft.query("a b&c=d", &[val!("a b&c=d", "y")]);
    draft.query("k~-._", &[val!("k~-._", "z")]);
    assert_query(
        &ok(finish(draft, "tasks.get")),
        "filter%5Bname%5D=x&a%20b%26c%3Dd=y&k~-._=z",
    );
}

#[test]
fn c2_6_an_override_keeps_the_place_of_the_pair_or_pairs_it_replaces() {
    // What the generated code gives the draft, root first, for a client with
    // `query { "a" = 1; "ids" = client.ids; "b" = 2; }` and an endpoint with
    // `query { override "ids" = [9]; override "a" = 3; "c" = 4; }`.
    let mut draft = get();
    draft.query("a", &[val!("a", 1u8)]);
    draft.query("ids", &[val!("ids", [1u8, 2])]);
    draft.query("b", &[val!("b", 2u8)]);
    draft.query("ids", &[val!("ids", [9u8])]);
    draft.query("a", &[val!("a", 3u8)]);
    draft.query("c", &[val!("c", 4u8)]);
    assert_eq!(query(&ok(finish(draft, "tasks.get"))), "a=3&ids=9&b=2&c=4");
    // An override with a list in place of one pair keeps that place for all its pairs.
    let mut draft = get();
    draft.query("first", &[lit!("0")]);
    draft.query("a", &[val!("a", 1u8)]);
    draft.query("last", &[lit!("9")]);
    draft.query("a", &[val!("a", [5u8, 6])]);
    assert_eq!(
        query(&ok(finish(draft, "tasks.get"))),
        "first=0&a=5&a=6&last=9"
    );
}

#[test]
fn c2_6_a_template_in_a_query_is_encoded_for_its_place() {
    let mut draft = get();
    draft.query("q", &[lit!("by "), val!("name", "a&b")]);
    assert_query(&ok(finish(draft, "tasks.get")), "q=by%20a%26b");
}

// C2.7 -------------------------------------------------------------------------------

#[test]
fn c2_7_a_header_name_is_sent_in_lower_case() {
    let mut draft = get();
    draft.header("x-request-id", &[val!("x-request-id", "r-1")]);
    let request = ok(finish(draft, "tasks.get"));
    let names: Vec<&str> = request.headers().keys().map(|name| name.as_str()).collect();
    assert!(names.contains(&"x-request-id"), "{names:?}");
    assert_eq!(header(&request, "x-request-id"), ["r-1"]);
}

#[test]
fn c2_7_a_header_name_given_as_written_is_sent_in_lower_case_and_compared_without_case() {
    // `Draft::header` is given the name as written.
    let mut draft = get();
    draft.header("X-Request-ID", &[val!("X-Request-ID", "r-1")]);
    draft.header("Accept", &[lit!("application/json")]);
    let request = ok(finish(draft, "tasks.get"));
    let names: Vec<&str> = request.headers().keys().map(|name| name.as_str()).collect();
    assert_eq!(names, ["x-request-id", "accept"]);
    assert_eq!(header(&request, "x-request-id"), ["r-1"]);
    assert_eq!(header(&request, "accept"), ["application/json"]);
    // The same name in another case, below: it replaces, and one header is sent.
    let mut draft = get();
    draft.header("X-Note", &[lit!("root")]);
    draft.header("x-note", &[lit!("node")]);
    let request = ok(finish(draft, "tasks.get"));
    assert_eq!(request.headers().len(), 1);
    assert_eq!(header(&request, "x-note"), ["node"]);
}

#[test]
fn c2_7_a_header_value_is_never_percent_encoded() {
    for value in [
        "a%20b/c?d#e&f=g",
        "Bearer abc.def",
        "\"quoted\"",
        "~!*'();:@$,",
    ] {
        let request = ok(in_header(&(&ValueRef(&value)).adapt()));
        assert_eq!(header(&request, "x-note"), [value], "{value:?}");
    }
    // A fixed text with a space, as `"Bearer {token}"` writes it.
    let mut draft = get();
    draft.header("authorization", &[lit!("Bearer "), val!("token", "abc")]);
    assert_eq!(
        header(&ok(finish(draft, "tasks.get")), "authorization"),
        ["Bearer abc"]
    );
}

// "A header value may hold ... bytes 0x20 to 0x7E and a tab."
#[test]
fn c2_7_a_value_that_is_not_a_legal_header_value_is_a_build_error() {
    for value in [
        "a\nb",
        "a\rb",
        "a\r\nX-Evil: 1",
        "a\0b",
        "a\u{1b}b",
        "a\u{7f}b",
        "café",
        "€",
    ] {
        let built = in_header(&(&ValueRef(&value)).adapt());
        assert_eq!(build_error(built), ILLEGAL, "{value:?}");
    }
    for value in ["a b", "a\tb"] {
        let request = ok(in_header(&(&ValueRef(&value)).adapt()));
        assert_eq!(header(&request, "x-note"), [value], "{value:?}");
    }
    // The finished value of a template is checked too.
    let mut draft = get();
    draft.header("x-note", &[lit!("by "), val!("name", "a\nb")]);
    assert_eq!(build_error(finish(draft, "tasks.get")), ILLEGAL);
    // Every byte from `!` to `~` is visible.
    let visible: String = (0x21u8..=0x7e).map(char::from).collect();
    let request = ok(in_header(&(&ValueRef(&visible)).adapt()));
    assert_eq!(header(&request, "x-note"), [visible]);
    // bytes 0x20 to 0x7E and a tab, also inside a template.
    let legal: String = "\t"
        .chars()
        .chain((0x20u8..=0x7e).map(char::from))
        .chain(['x'])
        .collect();
    let mut draft = get();
    draft.header("x-note", &[lit!("by "), val!("name", legal.clone())]);
    assert_eq!(
        header(&ok(finish(draft, "tasks.get")), "x-note"),
        [format!("by {legal}")]
    );
}

#[test]
fn c2_7_a_list_sends_one_field_line_for_each_item() {
    let request = ok(in_header(&(&ValueRef(&vec!["a", "b", "c"])).adapt()));
    assert_eq!(header(&request, "x-note"), ["a", "b", "c"]);
    let request = ok(in_header(&(&ValueRef(&Vec::<String>::new())).adapt()));
    assert!(header(&request, "x-note").is_empty());
}

#[test]
fn c2_7_a_header_whose_value_holds_a_secret_is_marked_sensitive() {
    let token = Secret::new("t-1");
    let mut draft = get();
    draft.header("authorization", &[lit!("Bearer "), val!("token", token)]);
    draft.header("x-api-key", &[val!("x-api-key", token)]);
    draft.header(
        "x-optional",
        &[val!("x-optional", Some(Secret::new("t-2")))],
    );
    draft.header(
        "x-basic",
        &[val!("x-basic", concord::basic("user", "pass"))],
    );
    draft.header("x-plain", &[val!("x-plain", "public")]);
    let request = ok(finish(draft, "tasks.get"));
    assert_eq!(header(&request, "authorization"), ["Bearer t-1"]);
    assert!(sensitive(&request, "authorization"));
    assert_eq!(header(&request, "x-api-key"), ["t-1"]);
    assert!(sensitive(&request, "x-api-key"));
    assert_eq!(header(&request, "x-optional"), ["t-2"]);
    assert!(sensitive(&request, "x-optional"));
    assert!(sensitive(&request, "x-basic"));
    assert_eq!(header(&request, "x-plain"), ["public"]);
    assert!(!sensitive(&request, "x-plain"));
}

#[test]
fn c2_7_a_value_that_a_level_below_replaces_or_removes_is_not_checked() {
    // `headers { "x-note" = client.note; }` on the client, with a line break in it; the
    // endpoint writes `override "x-note" = "ok";`, or `override "x-note" = None::<String>;`.
    let mut draft = get();
    draft.header("x-note", &[val!("x-note", "a\nb")]);
    draft.header("x-note", &[lit!("ok")]);
    let request = ok(finish(draft, "tasks.get"));
    assert_eq!(header(&request, "x-note"), ["ok"]);
    let mut draft = get();
    draft.header("x-note", &[val!("x-note", "a\nb")]);
    draft.header("x-note", &[val!("x-note", None::<String>)]);
    let request = ok(finish(draft, "tasks.get"));
    assert!(header(&request, "x-note").is_empty());
    // The value that stays is checked.
    let mut draft = get();
    draft.header("x-note", &[lit!("ok")]);
    draft.header("x-note", &[val!("x-note", "a\nb")]);
    assert_eq!(build_error(finish(draft, "tasks.get")), ILLEGAL);
}

// C2.8 -------------------------------------------------------------------------------

#[test]
fn c2_8_a_template_that_reads_a_none_leaves_its_entry_out() {
    let mut draft = get();
    draft.query("before", &[lit!("1")]);
    draft.query("q", &[lit!("by-"), val!("name", None::<String>)]);
    draft.query("after", &[lit!("2")]);
    draft.header("x-note", &[lit!("by-"), val!("name", None::<String>)]);
    let request = ok(finish(draft, "tasks.get"));
    assert_eq!(query(&request), "before=1&after=2");
    assert!(header(&request, "x-note").is_empty());
}

#[test]
fn c2_8_every_hole_of_a_template_is_checked_before_the_entry_is_left_out() {
    // The `None` comes first, the hole that cannot be written second.
    let mut draft = get();
    draft.query(
        "q",
        &[
            val!("name", None::<String>),
            lit!("-"),
            val!("filter", Filter { done: true }),
        ],
    );
    assert_eq!(build_error(finish(draft, "tasks.get")), SHAPE);
    // And the other way round.
    let mut draft = get();
    draft.query(
        "q",
        &[
            val!("filter", Filter { done: true }),
            lit!("-"),
            val!("name", None::<String>),
        ],
    );
    assert_eq!(build_error(finish(draft, "tasks.get")), SHAPE);
    // A number that is not finite, beside a `None`.
    let mut draft = get();
    draft.query(
        "q",
        &[val!("name", None::<String>), val!("ratio", f64::NAN)],
    );
    assert_eq!(build_error(finish(draft, "tasks.get")), FINITE);
    // In a header too.
    let mut draft = get();
    draft.header(
        "x-note",
        &[val!("name", None::<String>), val!("ratio", f64::NAN)],
    );
    assert_eq!(build_error(finish(draft, "tasks.get")), FINITE);
}

#[test]
fn c2_8_an_entry_whose_value_is_none_makes_the_key_not_sent() {
    // What the generated code gives the draft for `override "x-note" = None::<String>;`
    // and `override "page" = None::<u32>;`: the key with a `None`.
    let mut draft = get();
    draft.query("page", &[val!("page", None::<u32>)]);
    draft.header("x-note", &[val!("x-note", None::<String>)]);
    let request = ok(finish(draft, "tasks.get"));
    assert_eq!(query(&request), "");
    assert!(header(&request, "x-note").is_empty());
}

#[test]
fn c2_8_an_override_whose_value_is_none_removes_the_key_where_it_was() {
    // `query { "a" = 1; "page" = 2; "b" = 3; }` above, `override "page" = None::<u32>;` below.
    let mut draft = get();
    draft.query("a", &[lit!("1")]);
    draft.query("page", &[val!("page", 2u32)]);
    draft.query("b", &[lit!("3")]);
    draft.query("page", &[val!("page", None::<u32>)]);
    assert_eq!(query(&ok(finish(draft, "tasks.get"))), "a=1&b=3");
    // Every pair of the key is removed, and then no `?` is left.
    let mut draft = get();
    draft.query("page", &[val!("page", [1u32, 2])]);
    draft.query("page", &[val!("page", None::<u32>)]);
    assert_eq!(ok(finish(draft, "tasks.get")).url().query(), None);
}

#[test]
fn c2_8_a_content_type_entry_whose_value_is_none_lets_the_body_label_be_written() {
    let mut draft = draft(http::Method::POST, "https://api.example.com/v1");
    draft.header("content-type", &[val!("content-type", None::<String>)]);
    draft.body(concord::json(serde_json::json!({ "a": 1 })));
    let request = ok(finish(draft, "tasks.create"));
    assert_eq!(header(&request, "content-type"), ["application/json"]);
}

// C2.13 ------------------------------------------------------------------------------

#[test]
fn c2_13_a_build_error_of_a_hole_names_its_input() {
    let mut draft = concord::__private::Draft::new(http::Method::GET);
    draft.base(&[
        lit!("https://"),
        val!("region", "a.b"),
        lit!(".example.com"),
    ]);
    assert_eq!(
        build_error(finish(draft, "tasks.get")),
        "build error: input region is not one label of a host, in tasks.get"
    );
    let mut draft = concord::__private::Draft::new(http::Method::GET);
    draft.base(&[
        lit!("https://"),
        val!("client.tenant", "a.b"),
        lit!(".example.com"),
    ]);
    assert_eq!(
        build_error(finish(draft, "tasks.get")),
        "build error: input client.tenant is not one label of a host, in tasks.get"
    );
    let mut draft = get();
    draft.segment(&[val!("id", "")]);
    assert_eq!(
        build_error(finish(draft, "tasks.get")),
        "build error: input id gives an empty or dot path segment, in tasks.get"
    );
    let mut draft = get();
    draft.query("ratio", &[val!("ratio", f64::INFINITY)]);
    assert_eq!(build_error(finish(draft, "tasks.get")), FINITE);
}

#[test]
fn c2_13_a_build_error_of_a_name_entry_names_its_input() {
    // `query { filter; }`: the key and the input have one name.
    let filter = Filter { done: true };
    let mut draft = get();
    draft.query("filter", &[val!("filter", filter)]);
    assert_eq!(build_error(finish(draft, "tasks.get")), SHAPE);
}

#[test]
fn c2_13_a_build_error_of_a_keyed_entry_names_its_key() {
    // `query { "labels" = client.labels; }`: the name is the key, not the input. The words
    // are `input <name>` (C2.13).
    let labels = Filter { done: false };
    let mut draft = get();
    draft.query("labels", &[val!("labels", labels)]);
    assert_eq!(
        build_error(finish(draft, "tasks.get")),
        "build error: input labels cannot be written in a query, in tasks.get"
    );
    // `headers { "x-labels" = client.labels; }`.
    let mut draft = get();
    draft.header("x-labels", &[val!("x-labels", Filter { done: false })]);
    assert_eq!(
        build_error(finish(draft, "tasks.get")),
        "build error: input x-labels cannot be written in a header, in tasks.get"
    );
}

#[test]
fn c2_13_an_entry_whose_expression_fails_names_its_key_and_keeps_the_error_as_its_source() {
    // `query { "limit" = raw.parse::<u32>()?; }`: the generated code gives the error to
    // `Draft::entry_error`.
    let failure = "twelve".parse::<u32>().expect_err("not a number");
    let error = concord::__private::Draft::entry_error::<()>(
        "tasks.get",
        "limit",
        Box::new(failure.clone()),
    );
    assert_eq!(error.kind(), concord::error::Kind::Build);
    assert_eq!(
        error.to_string(),
        "build error: input limit could not be written, in tasks.get"
    );
    let source = std::error::Error::source(&error).expect("the author's error");
    assert_eq!(
        source.downcast_ref::<std::num::ParseIntError>(),
        Some(&failure)
    );
}

#[test]
fn c2_13_a_build_error_prints_no_value() {
    let secret = "s3cr3t-value";
    for built in [
        {
            let mut draft = get();
            draft.segment(&[val!("id", format!("{secret}/.."))]);
            draft.segment(&[val!("id", ".")]);
            finish(draft, "tasks.get")
        },
        {
            let mut draft = get();
            draft.header("x-note", &[val!("x-note", format!("{secret}\n"))]);
            finish(draft, "tasks.get")
        },
        {
            let mut draft = concord::__private::Draft::new(http::Method::GET);
            draft.base(&[lit!("https://"), val!("region", format!("{secret}.x"))]);
            finish(draft, "tasks.get")
        },
    ] {
        let text = build_error(built);
        assert!(!text.contains(secret), "{text}");
    }
}
