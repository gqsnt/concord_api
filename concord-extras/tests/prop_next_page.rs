//! C8.6: the `Link` reader of `by_link` against a model of RFC 8288 as the contract reads it,
//! and the next URL of `by_next_url` as the request builder sends it, query and all.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use concord_core::page::Pager;
use concord_core::{
    __private::{Core, Draft, Options, Piece},
    error::Kind,
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    page::Call,
    reqwest::Url,
    Answer, Error, Secret,
};
use concord_extras::page::{by_link, by_next_url};
use proptest::prelude::*;

#[derive(Clone, Debug, Default, PartialEq)]
struct NextCall(Option<String>);
impl Call for NextCall {
    type Value = String;
    type Problem = ();
    fn at_url(self, url: &str) -> Self {
        Self(Some(url.to_owned()))
    }
}

/// A Mismatch from a pager, as its `Debug` names it.
const MISMATCH: &str = "Fail(mismatch)";

/// What `by_link` makes of these `Link` fields: the next URL, none, or a Mismatch.
fn follow(fields: &[Vec<u8>]) -> Result<Option<String>, String> {
    let mut headers = HeaderMap::new();
    for field in fields {
        headers.append("link", HeaderValue::from_bytes(field).unwrap());
    }
    let answer = Answer::new(StatusCode::OK, headers.into(), String::new());
    match by_link().next_call(NextCall::default(), &answer) {
        Ok(next) => Ok(next.map(|call| call.0.unwrap())),
        Err(fail) => Err(format!("{fail:?}")),
    }
}

/// A parameter value: a token, or a quoted string given as its items, each a byte and
/// whether it is written as a quoted pair.
#[derive(Clone, Debug)]
enum Value {
    None,
    Token(String),
    Quoted(Vec<(u8, bool)>),
}
impl Value {
    fn decoded(&self) -> Vec<u8> {
        match self {
            Value::None => Vec::new(),
            Value::Token(token) => token.as_bytes().to_vec(),
            Value::Quoted(items) => items.iter().map(|(byte, _)| *byte).collect(),
        }
    }
}
#[derive(Clone, Debug)]
struct Param {
    name: String,
    value: Value,
    // Whitespace before the `;`, after it, and on each side of `=`.
    space: [&'static str; 4],
}
#[derive(Clone, Debug)]
struct Link {
    target: String,
    params: Vec<Param>,
    // Whitespace before and after the link in its list.
    space: [&'static str; 2],
}

fn render_value(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::None => {}
        Value::Token(token) => out.extend_from_slice(token.as_bytes()),
        Value::Quoted(items) => {
            out.push(b'"');
            for (byte, pair) in items {
                if *pair {
                    out.push(b'\\');
                }
                out.push(*byte);
            }
            out.push(b'"');
        }
    }
}
fn render(links: &[Option<Link>]) -> Vec<u8> {
    let mut out = Vec::new();
    for (n, link) in links.iter().enumerate() {
        if n > 0 {
            out.push(b',');
        }
        let Some(link) = link else {
            out.extend_from_slice(b" ");
            continue;
        };
        out.extend_from_slice(link.space[0].as_bytes());
        out.push(b'<');
        out.extend_from_slice(link.target.as_bytes());
        out.push(b'>');
        for param in &link.params {
            out.extend_from_slice(param.space[0].as_bytes());
            out.push(b';');
            out.extend_from_slice(param.space[1].as_bytes());
            out.extend_from_slice(param.name.as_bytes());
            if !matches!(param.value, Value::None) {
                out.extend_from_slice(param.space[2].as_bytes());
                out.push(b'=');
                out.extend_from_slice(param.space[3].as_bytes());
                render_value(&param.value, &mut out);
            }
        }
        out.extend_from_slice(link.space[1].as_bytes());
    }
    out
}

/// The contract's choice: the first link, over all fields in order, with no `anchor` and whose
/// first `rel` lists `next` among its space-separated relations, without its fragment.
fn model(fields: &[Vec<Option<Link>>]) -> Option<String> {
    fields.iter().flatten().flatten().find_map(|link| {
        let anchored = link
            .params
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("anchor"));
        let rel = link
            .params
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case("rel"))?;
        let next = rel
            .value
            .decoded()
            .split(|byte| *byte == b' ')
            .any(|part| part.eq_ignore_ascii_case(b"next"));
        (!anchored && next).then(|| link.target.split('#').next().unwrap().to_owned())
    })
}

