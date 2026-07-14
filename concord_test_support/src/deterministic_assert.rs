use crate::deterministic::RecordedExecution;
use http::header::HeaderName;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

pub struct ExecutionAssert<'a> {
    execution: &'a RecordedExecution,
}

pub fn assert_execution(execution: &RecordedExecution) -> ExecutionAssert<'_> {
    ExecutionAssert { execution }
}

impl ExecutionAssert<'_> {
    pub fn method(self, expected: http::Method) -> Self {
        assert_eq!(self.execution.method, expected, "execution method");
        self
    }

    pub fn host(self, expected: &str) -> Self {
        assert_eq!(
            self.execution.logical_url.host_str().unwrap_or(""),
            expected
        );
        self
    }

    pub fn path(self, expected: &str) -> Self {
        assert_eq!(self.execution.logical_url.path(), expected);
        self
    }

    pub fn body_present(self) -> Self {
        assert!(self.execution.body_present(), "expected request body");
        self
    }

    pub fn body_absent(self) -> Self {
        assert!(!self.execution.body_present(), "expected no request body");
        self
    }

    pub fn header(self, name: impl RecordedHeaderName, expected: &str) -> Self {
        let name = name.into_header_name();
        assert!(
            !self.execution.protected_header_names.contains(&name),
            "header {name} is protected; use protected_header() and a scripted fake credential expectation"
        );
        assert_eq!(
            self.execution
                .headers
                .get(&name)
                .and_then(|v| v.to_str().ok()),
            Some(expected),
            "public execution header {name}"
        );
        self
    }

    pub fn protected_header(self, name: impl RecordedHeaderName) -> Self {
        let name = name.into_header_name();
        assert!(
            self.execution.protected_header_names.contains(&name),
            "missing protected execution header {name}"
        );
        self
    }

    pub fn header_absent(self, name: impl RecordedHeaderName) -> Self {
        let name = name.into_header_name();
        assert!(!self.execution.headers.contains_key(&name));
        self
    }

    pub fn query_has(self, key: &str, expected_value: &str) -> Self {
        assert!(
            self.query_pairs()
                .iter()
                .any(|(name, value)| name == key && value == expected_value),
            "missing public query pair"
        );
        self
    }

    pub fn query_absent(self, key: &str) -> Self {
        assert!(!self.query_pairs().iter().any(|(name, _)| name == key));
        self
    }

    pub fn query_values(self, key: &str, expected: &[&str]) -> Self {
        let actual = self
            .query_pairs()
            .into_iter()
            .filter_map(|(name, value)| (name == key).then_some(value))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "public query values");
        self
    }

    pub fn query_keys_exact(self, expected: &[&str]) -> Self {
        let actual = self
            .query_pairs()
            .into_iter()
            .map(|(name, _)| name)
            .collect::<BTreeSet<_>>();
        let expected = expected
            .iter()
            .map(|name| (*name).to_string())
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected, "public query keys");
        self
    }

    pub fn debug_dump(self) -> Self {
        eprintln!("{:#?}", self.execution);
        self
    }

    fn query_pairs(&self) -> Vec<(String, String)> {
        self.execution
            .logical_url
            .query_pairs()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect()
    }

    pub fn query_multimap(&self) -> BTreeMap<String, Vec<String>> {
        let mut map = BTreeMap::new();
        for (name, value) in self.query_pairs() {
            map.entry(name).or_insert_with(Vec::new).push(value);
        }
        map
    }
}

pub trait RecordedHeaderName {
    fn into_header_name(self) -> HeaderName;
}

impl RecordedHeaderName for HeaderName {
    fn into_header_name(self) -> HeaderName {
        self
    }
}

impl RecordedHeaderName for &'static HeaderName {
    fn into_header_name(self) -> HeaderName {
        self.clone()
    }
}

impl RecordedHeaderName for &'static str {
    fn into_header_name(self) -> HeaderName {
        HeaderName::from_bytes(self.as_bytes()).expect("valid assertion header name")
    }
}

#[allow(dead_code)]
fn format_pairs(pairs: &[(String, String)]) -> String {
    let mut output = String::new();
    for (index, (name, value)) in pairs.iter().enumerate() {
        if index != 0 {
            output.push_str(", ");
        }
        let _ = write!(output, "{name}={value}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deterministic::RecordedExecution;
    use concord_core::__development::CapturedBodyCategory;
    use http::{HeaderMap, HeaderName, HeaderValue, Method};

    fn execution(protected: bool) -> RecordedExecution {
        let mut headers = HeaderMap::new();
        headers.insert("x-public", HeaderValue::from_static("value"));
        RecordedExecution {
            sequence: 0,
            method: Method::GET,
            logical_url: "https://example.test/path".parse().unwrap(),
            headers,
            protected_header_names: if protected {
                vec![HeaderName::from_static("authorization")]
            } else {
                Vec::new()
            },
            body_category: CapturedBodyCategory::Empty,
            known_body_length: None,
            endpoint: None,
            page_index: None,
            timeout: None,
        }
    }

    #[test]
    fn public_header_is_exact() {
        assert_execution(&execution(false)).header("x-public", "value");
    }

    #[test]
    fn protected_header_is_name_only() {
        assert_execution(&execution(true)).protected_header(http::header::AUTHORIZATION);
    }

    #[test]
    #[should_panic(expected = "is protected")]
    fn protected_header_cannot_use_public_value_assertion() {
        assert_execution(&execution(true)).header(http::header::AUTHORIZATION, "secret");
    }
}
