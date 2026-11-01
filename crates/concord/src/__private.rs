//! Support code for `api!` expansions. Not a public API: names and signatures may change in any
//! release.

use std::fmt::{self, Display, Write as _};

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC};
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use reqwest::{Method, Url};

use crate::error::Error;
use crate::prepared::Prepared;
use crate::repr::{Decode, Encode};

pub use crate::value::IntoValue;
pub use concord_macros::api as api_impl;

/// Static facts about one endpoint, produced by the macro.
#[derive(Debug)]
pub struct EndpointMeta {
    /// `Client.scope.endpoint`.
    pub name: &'static str,
    /// The HTTP method, as written.
    pub method: &'static str,
    /// Number of generated host labels (they are the leftmost labels).
    pub labels: usize,
    /// Indices (from the left) of generated labels that contain a sensitive value.
    pub sensitive_labels: &'static [usize],
    /// Number of generated path segments (they are the last segments; a trailing slash counts).
    pub segments: usize,
    /// Indices of generated segments that contain a sensitive value.
    pub sensitive_segments: &'static [usize],
    /// Query keys whose values are withheld from Concord output.
    pub sensitive_query: &'static [&'static str],
}

/// A validated base URL: `http`/`https`, a host, an optional port and path, nothing else.
#[derive(Clone)]
pub struct Base {
    url: Url,
    /// The base path with at most one trailing slash removed; generated segments follow it.
    prefix: String,
}

impl Base {
    /// Validates a base URL supplied at runtime.
    pub fn parse(input: &str) -> Result<Self, Error> {
        let fail = |reason| Error::build(None, "base URL", reason);
        let url = Url::parse(input).map_err(|_| fail("is not an absolute URL"))?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(fail("must use http or https"));
        }
        if url.host().is_none() {
            return Err(fail("must have a host"));
        }
        if url.port() == Some(0) {
            return Err(fail("must not use port 0"));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(fail("must not contain user information"));
        }
        if url.query().is_some() {
            return Err(fail("must not contain a query"));
        }
        if url.fragment().is_some() {
            return Err(fail("must not contain a fragment"));
        }
        let path = url.path();
        let prefix = path.strip_suffix('/').unwrap_or(path).to_owned();
        Ok(Self { url, prefix })
    }

    /// The base declared in `api!`, already validated by the macro.
    pub fn from_static(input: &'static str) -> Self {
        match Self::parse(input) {
            Ok(base) => base,
            Err(e) => panic!("api! accepted an invalid base URL {input:?}: {e}"),
        }
    }

    /// The validated URL.
    pub fn url(&self) -> &Url {
        &self.url
    }
}

impl fmt::Debug for Base {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.url.as_str(), f)
    }
}

/// Everything except RFC 3986 unreserved characters (`A-Z a-z 0-9 - . _ ~`).
const PATH_VALUE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

struct EncodeInto<'a>(&'a mut String);

impl fmt::Write for EncodeInto<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0
            .extend(percent_encoding::utf8_percent_encode(s, PATH_VALUE));
        Ok(())
    }
}

/// Appends a value to a path segment, percent-encoding everything but unreserved characters.
pub fn encode_segment<T: Display + ?Sized>(out: &mut String, value: &T) {
    let _ = write!(EncodeInto(out), "{value}");
}

/// Appends a value's `Display` text.
pub fn display_into<T: Display + ?Sized>(out: &mut String, value: &T) {
    let _ = write!(out, "{value}");
}

/// Encodes a body with its declared representation.
/// Encodes a body with its declared representation and validates the representation's content
/// type, so both failures belong to the body stage (before any URL part is formatted).
pub fn encode_body<E: Encode>(
    value: E::Value,
    meta: &'static EndpointMeta,
) -> Result<(reqwest::Body, Option<HeaderValue>), Error> {
    let content_type = match E::CONTENT_TYPE {
        None => None,
        Some(text) => {
            let visible = text
                .bytes()
                .all(|b| b == b'\t' || (0x20..0x7f).contains(&b));
            let value = HeaderValue::from_str(text)
                .ok()
                .filter(|_| visible)
                .ok_or_else(|| {
                    Error::build(
                        Some(meta),
                        "content-type",
                        "from the body representation is not a valid header value",
                    )
                })?;
            Some(value)
        }
    };
    let body = E::encode(value).map_err(|e| Error::from_encode(meta, e))?;
    Ok((body, content_type))
}