fn ows() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec!["", "", " ", "\t", "  ", " \t "])
}
fn token() -> impl Strategy<Value = String> {
    "[A-Za-z0-9!#$%&'*+.^_`|~-]{1,8}"
}
/// A relation list: `next` in any case among others, or near misses.
fn relations() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop::sample::select(vec![
            "next",
            "NEXT",
            "Next",
            "prev",
            "last",
            "nextish",
            "nex",
            "next-archive",
            "first",
        ]),
        1..4,
    )
    .prop_map(|words| words.join(" "))
}
fn quoted(text: String) -> impl Strategy<Value = Value> {
    proptest::collection::vec(any::<bool>(), text.len()).prop_map(move |pairs| {
        Value::Quoted(
            text.bytes()
                .zip(pairs)
                // A quote or a backslash is always written as a pair.
                .map(|(byte, pair)| (byte, pair || byte == b'"' || byte == b'\\'))
                .collect(),
        )
    })
}
fn value() -> impl Strategy<Value = Value> {
    prop_oneof![
        1 => Just(Value::None),
        2 => token().prop_map(Value::Token),
        3 => proptest::collection::vec(
            prop_oneof![
                3 => (0x20u8..0x7f).prop_map(char::from),
                1 => prop::sample::select(vec!['\t', ',', ';', '=', '<', '>', '"', '\\', 'é']),
            ],
            0..12,
        )
        .prop_map(|chars| chars.into_iter().collect::<String>())
        .prop_flat_map(quoted),
    ]
}
fn param() -> impl Strategy<Value = Param> {
    let rel = (
        prop::sample::select(vec!["rel", "REL", "Rel"]),
        relations(),
        any::<bool>(),
    )
        .prop_flat_map(|(name, words, quote)| {
            let value = if quote || words.contains(' ') {
                quoted(words).boxed()
            } else {
                Just(Value::Token(words)).boxed()
            };
            (Just(name.to_owned()), value)
        });
    let other = (
        prop_oneof![
            token(),
            prop::sample::select(vec![
                "anchor", "ANCHOR", "title", "type", "hreflang", "title*"
            ])
            .prop_map(str::to_owned),
        ],
        value(),
    );
    (
        prop_oneof![2 => rel, 3 => other],
        [ows(), ows(), ows(), ows()],
    )
        .prop_map(|((name, value), space)| Param { name, value, space })
}
fn target() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => "[A-Za-z0-9/._~:?#@!$&'()*+,;=%|{}^`\\[\\]-]{0,24}",
        1 => prop::sample::select(vec![
            "https://example.test/v1/items?page=2", "/v1/items?page=2#top", "?page=3", "",
            "../up", "//example.test/x", "/é?q=ü", "/a,b;c", "/%zz", "</no",
        ])
        .prop_map(|target| target.trim_start_matches('<').to_owned()),
    ]
}
fn link() -> impl Strategy<Value = Link> {
    (
        target(),
        proptest::collection::vec(param(), 0..5),
        [ows(), ows()],
    )
        .prop_map(|(target, params, space)| Link {
            target,
            params,
            space,
        })
}
fn field() -> impl Strategy<Value = Vec<Option<Link>>> {
    proptest::collection::vec(prop::option::weighted(0.85, link()), 1..5)
}

