//! C8.6 arbitrary Link octets and continuation URLs.
#![allow(clippy::unwrap_used)]
use concord_core::{
    __private::{Core, Draft, Options, Piece},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    page::Call,
    Answer, Error,
};
use concord_extras::page::{by_link, by_next_url};
use proptest::prelude::*;

#[derive(Clone, Debug, Default)]
struct NextCall(String);
impl Call for NextCall {
    type Value = String;
    type Problem = ();
    fn at_url(self, url: &str) -> Self {
        Self(url.to_owned())
    }
}
fn build(call: NextCall) -> Result<concord_core::reqwest::Request, Error> {
    let mut draft = Draft::new(Method::GET);
    draft.base(&[Piece::Lit("https://example.test/v1")]);
    let mut options = Options::default();
    options.at_url(&call.0);
    draft.finish(&Core::default(), &options, "props.next")
}
proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn c8_6_link_arbitrary_bytes_and_next_url_text_never_panic(
        fields in proptest::collection::vec(proptest::collection::vec(any::<u8>(),0..256),0..8),
        url in proptest::collection::vec(any::<char>(),0..128).prop_map(|v|v.into_iter().collect::<String>()),
    ){
        let mut headers = HeaderMap::new();
        for field in fields {
            if let Ok(v) = HeaderValue::from_bytes(&field) {
                headers.append("link", v);
            }
        }
        let answer = Answer::new(StatusCode::OK, headers.into(), url.clone());
        if let Ok(Some(next)) = by_link()(NextCall::default(), &answer) {
            prop_assert!(!next.0.contains('#'));
            if let Err(e) = build(next) {
                prop_assert_eq!(e.kind(), concord_core::error::Kind::Build);
            }
        }
        let next = by_next_url(|v: &String| Some(v.clone()))(NextCall::default(), &answer).unwrap();
        prop_assert_eq!(next.0.as_str(), url.split('#').next().unwrap());
        if let Ok(request) = build(next) {
            prop_assert_eq!(
                request.url().origin(),
                "https://example.test"
                    .parse::<concord_core::reqwest::Url>()
                    .unwrap()
                    .origin()
            );
            prop_assert_eq!(request.url().fragment(), None);
        }
    }

    #[test]
    fn c8_6_link_next_preserves_quoted_delimiters_and_ignores_anchors(
        target in "[a-z0-9/_?=&%|]{1,50}",label in "[a-z0-9;, ]{0,24}",anchor in any::<bool>(),
    ){
        let link = format!(
            "</{target}>; title=\"{label}\"; rel=\"prev next\"{}",
            if anchor { "; anchor=\"/context\"" } else { "" }
        );
        let mut headers = HeaderMap::new();
        headers.append("link", HeaderValue::from_str(&link).unwrap());
        let answer = Answer::new(StatusCode::OK, headers.into(), String::new());
        let found = by_link()(NextCall::default(), &answer).unwrap();
        prop_assert_eq!(found.map(|c| c.0), (!anchor).then(|| format!("/{target}")));
    }
}
