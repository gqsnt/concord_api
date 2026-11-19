//! `transfer::at`: any text; an accepted URL keeps its spelling as a URL and as a URI.
#![no_main]

use concord_core::http::Uri;
use concord_core::page::Call;
use concord_core::reqwest::Url;
use concord_extras::transfer::at;
use libfuzzer_sys::fuzz_target;

#[derive(Clone)]
struct Upload(Option<String>);

/// Everything an error can print: fixed words, and no part of the URL.
const WORDS: [&str; 7] = [
    "transfer URL could not be parsed",
    "transfer URL is not an HTTP URI",
    "transfer URL must have an absolute authority",
    "transfer URL must use HTTP or HTTPS",
    "transfer URL must omit user information and fragments",
    "transfer URL spelling would change",
    "transfer URL exceeds 65,534 bytes",
];
const NAMES: [&str; 7] = [
    "InvalidUrl",
    "InvalidUri",
    "NotAbsolute",
    "UnsupportedScheme",
    "ForbiddenUrlPart",
    "ChangedUrl",
    "UrlTooLong",
];

impl Call for Upload {
    type Value = ();
    type Problem = ();
    fn at_url(self, url: &str) -> Self {
        Self(Some(url.to_owned()))
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(url) = std::str::from_utf8(data) else {
        return;
    };
    match at(Upload(None), url) {
        Ok(call) => {
            assert_eq!(call.0.as_deref(), Some(url));
            assert!(url.len() <= 65_534 && !url.contains('#'));
            let parsed = Url::parse(url).expect("an accepted URL parses");
            assert_eq!(parsed.as_str(), url);
            assert!(matches!(parsed.scheme(), "http" | "https"));
            assert!(parsed.username().is_empty() && parsed.password().is_none());
            assert_eq!(
                url.parse::<Uri>()
                    .expect("an accepted URL is a URI")
                    .to_string(),
                url
            );
        }
        Err(error) => {
            assert!(WORDS.contains(&error.to_string().as_str()));
            assert!(NAMES.contains(&format!("{error:?}").as_str()));
        }
    }
});