/// A change to one well-formed field that makes it one that cannot be read.
fn break_field(field: &[u8], how: u8) -> Vec<u8> {
    let mut out = field.to_vec();
    match how % 10 {
        0 => out.extend_from_slice(b"; =x"),
        1 => out.extend_from_slice(b";"),
        2 => out.extend_from_slice(b"; title=\"open"),
        3 => out.extend_from_slice(b"; rel="),
        4 => out.extend_from_slice(b"; rel=\"next\"junk"),
        5 => out.extend_from_slice(b"; ti tle=x"),
        6 => out.extend_from_slice(b", <a b>; rel=next"),
        7 => out.extend_from_slice(b", <a>x; rel=next"),
        8 => out.extend_from_slice(b", a>; rel=next"),
        _ => {
            // An unclosed target, inserted where a link begins.
            // The first `<` of a field always opens a link: only spaces and commas precede it.
            match out.iter().position(|byte| *byte == b'<') {
                Some(start) => out.insert(start + 1, b'<'),
                None => out.extend_from_slice(b", <open"),
            }
        }
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn c8_6_by_link_takes_the_link_that_the_model_takes(
        fields in proptest::collection::vec(field(), 1..4),
    ) {
        let rendered: Vec<Vec<u8>> = fields.iter().map(|field| render(field)).collect();
        prop_assert_eq!(follow(&rendered), Ok(model(&fields)), "{:?}",
            rendered.iter().map(|f| String::from_utf8_lossy(f).into_owned()).collect::<Vec<_>>());
    }

    #[test]
    fn c8_6_by_link_refuses_a_field_that_cannot_be_read_even_after_a_next_link(
        fields in proptest::collection::vec(field(), 1..4), broken in any::<prop::sample::Index>(),
        how in any::<u8>(),
    ) {
        let mut rendered: Vec<Vec<u8>> = fields.iter().map(|field| render(field)).collect();
        let n = broken.index(rendered.len());
        rendered[n] = break_field(&rendered[n], how);
        prop_assert_eq!(follow(&rendered), Err(MISMATCH.to_owned()), "{:?}",
            rendered.iter().map(|f| String::from_utf8_lossy(f).into_owned()).collect::<Vec<_>>());
    }

    #[test]
    fn c8_6_by_link_never_panics_on_bytes_near_the_grammar(
        pieces in proptest::collection::vec(prop_oneof![
            prop::sample::select(vec![
                "<", ">", ";", ",", "=", "\"", "\\", " ", "\t", "rel", "next", "anchor", "/a",
                "\\\"", "é", "\u{80}",
            ]).prop_map(|piece| piece.as_bytes().to_vec()),
            proptest::collection::vec(0x80u8..=0xff, 1..3),
        ], 0..48),
    ) {
        let field = pieces.concat();
        if HeaderValue::from_bytes(&field).is_ok() {
            if let Ok(Some(next)) = follow(&[field]) {
                prop_assert!(!next.contains('#'));
            }
        }
    }
}

/// The URL that the builder makes for a call sent to `next`, with or without a secret
/// query entry.
fn build(next: &str, secret: Option<&str>) -> Result<Url, Error> {
    let mut draft = Draft::new(Method::GET);
    draft.base(&[Piece::Lit("https://example.test/v1")]);
    draft.segment(&[Piece::Lit("items")]);
    draft.query("page", &[Piece::Lit("1")]);
    let secret = secret.map(Secret::new);
    if let Some(secret) = &secret {
        draft.query("token", &[Piece::Val("token", secret)]);
    }
    let mut options = Options::default();
    options.at_url(next);
    draft
        .finish(&Core::default(), &options, "props.next")
        .map(|request| request.url().clone())
}

fn percent_decoded(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        match (
            bytes[at],
            bytes.get(at + 1).copied().and_then(hex),
            bytes.get(at + 2).copied().and_then(hex),
        ) {
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                at += 3;
            }
            (b'+', _, _) => {
                out.push(b' ');
                at += 1;
            }
            (byte, _, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    out
}

/// C8.6 and C8.2: the next URL resolved against the URL that the call builds, its fragment
/// dropped, its query as given, less the pairs whose key is the secret's, which is written
/// again at the end. Another origin is a Build error.
fn expected(next: &str, secret: Option<&str>) -> Option<Url> {
    let built = Url::parse("https://example.test/v1/items?page=1").unwrap();
    let mut url = built.join(next.split('#').next().unwrap()).ok()?;
    if url.origin() != built.origin() {
        return None;
    }
    url.set_fragment(None);
    if let Some(secret) = secret {
        let mut parts: Vec<String> = url
            .query()
            .map(|query| {
                query
                    .split('&')
                    .filter(|part| percent_decoded(part.split('=').next().unwrap()) != b"token")
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        parts.push(format!("token={secret}"));
        url.set_query(Some(&parts.join("&")));
    }
    Some(url)
}

fn next_url() -> impl Strategy<Value = String> {
    (
        prop::sample::select(vec![
            "",
            "next",
            "/v1/next",
            "../up",
            "./",
            "//example.test/a",
            "https://example.test/b",
            "https://EXAMPLE.test:443/c",
            "HTTPS://example.test/d",
            "http://example.test/e",
            "https://other.test/f",
            "https://example.test:8443/g",
            "/sl ice",
            "/é",
            "/%zz",
            "/a|b",
            "http://[bad",
            "https://user@example.test/h",
            "\\\\example.test\\i",
        ]),
        prop::option::of(prop::sample::select(vec![
            "",
            "&&",
            "&",
            "a=1&a=2",
            "%41=%61",
            "q=a+b",
            "token=evil",
            "to%6ben=evil&x=1",
            "token",
            "x=1&token=2&y=3",
            "sp ace=1",
            "é=ü",
            "=",
            "&token=&",
            "TOKEN=kept",
            "token%3D=kept",
            "a=%zz",
        ])),
        prop::option::of(prop::sample::select(vec!["", "top", "a#b", "?q"])),
    )
        .prop_map(|(path, query, fragment)| {
            let mut url = path.to_owned();
            if let Some(query) = query {
                url.push('?');
                url.push_str(query);
            }
            if let Some(fragment) = fragment {
                url.push('#');
                url.push_str(fragment);
            }
            url
        })
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    /// Also proves C8.2.
    fn c8_6_a_next_url_is_sent_with_its_query_as_given(
        next in prop_oneof![
            3 => next_url(),
            1 => proptest::collection::vec(any::<char>(), 0..48)
                .prop_map(|chars| chars.into_iter().collect::<String>()),
        ],
        secret in prop::option::of("[A-Za-z0-9]{1,12}"),
    ) {
        let answer = Answer::new(StatusCode::OK, HeaderMap::new().into(), next.clone());
        let call = by_next_url(|text: &String| Some(text.clone()))
            .next_call(NextCall::default(), &answer)
            .unwrap()
            .unwrap();
        let given = call.0.unwrap();
        prop_assert_eq!(given.as_str(), next.split('#').next().unwrap());
        match (build(&given, secret.as_deref()), expected(&given, secret.as_deref())) {
            (Ok(url), Some(expected)) => prop_assert_eq!(url.as_str(), expected.as_str()),
            (Err(error), None) => prop_assert_eq!(error.kind(), Kind::Build),
            (actual, expected) => prop_assert!(false, "{:?} != {:?}", actual.map(|u| u.to_string()), expected),
        }
    }
}

#[test]
fn c8_6_a_link_field_of_ten_thousand_entries_is_read_whole() {
    let items: Vec<String> = (0..9_999)
        .map(|n| format!("<https://example.test/v1/items/{n}>; rel=\"item\"; title=\"a, b; c\""))
        .collect();
    let last = "</v1/items?page=2>; rel=\"next\"";
    let field = format!("{}, {last}", items.join(", "));
    assert_eq!(
        follow(&[field.clone().into_bytes()]),
        Ok(Some("/v1/items?page=2".to_owned()))
    );
    // Without a next link, the list ends; one bad entry at the end is still found.
    assert_eq!(follow(&[items.join(", ").into_bytes()]), Ok(None));
    let broken = format!("{last}, {}, <open", items.join(", "));
    assert_eq!(follow(&[broken.into_bytes()]), Err(MISMATCH.to_owned()));
    // Ten thousand fields of one entry each, the next link last.
    let mut fields: Vec<Vec<u8>> = items.iter().map(|item| item.clone().into_bytes()).collect();
    fields.push(last.as_bytes().to_vec());
    assert_eq!(follow(&fields), Ok(Some("/v1/items?page=2".to_owned())));
}
