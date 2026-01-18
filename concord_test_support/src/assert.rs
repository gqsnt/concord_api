use crate::mock::RecordedRequest;
use http::header::HeaderName;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

pub struct RequestAssert<'a> {
    req: &'a RecordedRequest,
}

pub fn assert_request(req: &RecordedRequest) -> RequestAssert<'_> {
    RequestAssert { req }
}

impl<'a> RequestAssert<'a> {
    pub fn page_index(self, expected: u32) -> Self {
        let got = self.req.meta.page_index;
        if got != expected {
            panic!(
                "page_index mismatch\n  expected: {expected}\n  got: {got}\n  url: {}",
                self.req.url
            );
        }
        self
    }

    pub fn host(self, expected: &str) -> Self {
        let got = self.req.url.host_str().unwrap_or("");
        if got != expected {
            panic!(
                "host mismatch\n  expected: {expected}\n  got: {got}\n  url: {}",
                self.req.url
            );
        }
        self
    }

    pub fn path(self, expected: &str) -> Self {
        let got = self.req.url.path();
        if got != expected {
            panic!(
                "path mismatch\n  expected: {expected}\n  got: {got}\n  url: {}",
                self.req.url
            );
        }
        self
    }

    pub fn timeout(self, expected: Option<std::time::Duration>) -> Self {
        let got = self.req.timeout;
        if got != expected {
            panic!(
                "timeout mismatch\n  expected: {:?}\n  got: {:?}\n  url: {}",
                expected, got, self.req.url
            );
        }
        self
    }

    pub fn body_present(self) -> Self {
        if self.req.body.is_none() {
            panic!("expected body present, but body=None\nurl: {}", self.req.url);
        }
        self
    }

    pub fn body_absent(self) -> Self {
        if self.req.body.is_some() {
            panic!("expected body absent, but body=Some(..)\nurl: {}", self.req.url);
        }
        self
    }

    pub fn header(self, name: impl IntoHeaderName, expected: &str) -> Self {
        let name = name.into_header_name();
        let got = self.req.headers.get(&name).and_then(|v| v.to_str().ok());
        match got {
            Some(v) if v == expected => {}
            Some(v) => {
                panic!(
                    "header mismatch\n  header: {}\n  expected: {}\n  got: {}\n  url: {}",
                    name, expected, v, self.req.url
                );
            }
            None => {
                panic!(
                    "missing header\n  header: {}\n  expected: {}\n  url: {}",
                    name, expected, self.req.url
                );
            }
        }
        self
    }

    pub fn header_absent(self, name: impl IntoHeaderName) -> Self {
        let name = name.into_header_name();
        if self.req.headers.contains_key(&name) {
            let got = self.req.headers.get(&name).and_then(|v| v.to_str().ok());
            panic!(
                "expected header absent\n  header: {}\n  got: {:?}\n  url: {}",
                name, got, self.req.url
            );
        }
        self
    }

    pub fn query_has(self, key: &str, expected_value: &str) -> Self {
        let pairs = self.query_pairs();
        if !pairs.iter().any(|(k, v)| k == key && v == expected_value) {
            panic!(
                "missing query pair\n  expected: {}={}\n  got: {}\n  url: {}",
                key,
                expected_value,
                format_pairs(&pairs),
                self.req.url
            );
        }
        self
    }

    pub fn query_absent(self, key: &str) -> Self {
        let pairs = self.query_pairs();
        if pairs.iter().any(|(k, _)| k == key) {
            panic!(
                "expected query key absent\n  key: {}\n  got: {}\n  url: {}",
                key,
                format_pairs(&pairs),
                self.req.url
            );
        }
        self
    }

    pub fn query_values(self, key: &str, expected: &[&str]) -> Self {
        let pairs = self.query_pairs();
        let got: Vec<String> = pairs
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .collect();
        let exp: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        if got != exp {
            panic!(
                "query values mismatch\n  key: {}\n  expected: {:?}\n  got: {:?}\n  all: {}\n  url: {}",
                key,
                exp,
                got,
                format_pairs(&pairs),
                self.req.url
            );
        }
        self
    }

    pub fn query_keys_exact(self, expected_keys: &[&str]) -> Self {
        let pairs = self.query_pairs();
        let got: BTreeSet<String> = pairs.iter().map(|(k, _)| k.clone()).collect();
        let exp: BTreeSet<String> = expected_keys.iter().map(|s| s.to_string()).collect();
        if got != exp {
            panic!(
                "query key-set mismatch\n  expected: {:?}\n  got: {:?}\n  all: {}\n  url: {}",
                exp,
                got,
                format_pairs(&pairs),
                self.req.url
            );
        }
        self
    }

    pub fn debug_dump(self) -> Self {
        eprintln!("{:#?}", self.req);
        self
    }

    fn query_pairs(&self) -> Vec<(String, String)> {
        self.req
            .url
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[allow(dead_code)]
    pub fn query_multimap(&self) -> BTreeMap<String, Vec<String>> {
        let mut mm: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (k, v) in self.query_pairs() {
            mm.entry(k).or_default().push(v);
        }
        mm
    }
}

pub trait IntoHeaderName {
    fn into_header_name(self) -> HeaderName;
}

impl IntoHeaderName for HeaderName {
    fn into_header_name(self) -> HeaderName {
        self
    }
}

impl IntoHeaderName for &'static HeaderName {
    fn into_header_name(self) -> HeaderName {
        self.clone()
    }
}

impl IntoHeaderName for &'static str {
    fn into_header_name(self) -> HeaderName {
        HeaderName::from_bytes(self.as_bytes()).unwrap_or_else(|_| {
            panic!("invalid header name literal: {:?}", self);
        })
    }
}

fn format_pairs(pairs: &[(String, String)]) -> String {
    let mut s = String::new();
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        let _ = write!(s, "{}={}", k, v);
    }
    s
}