/// The request being composed by one endpoint's generated code.
pub struct Parts<'a> {
    meta: &'static EndpointMeta,
    base: &'a Base,
    url: Url,
    labels: Vec<String>,
    segments: Vec<String>,
    trailing_slash: bool,
    query: Vec<(&'static str, String)>,
    headers: HeaderMap,
}

impl<'a> Parts<'a> {
    /// Starts composing a request for `meta` against `base`.
    pub fn new(meta: &'static EndpointMeta, base: &'a Base) -> Self {
        Self {
            meta,
            base,
            url: base.url.clone(),
            labels: Vec::new(),
            segments: Vec::new(),
            trailing_slash: false,
            query: Vec::new(),
            headers: HeaderMap::new(),
        }
    }

    /// Adds the next host label (leftmost first). `what` names the template for errors.
    pub fn label(&mut self, mut label: String, what: &'static str) -> Result<(), Error> {
        label.make_ascii_lowercase();
        let bytes = label.as_bytes();
        let valid = !bytes.is_empty()
            && bytes.len() <= 63
            && bytes
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
            && bytes[0] != b'-'
            && bytes[bytes.len() - 1] != b'-';
        if !valid {
            return Err(Error::build(
                Some(self.meta),
                what,
                "is not a DNS label (1-63 ASCII letters, digits or interior hyphens)",
            ));
        }
        self.labels.push(label);
        Ok(())
    }

    /// Adds a path segment containing values (already encoded).
    pub fn segment(&mut self, segment: String, what: &'static str) -> Result<(), Error> {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(Error::build(Some(self.meta), what, "is empty, `.` or `..`"));
        }
        self.segments.push(segment);
        Ok(())
    }

    /// Adds a literal path segment (validated by the macro).
    pub fn segment_static(&mut self, segment: &'static str) {
        self.segments.push(segment.to_owned());
    }

    /// Ends the path with `/`.
    pub fn trailing_slash(&mut self) {
        self.trailing_slash = true;
    }

    /// Adds one query occurrence.
    pub fn query<T: Display + ?Sized>(&mut self, key: &'static str, value: &T) {
        self.query.push((key, value.to_string()));
    }

    /// Adds one query occurrence per item, in iteration order.
    pub fn query_each<I>(&mut self, key: &'static str, values: I)
    where
        I: IntoIterator,
        I::Item: Display,
    {
        for value in values {
            self.query.push((key, value.to_string()));
        }
    }

    /// Adds one header line. `name` is a lowercase field name validated by the macro.
    pub fn header<T: Display + ?Sized>(
        &mut self,
        name: &'static str,
        value: &T,
        sensitive: bool,
    ) -> Result<(), Error> {
        let text = value.to_string();
        if !text
            .bytes()
            .all(|b| b == b'\t' || (0x20..0x7f).contains(&b))
        {
            return Err(Error::build(
                Some(self.meta),
                name,
                "header value contains a control or non-ASCII character",
            ));
        }
        let mut value = HeaderValue::from_str(&text)
            .map_err(|_| Error::build(Some(self.meta), name, "header value is invalid"))?;
        value.set_sensitive(sensitive);
        self.headers.append(HeaderName::from_static(name), value);
        Ok(())
    }

    /// Adds one header line per item, in iteration order.
    pub fn header_each<I>(
        &mut self,
        name: &'static str,
        values: I,
        sensitive: bool,
    ) -> Result<(), Error>
    where
        I: IntoIterator,
        I::Item: Display,
    {
        for value in values {
            self.header(name, &value, sensitive)?;
        }
        Ok(())
    }

    /// Prepends the labels to the base host and validates the result. Called once, right after
    /// the last label, so host errors precede path errors.
    pub fn host(&mut self) -> Result<(), Error> {
        if !self.labels.is_empty() {
            let domain = match self.url.host() {
                Some(url::Host::Domain(domain)) => domain,
                _ => {
                    return Err(Error::build(
                        Some(self.meta),
                        "subdomain",
                        "requires a base URL with a domain name, not an IP address",
                    ));
                }
            };
            let host = format!("{}.{}", self.labels.join("."), domain);
            if host.len() > 253 {
                return Err(Error::build(
                    Some(self.meta),
                    "host",
                    "is longer than 253 bytes",
                ));
            }
            self.url
                .set_host(Some(&host))
                .map_err(|_| Error::build(Some(self.meta), "host", "is not a valid host name"))?;
        }
        Ok(())
    }

    /// Assembles the native request.
    pub fn finish<R: Decode>(
        self,
        method: Method,
        client: &reqwest::Client,
        body: Option<(reqwest::Body, Option<HeaderValue>)>,
    ) -> Result<Prepared<R>, Error> {
        let mut url = self.url;
        if !self.segments.is_empty() || self.trailing_slash {
            let mut path = self.base.prefix.clone();
            for segment in &self.segments {
                path.push('/');
                path.push_str(segment);
            }
            if self.trailing_slash {
                path.push('/');
            }
            url.set_path(&path);
        }
        if !self.query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in &self.query {
                pairs.append_pair(key, value);
            }
        }
        let mut request = reqwest::Request::new(method, url);
        let mut headers = self.headers;
        if let Some((body, content_type)) = body {
            if let Some(content_type) = content_type {
                if !headers.contains_key(CONTENT_TYPE) {
                    headers.insert(CONTENT_TYPE, content_type);
                }
            }
            *request.body_mut() = Some(body);
        }
        *request.headers_mut() = headers;
        Ok(Prepared::new(client.clone(), request, self.meta))
    }
}

