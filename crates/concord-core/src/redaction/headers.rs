use std::collections::HashSet;
use std::fmt;

use http::{HeaderMap, HeaderName, HeaderValue};

use super::names::is_sensitive_name;

/// A header value after applying Concord's central sanitization policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SanitizedHeaderValue<'a> {
    /// Non-sensitive valid UTF-8 text.
    Text(&'a str),
    /// A value hidden because its header name is sensitive.
    Redacted,
    /// A non-sensitive value that is not valid UTF-8.
    NonUtf8,
}

/// A borrowed, sanitized view of an HTTP header map.
#[derive(Clone, Copy)]
pub struct SanitizedHeaders<'a> {
    headers: &'a HeaderMap,
    sensitive: Option<&'a HashSet<HeaderName>>,
}

impl<'a> SanitizedHeaders<'a> {
    /// Creates a borrowed view via Concord's built-in sensitive-header policy.
    ///
    /// Runtime-owned views may additionally protect declaration-selected names.
    #[must_use]
    pub const fn new(headers: &'a HeaderMap) -> Self {
        Self {
            headers,
            sensitive: None,
        }
    }

    pub(crate) const fn with_sensitive(
        headers: &'a HeaderMap,
        sensitive: &'a HashSet<HeaderName>,
    ) -> Self {
        Self {
            headers,
            sensitive: Some(sensitive),
        }
    }

    /// Returns the number of values, including repeated values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.headers.len()
    }

    /// Returns whether the view contains no header values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }

    /// Returns whether at least one value exists for `name`.
    #[must_use]
    pub fn contains_key(&self, name: &HeaderName) -> bool {
        self.headers.contains_key(name)
    }

    /// Returns the first value for `name`, following `HeaderMap` semantics.
    #[must_use]
    pub fn get(&self, name: &HeaderName) -> Option<SanitizedHeaderValue<'a>> {
        self.headers
            .get(name)
            .map(|value| sanitize_value(name, value, self.sensitive))
    }

    /// Iterates over every header value while preserving repeated-value order.
    pub fn iter(&self) -> impl Iterator<Item = (&'a HeaderName, SanitizedHeaderValue<'a>)> + 'a {
        let sensitive = self.sensitive;
        self.headers
            .iter()
            .map(move |(name, value)| (name, sanitize_value(name, value, sensitive)))
    }
}

impl fmt::Debug for SanitizedHeaders<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_map().entries(self.iter()).finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OwnedSanitizedHeaderValue {
    Text(String),
    Redacted,
    NonUtf8,
}

impl OwnedSanitizedHeaderValue {
    fn as_borrowed(&self) -> SanitizedHeaderValue<'_> {
        match self {
            Self::Text(text) => SanitizedHeaderValue::Text(text),
            Self::Redacted => SanitizedHeaderValue::Redacted,
            Self::NonUtf8 => SanitizedHeaderValue::NonUtf8,
        }
    }
}

/// An owned header map that contains only safe text and structural markers.
pub struct SanitizedHeaderMap {
    entries: Vec<(HeaderName, OwnedSanitizedHeaderValue)>,
}

impl SanitizedHeaderMap {
    pub(crate) fn from_headers(headers: &HeaderMap) -> Self {
        Self::from_headers_with_sensitive(headers, None)
    }

    pub(crate) fn from_headers_with_sensitive(
        headers: &HeaderMap,
        sensitive: Option<&HashSet<HeaderName>>,
    ) -> Self {
        let entries = headers
            .iter()
            .map(|(name, value)| {
                let value = match sanitize_value(name, value, sensitive) {
                    SanitizedHeaderValue::Text(text) => {
                        OwnedSanitizedHeaderValue::Text(text.to_owned())
                    }
                    SanitizedHeaderValue::Redacted => OwnedSanitizedHeaderValue::Redacted,
                    SanitizedHeaderValue::NonUtf8 => OwnedSanitizedHeaderValue::NonUtf8,
                };
                (name.clone(), value)
            })
            .collect();
        Self { entries }
    }

    /// Returns the number of values, including repeated values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the map contains no header values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns whether at least one value exists for `name`.
    #[must_use]
    pub fn contains_key(&self, name: &HeaderName) -> bool {
        self.entries.iter().any(|(candidate, _)| candidate == name)
    }

    /// Returns the first value for `name`.
    #[must_use]
    pub fn get(&self, name: &HeaderName) -> Option<SanitizedHeaderValue<'_>> {
        self.entries
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| value.as_borrowed())
    }

    /// Iterates over every stored structural value in source-map order.
    pub fn iter(&self) -> impl Iterator<Item = (&HeaderName, SanitizedHeaderValue<'_>)> {
        self.entries
            .iter()
            .map(|(name, value)| (name, value.as_borrowed()))
    }
}

impl fmt::Debug for SanitizedHeaderMap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_map().entries(self.iter()).finish()
    }
}

fn sanitize_value<'a>(
    name: &HeaderName,
    value: &'a HeaderValue,
    extra_sensitive: Option<&HashSet<HeaderName>>,
) -> SanitizedHeaderValue<'a> {
    if is_sensitive_name(name.as_str())
        || extra_sensitive.is_some_and(|sensitive| sensitive.contains(name))
    {
        SanitizedHeaderValue::Redacted
    } else {
        match value.to_str() {
            Ok(text) => SanitizedHeaderValue::Text(text),
            Err(_) => SanitizedHeaderValue::NonUtf8,
        }
    }
}

