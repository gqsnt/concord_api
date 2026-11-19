//! `by_next_url` and the request builder: any next URL, for a call with and without a
//! secret query entry.
#![no_main]

use concord_core::__private::{Core, Draft, Options, Piece};
use concord_core::http::{HeaderMap, Method, StatusCode};
use concord_core::page::{Call, Pager};
use concord_core::reqwest::Url;
use concord_core::{Answer, Secret};
use concord_extras::page::by_next_url;
use libfuzzer_sys::fuzz_target;

#[derive(Clone, Default)]
struct Next(Option<String>);
impl Call for Next {
    type Value = String;
    type Problem = ();
    fn at_url(self, url: &str) -> Self {
        Self(Some(url.to_owned()))
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((&secret, text)) = data.split_first() else {
        return;
    };
    let Ok(text) = std::str::from_utf8(text) else {
        return;
    };
    let answer = Answer::new(StatusCode::OK, HeaderMap::new().into(), text.to_owned());
    let next = by_next_url(|url: &String| Some(url.clone()))
        .next_call(Next::default(), &answer)
        .expect("by_next_url never fails")
        .expect("a URL was given");
    let next = next.0.expect("a next call has a URL");
    assert!(!next.contains('#'));
    let mut draft = Draft::new(Method::GET);
    draft.base(&[Piece::Lit("https://example.test/v1")]);
    let token = Secret::new("hidden");
    if secret % 2 == 1 {
        draft.query("token", &[Piece::Val("token", &token)]);
    }
    let mut options = Options::default();
    options.at_url(&next);
    if let Ok(request) = draft.finish(&Core::<()>::default(), &options, "fuzz.next") {
        let base = Url::parse("https://example.test/").expect("a valid base");
        assert_eq!(request.url().origin(), base.origin());
        assert_eq!(request.url().fragment(), None);
        if secret % 2 == 1 {
            assert!(request
                .url()
                .query()
                .is_some_and(|query| query.ends_with("token=hidden")));
        }
    }
});