/// Renders a URL built by `meta`'s endpoint with sensitive placements replaced by `<redacted>`.
pub(crate) fn redacted_url(url: &Url, meta: &EndpointMeta) -> String {
    const REDACTED: &str = "<redacted>";
    let mut out = String::new();
    out.push_str(url.scheme());
    out.push_str("://");
    if let Some(host) = url.host_str() {
        if meta.sensitive_labels.is_empty() {
            out.push_str(host);
        } else {
            for (i, label) in host.split('.').enumerate() {
                if i > 0 {
                    out.push('.');
                }
                out.push_str(if i < meta.labels && meta.sensitive_labels.contains(&i) {
                    REDACTED
                } else {
                    label
                });
            }
        }
    }
    if let Some(port) = url.port() {
        let _ = write!(out, ":{port}");
    }
    let path = url.path();
    if meta.sensitive_segments.is_empty() {
        out.push_str(path);
    } else {
        let pieces: Vec<&str> = path.split('/').collect();
        // `pieces[0]` is the empty string before the leading slash.
        let first_generated = pieces.len().saturating_sub(meta.segments);
        for (i, piece) in pieces.iter().enumerate() {
            if i > 0 {
                out.push('/');
            }
            let generated = i.checked_sub(first_generated);
            let hide = generated.is_some_and(|g| meta.sensitive_segments.contains(&g));
            out.push_str(if hide { REDACTED } else { piece });
        }
    }
    if let Some(query) = url.query() {
        out.push('?');
        for (i, pair) in query.split('&').enumerate() {
            if i > 0 {
                out.push('&');
            }
            let raw_key = pair.split('=').next().unwrap_or("");
            let key = url::form_urlencoded::parse(raw_key.as_bytes())
                .next()
                .map(|(k, _)| k);
            let sensitive = key.is_some_and(|k| meta.sensitive_query.contains(&k.as_ref()));
            if sensitive {
                out.push_str(raw_key);
                out.push('=');
                out.push_str(REDACTED);
            } else {
                out.push_str(pair);
            }
        }
    }
    out
}

/// `Debug` through `Display`, for generated `Debug` impls.
pub struct DebugDisplay<'a, T: ?Sized>(pub &'a T);

impl<T: Display + ?Sized> fmt::Debug for DebugDisplay<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0.to_string(), f)
    }
}

/// `Debug` of an `each` value: the `Display` text of every item.
pub struct DebugEach<'a, T: ?Sized>(pub &'a T);

impl<'a, T: ?Sized> fmt::Debug for DebugEach<'a, T>
where
    &'a T: IntoIterator,
    <&'a T as IntoIterator>::Item: Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.0.into_iter().map(|item| item.to_string()))
            .finish()
    }
}

/// `Debug` placeholder for sensitive values.
pub struct Redacted;

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}
