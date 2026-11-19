//! C11.11: `transfer::at` accepts only a URL that is sent as it is written, for every text.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use concord_core::{
    __private::{Core, Draft, Options, Piece},
    http::{Method, Uri},
    page::Call,
    reqwest::Url,
};
use concord_extras::transfer::at;
use proptest::prelude::*;
use std::error::Error as _;

#[derive(Clone, Debug, Default, PartialEq)]
struct Upload(Option<String>);
impl Call for Upload {
    type Value = ();
    type Problem = ();
    fn at_url(self, url: &str) -> Self {
        Self(Some(url.to_owned()))
    }
}

/// Everything an error prints, its causes included.
fn printed(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = format!("{error} {error:?}");
    let mut next = error.source();
    while let Some(cause) = next {
        text.push_str(&format!(" {cause} {cause:?}"));
        next = cause.source();
    }
    text
}

/// The URL that the request builder sends for an upload call given `url`.
fn sent(url: &str) -> Option<String> {
    let mut draft = Draft::new(Method::PUT);
    draft.base(&[Piece::Lit("https://storage.test/")]);
    let mut options = Options::default();
    options.at_url(url);
    let request = draft
        .finish(&Core::<()>::default(), &options, "props.upload")
        .ok()?;
    Some(request.url().as_str().to_owned())
}

/// Checks what C11.11 promises of one text, whatever it is.
fn check(url: &str) -> Result<bool, TestCaseError> {
    match at(Upload::default(), url) {
        Ok(call) => {
            prop_assert_eq!(call.0.as_deref(), Some(url));
            prop_assert!(url.len() <= 65_534);
            prop_assert!(!url.contains('#'));
            let parsed = Url::parse(url).unwrap();
            prop_assert!(matches!(parsed.scheme(), "http" | "https"));
            prop_assert!(parsed.has_host());
            prop_assert!(parsed.username().is_empty() && parsed.password().is_none());
            prop_assert_eq!(parsed.as_str(), url);
            prop_assert_eq!(url.parse::<Uri>().unwrap().to_string(), url);
            // At its own origin, the request goes out byte for byte.
            if parsed.origin() == Url::parse("https://storage.test/").unwrap().origin() {
                let sent = sent(url);
                prop_assert_eq!(sent.as_deref(), Some(url));
            }
            Ok(true)
        }
        Err(error) => {
            // The error never holds the URL.
            if url.len() >= 12 {
                prop_assert!(!printed(&error).contains(url));
            }
            Ok(false)
        }
    }
}

