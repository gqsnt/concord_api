//! `by_link`: any `Link` fields, one per line of the input.
#![no_main]

use concord_core::http::{HeaderMap, HeaderValue, StatusCode};
use concord_core::page::{Call, Pager};
use concord_core::Answer;
use concord_extras::page::by_link;
use libfuzzer_sys::fuzz_target;

#[derive(Clone, Default)]
struct Next(Option<String>);
impl Call for Next {
    type Value = ();
    type Problem = ();
    fn at_url(self, url: &str) -> Self {
        Self(Some(url.to_owned()))
    }
}

fuzz_target!(|data: &[u8]| {
    let mut headers = HeaderMap::new();
    for field in data.split(|byte| *byte == b'\n') {
        if let Ok(value) = HeaderValue::from_bytes(field) {
            headers.append("link", value);
        }
    }
    let answer = Answer::new(StatusCode::OK, headers.into(), ());
    if let Ok(Some(next)) = by_link().next_call(Next::default(), &answer) {
        let url = next.0.expect("a next call has a URL");
        assert!(!url.contains('#'));
        assert!(!url
            .bytes()
            .any(|b| b <= 0x20 || b == 0x7f || matches!(b, b'<' | b'>' | b'"' | b'\\')));
    }
});