#[cfg(test)]
mod tests {
    use http::header::{AUTHORIZATION, HeaderName};

    use super::*;

    const FIRST_SECRET: &str = "secret-sentinel-alpha-56f849";
    const SECOND_SECRET: &str = "secret-sentinel-beta-17c322";

    fn source_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.append("x-public", HeaderValue::from_static("first"));
        headers.append("x-public", HeaderValue::from_static("second"));
        headers.append(AUTHORIZATION, HeaderValue::from_static(FIRST_SECRET));
        headers.append(AUTHORIZATION, HeaderValue::from_static(SECOND_SECRET));
        headers.append(
            "x-binary",
            HeaderValue::from_bytes(&[0xff, 0xfe]).expect("binary header should be valid"),
        );
        headers
    }

    #[test]
    fn borrowed_headers_preserve_structure_and_sanitize_every_value() {
        let headers = source_headers();
        let view = SanitizedHeaders::new(&headers);
        let public = HeaderName::from_static("x-public");
        let binary = HeaderName::from_static("x-binary");

        assert_eq!(view.len(), 5);
        assert!(!view.is_empty());
        assert!(view.contains_key(&public));
        assert!(!view.contains_key(&HeaderName::from_static("x-absent")));
        assert_eq!(view.get(&public), Some(SanitizedHeaderValue::Text("first")));
        assert_eq!(
            view.get(&AUTHORIZATION),
            Some(SanitizedHeaderValue::Redacted)
        );
        assert_eq!(view.get(&binary), Some(SanitizedHeaderValue::NonUtf8));

        let public_values: Vec<_> = view
            .iter()
            .filter(|(name, _)| *name == public)
            .map(|(_, value)| value)
            .collect();
        assert_eq!(
            public_values,
            [
                SanitizedHeaderValue::Text("first"),
                SanitizedHeaderValue::Text("second")
            ]
        );
        assert_eq!(
            view.iter()
                .filter(|(name, _)| *name == AUTHORIZATION)
                .map(|(_, value)| value)
                .collect::<Vec<_>>(),
            [
                SanitizedHeaderValue::Redacted,
                SanitizedHeaderValue::Redacted
            ]
        );

        let debug = format!("{view:?}");
        assert!(!debug.contains(FIRST_SECRET));
        assert!(!debug.contains(SECOND_SECRET));
    }

    #[test]
    fn empty_borrowed_headers_report_empty_structure() {
        let headers = HeaderMap::new();
        let view = SanitizedHeaders::new(&headers);
        assert_eq!(view.len(), 0);
        assert!(view.is_empty());
        assert_eq!(view.iter().count(), 0);
    }

    #[test]
    fn owned_headers_copy_only_safe_or_structural_values() {
        let mut source = source_headers();
        let owned = SanitizedHeaderMap::from_headers(&source);
        source.clear();
        source.insert(
            AUTHORIZATION,
            HeaderValue::from_static("replacement-secret"),
        );
        drop(source);

        let public = HeaderName::from_static("x-public");
        let binary = HeaderName::from_static("x-binary");

        assert_eq!(owned.len(), 5);
        assert!(!owned.is_empty());
        assert!(owned.contains_key(&AUTHORIZATION));
        assert!(!owned.contains_key(&HeaderName::from_static("x-absent")));
        assert_eq!(
            owned.get(&public),
            Some(SanitizedHeaderValue::Text("first"))
        );
        assert_eq!(
            owned.get(&AUTHORIZATION),
            Some(SanitizedHeaderValue::Redacted)
        );
        assert_eq!(owned.get(&binary), Some(SanitizedHeaderValue::NonUtf8));
        assert_eq!(
            owned
                .iter()
                .filter(|(name, _)| *name == public)
                .map(|(_, value)| value)
                .collect::<Vec<_>>(),
            [
                SanitizedHeaderValue::Text("first"),
                SanitizedHeaderValue::Text("second")
            ]
        );
        assert_eq!(
            owned
                .iter()
                .filter(|(name, _)| *name == AUTHORIZATION)
                .map(|(_, value)| value)
                .collect::<Vec<_>>(),
            [
                SanitizedHeaderValue::Redacted,
                SanitizedHeaderValue::Redacted
            ]
        );

        for (name, value) in &owned.entries {
            if name == AUTHORIZATION {
                assert_eq!(value, &OwnedSanitizedHeaderValue::Redacted);
            }
            if let OwnedSanitizedHeaderValue::Text(text) = value {
                assert!(!text.contains(FIRST_SECRET));
                assert!(!text.contains(SECOND_SECRET));
            }
        }

        let visible: Vec<_> = owned.iter().map(|(_, value)| value).collect();
        assert!(!visible.iter().any(|value| {
            matches!(value, SanitizedHeaderValue::Text(text) if text.contains(FIRST_SECRET) || text.contains(SECOND_SECRET))
        }));
        let debug = format!("{owned:?}");
        assert!(!debug.contains(FIRST_SECRET));
        assert!(!debug.contains(SECOND_SECRET));
        assert!(!debug.contains("replacement-secret"));
    }

    #[test]
    fn empty_owned_headers_report_empty_structure() {
        let owned = SanitizedHeaderMap::from_headers(&HeaderMap::new());
        assert_eq!(owned.len(), 0);
        assert!(owned.is_empty());
        assert_eq!(owned.iter().count(), 0);
    }
}