fn path_text() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            4 => "[A-Za-z0-9._~!$&()*+,;=:@-]{1,8}".prop_filter("not a dot segment", |s| s != "." && s != ".."),
            1 => prop::sample::select(vec!["%2F", "%2f", "%41", "a%20b", ""]).prop_map(str::to_owned),
        ],
        0..5,
    )
    .prop_map(|segments| format!("/{}", segments.join("/")))
}
fn query_text() -> impl Strategy<Value = Option<String>> {
    prop::option::of(prop_oneof![
        3 => "[A-Za-z0-9._~!$&()*+,;=:@/?-]{0,24}",
        1 => prop::sample::select(vec!["", "&", "&&", "a=1&a=2", "sig=%2F%2b+", "bare", "eq=a=b=="]).prop_map(str::to_owned),
    ])
}
/// A URL as a storage service returns one: already in the form that parsing keeps.
fn kept_url() -> impl Strategy<Value = String> {
    (
        prop::sample::select(vec!["https", "http"]),
        prop::sample::select(vec![
            "storage.test",
            "uploads.example.test",
            "127.0.0.1",
            "[::1]",
        ]),
        prop::sample::select(vec!["", ":8443", ":1"]),
        path_text(),
        query_text(),
    )
        .prop_map(|(scheme, host, port, path, query)| {
            let mut url = format!("{scheme}://{host}{port}{path}");
            if let Some(query) = query {
                url.push('?');
                url.push_str(&query);
            }
            url
        })
}
/// One change that C11.11 refuses, with the words of its refusal.
fn refused(url: &str, how: u8) -> (String, &'static str) {
    let (scheme, rest) = url.split_once("://").unwrap();
    match how % 9 {
        0 => (
            format!("{}://{rest}", scheme.to_uppercase()),
            "transfer URL spelling would change",
        ),
        1 => (
            format!("ftp://{rest}"),
            "transfer URL must use HTTP or HTTPS",
        ),
        2 => (
            format!("{scheme}://user@{rest}"),
            "transfer URL must omit user information and fragments",
        ),
        3 => (
            format!("{url}#"),
            "transfer URL must omit user information and fragments",
        ),
        4 => (
            rest.split_once('/')
                .map_or("/".into(), |(_, path)| format!("/{path}")),
            "transfer URL must have an absolute authority",
        ),
        5 => (
            format!(
                "{scheme}://{}",
                rest.replacen("storage.test", "STORAGE.test", 1)
                    .replacen("127.0.0.1", "127.1", 1)
                    .replacen("[::1]", "[0::1]", 1)
                    .replacen("uploads.", "Uploads.", 1)
            ),
            "transfer URL spelling would change",
        ),
        6 => {
            // A dot segment in the path, which parsing removes.
            let (path, query) = url
                .split_once('?')
                .map_or((url, None), |(p, q)| (p, Some(q)));
            let query = query.map_or(String::new(), |q| format!("?{q}"));
            (
                format!("{path}/./x{query}"),
                "transfer URL spelling would change",
            )
        }
        7 => (format!(" {url}"), "transfer URL spelling would change"),
        _ => {
            let mut long = format!("{url}{}", if url.contains('?') { "&" } else { "?" });
            while long.len() < 65_535 {
                long.push('a');
            }
            (long, "transfer URL exceeds 65,534 bytes")
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn c11_11_a_url_that_keeps_its_spelling_is_accepted_and_sent_byte_for_byte(url in kept_url()) {
        prop_assert!(check(&url)?, "refused {:?}", url);
    }

    #[test]
    fn c11_11_each_listed_refusal_has_its_own_error(url in kept_url(), how in any::<u8>()) {
        let (changed, words) = refused(&url, how);
        prop_assert!(!check(&changed)?);
        prop_assert_eq!(at(Upload::default(), &changed).unwrap_err().to_string(), words, "{:?}", changed);
    }

    #[test]
    fn c11_11_any_text_is_accepted_only_as_it_is_sent(
        url in prop_oneof![
            2 => proptest::collection::vec(any::<char>(), 0..64).prop_map(|c| c.into_iter().collect::<String>()),
            3 => (kept_url(), any::<prop::sample::Index>(), prop::sample::select(vec![
                "\\", "\t", "\n", " ", "%", "%0", "#", "@", ":", "//", "..", "é", "\u{1}", "[", "]", "?", "`", "'", "\"", "<",
            ])).prop_map(|(url, at, piece)| {
                let at = at.index(url.len() + 1);
                let at = (0..=at).rev().find(|n| url.is_char_boundary(*n)).unwrap_or(0);
                format!("{}{piece}{}", &url[..at], &url[at..])
            }),
        ],
    ) {
        check(&url)?;
    }
}

#[test]
fn c11_11_the_byte_bound_is_65_534_bytes() {
    let base = "https://storage.test/upload?k=";
    let at_bound = format!("{base}{}", "a".repeat(65_534 - base.len()));
    assert_eq!(at_bound.len(), 65_534);
    assert_eq!(
        at(Upload::default(), &at_bound).unwrap().0.as_deref(),
        Some(at_bound.as_str())
    );
    let over = format!("{at_bound}a");
    assert_eq!(
        at(Upload::default(), &over).unwrap_err().to_string(),
        "transfer URL exceeds 65,534 bytes"
    );
    // The bound counts bytes: 32,767 two-byte characters and one more byte.
    let wide = format!("{}a", "é".repeat(32_767));
    assert_eq!(wide.len(), 65_535);
    assert!(at(Upload::default(), &wide).unwrap_err().source().is_none());
}
