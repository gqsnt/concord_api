use std::borrow::Cow;
use std::fmt;
use std::fmt::Write as _;
use std::future::Future;
use std::ops::Range;

use super::value::{Shape, WriteFailure};
use super::{Core, Idempotent, Options, Prepared, RequestSecrets, Run, SecretUrl, Value};
use crate::answer::Answer;
use crate::base::Base;
use crate::body::{Body, IntoBody};
use crate::error::{BuildFailure, Error, ValuePlace};
use crate::transport::TryInfo;

// http::Uri's byte limit, also applied by reqwest. The boundary tests use its
// parser as the reference; checking the length here needs no parse or allocation.
const MAX_URL_LENGTH: usize = 65_534;

/// What a [`Value`] wrote. Debug never exposes the written text.
pub struct Written {
    value: Result<Shape, WriteFailure>,
    secret: bool,
}
impl Default for Written {
    fn default() -> Self {
        Self {
            value: Ok(Shape::Nothing),
            secret: false,
        }
    }
}
impl fmt::Debug for Written {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Written(..)")
    }
}
impl Written {
    /// Store the serializer's result and whether the adapter read a secret.
    pub(super) fn store(&mut self, value: Result<Shape, WriteFailure>, secret: bool) {
        self.value = value;
        self.secret = secret;
    }
    /// Take the three-shape result and its secret mark without exposing text in Debug.
    pub(super) fn into_parts(self) -> (Result<Shape, WriteFailure>, bool) {
        (self.value, self.secret)
    }
}

/// One piece of a template: fixed text, or the value of a hole.
pub enum Piece<'a> {
    /// Fixed text.
    Lit(&'static str),
    /// The input name (or expression's key), and its adapted value.
    Val(&'static str, &'a dyn Value),
}
impl fmt::Debug for Piece<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Piece::Lit(_) => f.write_str("Lit(..)"),
            Piece::Val(name, _) => f.debug_tuple("Val").field(name).field(&"..").finish(),
        }
    }
}
struct BuiltBase {
    url: reqwest::Url,
    secrets: SecretUrl,
}
struct PathPart {
    text: Cow<'static, str>,
    marks: Vec<Range<usize>>,
}
struct Entry {
    values: Shape,
    secret: bool,
}
struct NamedEntry {
    key: &'static str,
    value: Result<Entry, BuildFailure>,
}

/// A request while it is built, root first. Debug never exposes values.
pub struct Draft {
    method: http::Method,
    base: Option<Result<BuiltBase, BuildFailure>>,
    path: Vec<Result<PathPart, BuildFailure>>,
    trailing: bool,
    query: Vec<NamedEntry>,
    headers: Vec<NamedEntry>,
    body: Option<Result<Body, Box<dyn std::error::Error + Send + Sync>>>,
}
impl fmt::Debug for Draft {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Draft").finish_non_exhaustive()
    }
}
impl Draft {
    /// A request with this method.
    pub fn new(method: http::Method) -> Draft {
        Self {
            method,
            base: None,
            path: Vec::new(),
            trailing: false,
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
        }
    }
    /// A failed entry expression, retaining its name and the author's source.
    pub fn entry_error<P>(
        endpoint: &'static str,
        input: &'static str,
        source: Box<dyn std::error::Error + Send + Sync>,
    ) -> Error<P> {
        Error::entry_source(endpoint, input, source)
    }

    /// Render a body-entry template. An absent hole omits the entry, but never
    /// stops validation of later holes. Lists cannot occupy a template hole.
    /// Secret values are read through their adapter and never appear in errors.
    pub fn body_template<P>(
        endpoint: &'static str,
        pieces: &[Piece<'_>],
    ) -> Result<Option<String>, Error<P>> {
        let mut text = String::new();
        let mut absent = false;
        let mut failure = None;
        for piece in pieces {
            match piece {
                Piece::Lit(literal) => text.push_str(literal),
                Piece::Val(name, value) => match write(name, *value, ValuePlace::Body) {
                    Ok((Shape::One(value), _)) => text.push_str(&value),
                    Ok((Shape::Nothing, _)) => absent = true,
                    Ok((Shape::List(values), _)) if values.is_empty() => absent = true,
                    Ok(_) | Err(BuildFailure::Shape { .. }) => {
                        failure.get_or_insert(BuildFailure::Body);
                    }
                    Err(error) => {
                        failure.get_or_insert(error);
                    }
                },
            }
        }
        if let Some(failure) = failure {
            return Err(Error::build(endpoint, failure));
        }
        Ok((!absent).then_some(text))
    }

    /// Set the description's base URL from literal and adapted template pieces.
    pub fn base(&mut self, pieces: &[Piece<'_>]) {
        self.base = Some(template_base(pieces));
    }
    /// `base x;` with an input of the type [`Base`].
    pub fn base_from(&mut self, base: &Base) {
        self.base = Some(parsed_base(base));
    }
    /// One segment of a `path`. A final literal `/` records a trailing separator.
    /// An empty literal alone does nothing.
    pub fn segment(&mut self, pieces: &[Piece<'_>]) {
        if matches!(pieces, [Piece::Lit("")]) {
            return;
        }
        self.trailing = false;
        let trailing = matches!(pieces.last(), Some(Piece::Lit(text)) if text.ends_with('/'));
        self.path.push(path_piece(pieces, trailing));
        self.trailing = trailing;
    }
    /// A `{*x}` hole: a value that is itself a path, including its empty pieces.
    pub fn segments(&mut self, name: &'static str, value: &dyn Value) {
        self.trailing = false;
        let rendered = single(name, value, ValuePlace::Path).and_then(|(text, secret)| {
            if text.is_empty() || text.split('/').any(|part| part == "." || part == "..") {
                return Err(BuildFailure::Path(name));
            }
            let mut joined = String::new();
            let mut marks = Vec::new();
            for (index, part) in text.split('/').enumerate() {
                if index != 0 {
                    joined.push('/');
                }
                joined.push_str(&encode(part));
            }
            if secret {
                marks.push(0..joined.len());
            }
            Ok(PathPart {
                text: joined.into(),
                marks,
            })
        });
        self.path.push(rendered);
    }

    /// Keep the final literal separator after a splat, without adding a segment.
    pub fn trailing_separator(&mut self) {
        self.trailing = true;
    }
    /// An entry of `query { }`. Replacing a key keeps its first position.
    pub fn query(&mut self, key: &'static str, pieces: &[Piece<'_>]) {
        put(
            &mut self.query,
            key,
            entry(pieces, ValuePlace::Query),
            false,
        );
    }
    /// An entry of `headers { }`. Final header values are checked after overrides.
    pub fn header(&mut self, name: &'static str, pieces: &[Piece<'_>]) {
        put(
            &mut self.headers,
            name,
            entry(pieces, ValuePlace::Header),
            true,
        );
    }
    /// `body <expression>;`
    pub fn body(&mut self, body: impl IntoBody) {
        self.body = Some(body.into_body());
    }
    /// Inspect the finished request, refusing a one-shot body without touching it.
    pub fn finish<P>(
        self,
        core: &Core<P>,
        options: &Options<P>,
        endpoint: &'static str,
    ) -> Result<reqwest::Request, Error<P>> {
        self.finish_marked(core, options, endpoint, false)
            .map(|(request, _)| request)
    }
    /// Finish for sending, claiming all composed streams before any byte is polled.
    /// Relative `at_url` resolution and origin checks precede all caller edits.
    pub fn finish_for_send<P>(
        self,
        core: &Core<P>,
        options: &Options<P>,
        endpoint: &'static str,
    ) -> Result<reqwest::Request, Error<P>> {
        self.finish_marked(core, options, endpoint, true)
            .map(|(request, _)| request)
    }
    /// Finish once, retaining secret annotations for preparation and page comparison.
    pub(super) fn finish_marked<P>(
        self,
        core: &Core<P>,
        options: &Options<P>,
        endpoint: &'static str,
        take_body: bool,
    ) -> Result<(reqwest::Request, RequestSecrets), Error<P>> {
        let fail = |failure| Error::build(endpoint, failure);
        let BuiltBase {
            mut url,
            secrets: mut url_marks,
        } = match core.base() {
            Some(Ok(base)) => parsed_base(base).map_err(|_| BuildFailure::ReplacementBase),
            Some(Err(_)) => Err(BuildFailure::ReplacementBase),
            None => self.base.unwrap_or(Err(BuildFailure::Base)),
        }
        .map_err(fail)?;
        let has_path = !self.path.is_empty();
        let prefix = url.path().strip_suffix('/').unwrap_or(url.path());
        let mut path = if has_path || self.trailing {
            let capacity = self
                .path
                .iter()
                .filter_map(|part| part.as_ref().ok())
                .fold(prefix.len(), |size, part| {
                    size.saturating_add(part.text.len()).saturating_add(1)
                });
            let mut joined = String::with_capacity(capacity);
            joined.push_str(prefix);
            joined.push('/');
            joined
        } else {
            String::new()
        };
        let mut marks = Vec::new();
        for (index, part) in self.path.into_iter().enumerate() {
            let part = part.map_err(fail)?;
            if index != 0 {
                path.push('/');
            }
            let offset = path.len();
            marks.extend(
                part.marks
                    .into_iter()
                    // PANIC-SAFETY: Both offsets are lengths of allocated strings, each at most isize::MAX; their sum fits usize.
                    .map(|r| r.start + offset..r.end + offset),
            );
            path.push_str(&part.text);
        }
        if has_path || self.trailing {
            if self.trailing {
                path.push('/');
            }
            url.set_path(&path);
            if url.path().split('/').count() != path.split('/').count() {
                return Err(fail(BuildFailure::Base));
            }
            url_marks.path.extend(marks);
        }
        let query_capacity = self.query.iter().try_fold(0usize, |size, entry| {
            let value = entry.value.as_ref().map_err(|error| fail(error.clone()))?;
            Ok::<usize, Error<P>>(values_encoded_len(entry.key, &value.values).saturating_add(size))
        })?;
        let mut encoded_query = String::with_capacity(query_capacity);
        let mut secret_query = Vec::new();
        // Retain the original entries for at_url's secret reattachment; encode
        // borrowed scalar values directly, without a second collection of pairs.
        for NamedEntry { key, value } in &self.query {
            let Entry { values, secret } = value.as_ref().map_err(|error| fail(error.clone()))?;
            if *secret && !values.is_empty() {
                secret_query.push(*key);
            }
            for value in values {
                query_pair(&mut encoded_query, key, value);
            }
        }
        url.set_query((!encoded_query.is_empty()).then_some(encoded_query.as_str()));
        if url.as_str().len() > MAX_URL_LENGTH {
            return Err(fail(BuildFailure::UrlLength));
        }
        let mut request = reqwest::Request::new(self.method, url);
        let mut secrets = RequestSecrets {
            query: secret_query,
            headers: Vec::new(),
            url: Some(url_marks),
        };
        // Borrow the label without surfacing a body-expression failure before header
        // validation. A removed Content-Type still owns its first key position.
        let body = self.body.transpose();
        let mut body_label = body
            .as_ref()
            .ok()
            .and_then(Option::as_ref)
            .and_then(Body::__content_type);
        for NamedEntry { key, value } in self.headers {
            let Entry { values, secret } = value.map_err(fail)?;
            let name = http::header::HeaderName::from_bytes(key.as_bytes())
                .map_err(|_| fail(BuildFailure::Description("a header name")))?;
            if name == http::header::HOST {
                return Err(fail(BuildFailure::Description("a header name")));
            }
            if name == http::header::CONTENT_TYPE {
                if values.is_empty() {
                    if let Some(label) = body_label.take() {
                        request
                            .headers_mut()
                            .try_insert(
                                name,
                                header_value(label)
                                    .map_err(|_| fail(BuildFailure::Header("content-type")))?,
                            )
                            .map_err(|_| fail(BuildFailure::TooManyHeaders))?;
                    }
                    continue;
                }
                body_label = None;
            }
            for value in values {
                let mut value =
                    header_value(&value).map_err(|_| fail(BuildFailure::Header(key)))?;
                value.set_sensitive(secret);
                request
                    .headers_mut()
                    .try_append(name.clone(), value)
                    .map_err(|_| fail(BuildFailure::TooManyHeaders))?;
            }
        }
        if let Some(label) = body_label {
            request
                .headers_mut()
                .try_insert(
                    http::header::CONTENT_TYPE,
                    header_value(label).map_err(|_| fail(BuildFailure::Header("content-type")))?,
                )
                .map_err(|_| fail(BuildFailure::TooManyHeaders))?;
        }
        let body = body.map_err(|source| Error::build_source(endpoint, source))?;
        let bytes = body.as_ref().and_then(Body::as_bytes);
        if let Some(length) = bytes
            .map(<[u8]>::len)
            .or_else(|| body.is_none().then_some(0))
        {
            for value in request.headers().get_all(http::header::CONTENT_LENGTH) {
                if value
                    .to_str()
                    .ok()
                    .filter(|text| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
                    .and_then(|text| text.parse::<u64>().ok())
                    // PANIC-SAFETY: A body length is usize, at most 64 bits on reqwest-supported targets.
                    != Some(length as u64)
                {
                    return Err(fail(BuildFailure::ContentLength));
                }
            }
        }
        if !take_body && body.as_ref().is_some_and(|body| body.as_bytes().is_none()) {
            return Err(fail(BuildFailure::InspectStream));
        }
        if let Some(mut replacement) = options
            .replacement_url(request.url())
            .map_err(|_| fail(BuildFailure::NextUrl))?
        {
            if replacement.origin() != request.url().origin() {
                return Err(fail(BuildFailure::Origin));
            }
            let mut one_pair = replacement.clone();
            let mut kept = Vec::new();
            if let Some(raw) = replacement.query() {
                for part in raw.split('&') {
                    one_pair.set_query(Some(part));
                    let secret = one_pair
                        .query_pairs()
                        .next()
                        .is_some_and(|(key, _)| secrets.query.contains(&key.as_ref()));
                    if !secret {
                        kept.push(part.to_owned());
                    }
                }
            }
            for NamedEntry { key, value } in &self.query {
                let Entry { values, secret } =
                    value.as_ref().map_err(|error| fail(error.clone()))?;
                if *secret {
                    for value in values {
                        kept.push(format!("{}={}", encode(key), encode(value)));
                    }
                }
            }
            replacement.set_query(
                kept.iter()
                    .any(|pair| !pair.is_empty())
                    .then(|| kept.join("&"))
                    .as_deref(),
            );
            replacement.set_fragment(None);
            if replacement.path() != request.url().path() {
                if let Some(url_marks) = &mut secrets.url {
                    url_marks.path.clear();
                }
            }
            // PANIC-SAFETY: This is a dereference assignment, not integer multiplication.
            *request.url_mut() = replacement;
        }
        if let Some(body) = body {
            let body = match body.as_bytes() {
                Some(bytes) => reqwest::Body::from(bytes.to_vec()),
                None => reqwest::Body::wrap_stream(
                    body.into_stream()
                        .map_err(|_| fail(BuildFailure::SpentStream))?,
                ),
            };
            *request.body_mut() = Some(body);
        }
        if let Some(etag) = options.conditional_etag() {
            request
                .headers_mut()
                .try_insert(
                    http::header::IF_NONE_MATCH,
                    header_value(etag).map_err(|_| fail(BuildFailure::Header("if-none-match")))?,
                )
                .map_err(|_| fail(BuildFailure::TooManyHeaders))?;
        }
        if options.edit(&mut request) {
            secrets.url = None;
        }
        if request.url().as_str().len() > MAX_URL_LENGTH {
            return Err(fail(BuildFailure::UrlLength));
        }
        Ok((request, secrets))
    }
    /// Finish this draft for a send, retaining its secret marks, Run, mark and reader.
    /// Claims one-shot bodies and runs edits once, as [`Self::finish_for_send`] does.
    /// If an edit changes the URL, its secret marks become unknown. The reader owns
    /// the call's inputs; pages can inspect the finished request before [`Prepared::send`].
    /// Uses the same `at_url` resolution and origin check as [`Self::finish_for_send`].
    pub fn prepare<T, P, F, Fut>(
        self,
        run: Run,
        core: Core<P>,
        options: Options<P>,
        idempotent: Idempotent,
        one_try: F,
    ) -> Result<Prepared<T, P>, Error<P>>
    where
        T: Send + 'static,
        P: Send + Sync + 'static,
        F: Fn(reqwest::Response, TryInfo) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Answer<T>, Error<P>>> + Send + 'static,
    {
        let mut run = run;
        run.finish_options(&core, &options);
        let (request, secrets) = self.finish_marked(&core, &options, run.endpoint(), true)?;
        Ok(Prepared {
            run,
            request,
            secrets,
            idempotent,
            core,
            options,
            one_try: Box::new(move |response, info| crate::Sending::new(one_try(response, info))),
        })
    }
}

/// A failed body expression: Build, with the boxed failure as `source()`.
/// Entry expressions use [`Draft::entry_error`] to retain their program name.
pub fn build_error<P>(
    endpoint: &'static str,
    source: Box<dyn std::error::Error + Send + Sync>,
) -> Error<P> {
    Error::build_source(endpoint, source)
}
fn parsed_base(base: &Base) -> Result<BuiltBase, BuildFailure> {
    reqwest::Url::parse(&base.to_string())
        .map(|url| BuiltBase {
            url,
            secrets: SecretUrl::default(),
        })
        .map_err(|_| BuildFailure::Base)
}
fn write(
    name: &'static str,
    value: &dyn Value,
    slot: ValuePlace,
) -> Result<(Shape, bool), BuildFailure> {
    let mut written = Written::default();
    value.write(&mut written);
    let (shape, secret) = written.into_parts();
    shape
        .map(|shape| (shape, secret))
        .map_err(|failure| match failure {
            WriteFailure::Shape => BuildFailure::Shape { input: name, slot },
            WriteFailure::Depth => match slot {
                ValuePlace::Body => BuildFailure::BodyDepth(name.to_owned()),
                _ => BuildFailure::Shape { input: name, slot },
            },
            WriteFailure::Nonfinite => BuildFailure::Nonfinite(name),
        })
}
fn single(
    name: &'static str,
    value: &dyn Value,
    slot: ValuePlace,
) -> Result<(String, bool), BuildFailure> {
    match write(name, value, slot)? {
        (Shape::One(text), secret) => Ok((text, secret)),
        _ => Err(BuildFailure::Shape { input: name, slot }),
    }
}
fn entry(pieces: &[Piece<'_>], slot: ValuePlace) -> Result<Entry, BuildFailure> {
    if let [Piece::Val(name, value)] = pieces {
        let (shape, secret) = write(name, *value, slot)?;
        return Ok(Entry {
            values: shape,
            secret,
        });
    }
    let mut text = String::new();
    let mut absent = false;
    let mut secret = false;
    let mut failure = None;
    for piece in pieces {
        match piece {
            Piece::Lit(literal) => text.push_str(literal),
            Piece::Val(name, value) => match write(name, *value, slot) {
                Ok((shape, mark)) => {
                    secret |= mark;
                    match shape {
                        Shape::Nothing => absent = true,
                        Shape::List(list) if list.is_empty() => absent = true,
                        Shape::One(value) => text.push_str(&value),
                        _ => {
                            failure.get_or_insert(BuildFailure::Shape { input: name, slot });
                        }
                    }
                }
                Err(error) => {
                    failure.get_or_insert(error);
                }
            },
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(Entry {
        values: if absent {
            Shape::Nothing
        } else {
            Shape::One(text)
        },
        secret,
    })
}
fn put(
    entries: &mut Vec<NamedEntry>,
    key: &'static str,
    value: Result<Entry, BuildFailure>,
    header: bool,
) {
    if let Some(entry) = entries.iter_mut().find(|entry| {
        if header {
            entry.key.eq_ignore_ascii_case(key)
        } else {
            entry.key == key
        }
    }) {
        entry.value = value;
        entry.key = key;
    } else {
        entries.push(NamedEntry { key, value });
    }
}
fn encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    encode_into(text, &mut encoded);
    encoded
}
fn unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}
fn encode_into(text: &str, encoded: &mut String) {
    encoded.reserve(text.len());
    for byte in text.bytes() {
        if unreserved(byte) {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
}
fn literal_path(text: &str) -> Result<String, BuildFailure> {
    let mut out = String::with_capacity(text.len());
    literal_path_into(text, &mut out)?;
    Ok(out)
}
fn literal_path_into(text: &str, out: &mut String) -> Result<(), BuildFailure> {
    if text
        .bytes()
        .any(|byte| byte.is_ascii_control() || matches!(byte, b'\\' | b' ' | b'?' | b'#' | b'%'))
    {
        return Err(BuildFailure::Description("fixed path text"));
    }
    out.reserve(text.len());
    for byte in text.bytes() {
        if !byte.is_ascii() || matches!(byte, b'"' | b'<' | b'>' | b'`' | b'{' | b'}') {
            let _ = write!(out, "%{byte:02X}");
        } else {
            out.push(char::from(byte));
        }
    }
    Ok(())
}
fn path_piece(pieces: &[Piece<'_>], trailing: bool) -> Result<PathPart, BuildFailure> {
    // Generated literal segments already have a static lifetime. Keep them
    // borrowed when their wire spelling is unchanged, instead of allocating a
    // temporary String that finish_marked immediately copies into the URL.
    if let [Piece::Lit(literal)] = pieces {
        let literal = if trailing {
            literal.strip_suffix('/').unwrap_or(literal)
        } else {
            literal
        };
        if literal.is_empty() || matches!(literal, "." | "..") || literal.contains('/') {
            return Err(BuildFailure::Description("fixed path text"));
        }
        if literal.bytes().all(|byte| {
            byte.is_ascii()
                && !byte.is_ascii_control()
                && !matches!(
                    byte,
                    b'\\' | b' ' | b'?' | b'#' | b'%' | b'"' | b'<' | b'>' | b'`' | b'{' | b'}'
                )
        }) {
            return Ok(PathPart {
                text: Cow::Borrowed(literal),
                marks: Vec::new(),
            });
        }
    }
    let mut text = String::new();
    let mut marks = Vec::new();
    let mut name = None;
    for (index, piece) in pieces.iter().enumerate() {
        match piece {
            Piece::Lit(literal) => {
                // PANIC-SAFETY: enumerate yields an index strictly below the allocated pieces slice length.
                let literal = if trailing && index + 1 == pieces.len() {
                    literal.strip_suffix('/').unwrap_or(literal)
                } else {
                    literal
                };
                if literal.contains('/') {
                    return Err(BuildFailure::Description("fixed path text"));
                }
                literal_path_into(literal, &mut text)?;
            }
            Piece::Val(input, value) => {
                name.get_or_insert(*input);
                let (value, secret) = single(input, *value, ValuePlace::Path)?;
                if value.is_empty() {
                    return Err(BuildFailure::Path(input));
                }
                let start = text.len();
                encode_into(&value, &mut text);
                if secret {
                    marks.push(start..text.len());
                }
            }
        }
    }
    if text.is_empty() || text == "." || text == ".." {
        return Err(name.map_or(
            BuildFailure::Description("fixed path text"),
            BuildFailure::Path,
        ));
    }
    Ok(PathPart {
        text: text.into(),
        marks,
    })
}
fn header_value(text: &str) -> Result<http::HeaderValue, ()> {
    if !text
        .bytes()
        .all(|byte| byte == b'\t' || (32..=126).contains(&byte))
    {
        return Err(());
    }
    http::HeaderValue::from_str(text).map_err(|_| ())
}
fn values_encoded_len(key: &str, values: &Shape) -> usize {
    fn encoded_len(text: &str) -> usize {
        text.bytes().fold(0usize, |size, byte| {
            size.saturating_add(if unreserved(byte) { 1 } else { 3 })
        })
    }
    let key = encoded_len(key);
    values.into_iter().fold(0usize, |size, value| {
        size.saturating_add(key)
            .saturating_add(encoded_len(value))
            .saturating_add(2)
    })
}
fn query_pair(encoded: &mut String, key: &str, value: &str) {
    if !encoded.is_empty() {
        encoded.push('&');
    }
    encode_into(key, encoded);
    encoded.push('=');
    encode_into(value, encoded);
}

fn template_base(pieces: &[Piece<'_>]) -> Result<BuiltBase, BuildFailure> {
    let mut text = String::new();
    let mut holes = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Lit(literal) => {
                if literal
                    .bytes()
                    .any(|b| b.is_ascii_control() || matches!(b, b'\\' | b' ' | b'?' | b'#' | b'%'))
                {
                    return Err(BuildFailure::Base);
                }
                let previous_len = text.len();
                text.push_str(literal);
                if let Some((scheme, rest)) = text.split_once("://") {
                    if let Some(slash) = rest.find('/') {
                        // PANIC-SAFETY: These offsets stay within the allocated URL text (at most isize::MAX), plus at most one separator.
                        let start = previous_len.max(scheme.len() + 3 + slash);
                        let encoded = literal_path(text.get(start..).ok_or(BuildFailure::Base)?)?;
                        text.truncate(start);
                        text.push_str(&encoded);
                    }
                }
            }
            Piece::Val(name, value) => {
                let (_, rest) = text.split_once("://").ok_or(BuildFailure::Base)?;
                let in_path = rest.contains('/');
                let port = !in_path
                    && if rest.starts_with('[') {
                        rest.contains("]:")
                    } else {
                        rest.contains(':')
                    };
                let (value, secret) = single(
                    name,
                    *value,
                    if in_path {
                        ValuePlace::Path
                    } else {
                        ValuePlace::Host
                    },
                )?;
                if in_path && value.is_empty() {
                    return Err(BuildFailure::Path(name));
                }
                if !in_path && value.is_empty() {
                    return Err(if port {
                        BuildFailure::Base
                    } else {
                        BuildFailure::Host(name)
                    });
                }
                if !in_path
                    && (!value.bytes().all(|b| {
                        if port {
                            b.is_ascii_digit()
                        } else {
                            b.is_ascii_alphanumeric() || b == b'-'
                        }
                    }))
                {
                    return Err(if port {
                        BuildFailure::Base
                    } else {
                        BuildFailure::Host(name)
                    });
                }
                let start = text.len();
                text.push_str(&if in_path { encode(&value) } else { value });
                holes.push((start..text.len(), *name, secret, in_path, port));
            }
        }
    }
    let (scheme, rest) = text.split_once("://").ok_or(BuildFailure::Base)?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return Err(BuildFailure::Base);
    }
    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return Err(BuildFailure::Base);
    }
    let host = if authority.starts_with('[') {
        authority
            .find(']')
            .and_then(|end| authority.get(..=end))
            .ok_or(BuildFailure::Base)?
    } else {
        authority.split(':').next().unwrap_or("")
    };
    // PANIC-SAFETY: These offsets stay within the allocated URL text (at most isize::MAX), plus at most one separator.
    let host_offset = scheme.len() + 3;
    if !host.starts_with('[') {
        let mut offset = host_offset;
        for label in host.split('.') {
            if label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                return Err(holes
                    .iter()
                    .find(|(range, _, _, path, port)| {
                        // PANIC-SAFETY: These offsets stay within the allocated URL text (at most isize::MAX), plus at most one separator.
                        !path && !port && range.start < offset + label.len() && range.end > offset
                    })
                    .map_or(BuildFailure::Base, |(_, name, _, _, _)| {
                        BuildFailure::Host(name)
                    }));
            }
            // PANIC-SAFETY: These offsets stay within the allocated URL text (at most isize::MAX), plus at most one separator.
            offset += label.len() + 1;
        }
    }
    // PANIC-SAFETY: These offsets stay within the allocated URL text (at most isize::MAX), plus at most one separator.
    let path_offset = host_offset + authority.len();
    let raw_path = text.get(path_offset..).ok_or(BuildFailure::Base)?;
    let prefix = raw_path.strip_prefix('/').unwrap_or(raw_path);
    let prefix = prefix.strip_suffix('/').unwrap_or(prefix);
    if !prefix.is_empty() && prefix.split('/').any(str::is_empty) || raw_path == "//" {
        return Err(BuildFailure::Base);
    }
    // PANIC-SAFETY: These offsets stay within the allocated URL text (at most isize::MAX), plus at most one separator.
    let mut segment_offset = path_offset + 1;
    for segment in raw_path.split('/').skip(1) {
        if segment == "." || segment == ".." {
            return Err(holes
                .iter()
                .find(|(range, _, _, path, _)| {
                    *path
                        // PANIC-SAFETY: These offsets stay within the allocated URL text (at most isize::MAX), plus at most one separator.
                        && range.start < segment_offset + segment.len()
                        && range.end > segment_offset
                })
                .map_or(BuildFailure::Base, |(_, name, _, _, _)| {
                    BuildFailure::Path(name)
                }));
        }
        // PANIC-SAFETY: These offsets stay within the allocated URL text (at most isize::MAX), plus at most one separator.
        segment_offset += segment.len() + 1;
    }
    let mut url = reqwest::Url::parse(&text).map_err(|_| BuildFailure::Base)?;
    if !url
        .host_str()
        .is_some_and(|parsed| parsed.eq_ignore_ascii_case(host))
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(BuildFailure::Base);
    }
    if !raw_path.is_empty() && url.path() != raw_path {
        return Err(BuildFailure::Base);
    }
    let mut secrets = SecretUrl::default();
    for (range, _, secret, path, port) in holes {
        if !secret {
            continue;
        }
        if path {
            secrets
                .path
                // PANIC-SAFETY: Only holes recorded after the path separator are classified as path holes.
                .push(range.start - path_offset..range.end - path_offset);
        } else if !port {
            secrets
                .host
                // PANIC-SAFETY: Host holes are recorded after the scheme separator and before the path.
                .push(range.start - host_offset..range.end - host_offset);
        }
    }
    url.set_fragment(None);
    Ok(BuiltBase { url, secrets })
}

#[cfg(test)]
mod tests {
    use super::super::{AdaptValue, ValueRef};
    use super::*;
    use crate::Secret;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    fn draft() -> Draft {
        let mut draft = Draft::new(http::Method::GET);
        draft.base(&[Piece::Lit("https://example.test/prefix/")]);
        draft
    }
    fn finish(draft: Draft) -> Result<reqwest::Request, Error> {
        draft.finish(&Core::default(), &Options::default(), "tasks.get")
    }
    fn error(draft: Draft) -> String {
        finish(draft).unwrap_err().to_string()
    }
    fn named(message: &str) -> String {
        format!("build error: {message}, in tasks.get")
    }

    #[tokio::test(start_paused = true)]
    async fn c2_3_c2_6_the_http_uri_byte_edge_is_build_before_middleware() {
        // The kit uses the normal library, not this unit-test compilation of it.
        use concord_core::__private::{Core, Draft, Idempotent, Options, Piece, Run};
        use concord_core::transport::{TransportError, TryInfo};
        use concord_core::{Answer, Middleware, Next};
        struct Observe(Arc<AtomicUsize>);
        impl Middleware for Observe {
            async fn handle(
                &self,
                request: reqwest::Request,
                info: TryInfo,
                next: Next<'_>,
            ) -> Result<reqwest::Response, TransportError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                next.send(request, info).await
            }
        }
        const BASE: &str = "https://example.test/";
        for query in [false, true] {
            let overhead = BASE.len() + if query { "?q=".len() } else { 0 };
            for input_length in [65_534 - overhead, 65_535 - overhead, 65_534, 65_535] {
                let value = format!(
                    "PRIVATE_URL{}",
                    "a".repeat(input_length - "PRIVATE_URL".len())
                );
                let url = format!("{BASE}{}{value}", if query { "?q=" } else { "" });
                let fits = http::Uri::try_from(url.as_str()).is_ok();
                assert_eq!(fits, url.len() <= 65_534);
                let script = concord_testkit::Script::new();
                script.answer(200);
                let seen = Arc::new(AtomicUsize::new(0));
                let core = Core::<()>::default()
                    .with_transport(script.clone())
                    .with_middleware(Observe(seen.clone()));
                let options = Options::default();
                let run = Run::start(&core, &options, "tasks.get");
                let mut d = Draft::new(http::Method::GET);
                d.base(&[Piece::Lit(BASE)]);
                if query {
                    d.query("q", &[Piece::Val("value", &value)]);
                } else {
                    d.segment(&[Piece::Val("value", &value)]);
                }
                let result =
                    match d.prepare(
                        run,
                        core,
                        options,
                        Idempotent::ByMethod,
                        |r, _| async move {
                            Ok(Answer::new(r.status(), r.headers().clone().into(), ()))
                        },
                    ) {
                        Ok(prepared) => prepared.send().await,
                        Err(error) => Err(error),
                    };
                if fits {
                    assert!(result.is_ok());
                    assert_eq!(seen.load(Ordering::SeqCst), 1);
                    assert_eq!(script.requests().len(), 1);
                } else {
                    let error = result.unwrap_err();
                    assert_eq!(error.kind(), concord_core::error::Kind::Build);
                    assert_eq!(error.tries(), 0);
                    assert_eq!(error.to_string(), named("the URL is too long to be sent"));
                    assert!(!format!("{error} {error:?}").contains("PRIVATE_URL"));
                    assert_eq!(seen.load(Ordering::SeqCst), 0);
                    assert!(script.requests().is_empty());
                }
            }
        }
    }

    #[test]
    fn c2_2_replacement_and_edit_urls_obey_the_same_byte_limit() {
        let url = format!("https://example.test/{}", "a".repeat(65_535));
        for edit in [false, true] {
            let mut options = Options::<()>::default();
            if edit {
                let url = url.parse::<reqwest::Url>().unwrap();
                options.edit_request(move |request| *request.url_mut() = url.clone());
            } else {
                options.at_url(&url);
            }
            let error = draft()
                .finish(&Core::default(), &options, "tasks.get")
                .unwrap_err();
            assert_eq!(error.to_string(), named("the URL is too long to be sent"));
            assert_eq!(error.tries(), 0);
        }
    }

    #[test]
    fn c2_3_literal_segments_borrow_their_unchanged_wire_spelling() {
        let literal = path_piece(&[Piece::Lit("accounts-v1")], false)
            .unwrap_or_else(|_| panic!("valid literal"));
        assert!(matches!(literal.text, Cow::Borrowed("accounts-v1")));
        let trailing = path_piece(&[Piece::Lit("accounts/")], true)
            .unwrap_or_else(|_| panic!("valid trailing literal"));
        assert!(matches!(trailing.text, Cow::Borrowed("accounts")));
        let encoded = path_piece(&[Piece::Lit("café")], false)
            .unwrap_or_else(|_| panic!("valid unicode literal"));
        assert_eq!(encoded.text, "caf%C3%A9");
        for invalid in [".", "..", "bad/path", "bad%path", "bad path", "bad\npath"] {
            assert!(path_piece(&[Piece::Lit(invalid)], false).is_err());
        }
    }

    #[test]
    fn literal_piece_debug_redacts_text() {
        assert_eq!(format!("{:?}", Piece::Lit("private-key")), "Lit(..)");
        assert_eq!(format!("{:#?}", Piece::Lit("private-key")), "Lit(..)");
    }
    #[test]
    fn c2_1_host_label_pieces_case_ports_and_parsed_host_are_checked() {
        let mut d = draft();
        d.base(&[
            Piece::Lit("hTTps://api-"),
            Piece::Val("region", &"Eu"),
            Piece::Lit(".example.test:"),
            Piece::Val("port", &8443),
            Piece::Lit("/v1/"),
        ]);
        assert_eq!(
            finish(d).unwrap().url().as_str(),
            "https://api-eu.example.test:8443/v1/"
        );
        for value in ["bad.name", "bad/name", "bad@name", "é", "a_b"] {
            let mut d = draft();
            d.base(&[
                Piece::Lit("https://api-"),
                Piece::Val("region", &value),
                Piece::Lit(".test/"),
            ]);
            assert_eq!(error(d), named("input region is not one label of a host"));
        }
        for value in ["-bad".to_owned(), "bad-".to_owned(), "a".repeat(64)] {
            let mut d = draft();
            d.base(&[
                Piece::Lit("https://"),
                Piece::Val("region", &value),
                Piece::Lit(".test/"),
            ]);
            assert_eq!(error(d), named("input region is not one label of a host"));
        }
        for base in [
            "https://127.1/",
            "https://2130706433/",
            "https://0x7f000001/",
            "https://user@example.test/",
            "https://example.test/a//b",
            "https://example.test//",
            "https://example.test/a/../",
            "https://example.test/a\\b",
            "https://example.test/a\tb",
            "https://example.test/?q",
            "https://example.test/#f",
            "ftp://example.test/",
        ] {
            let mut d = draft();
            d.base(&[Piece::Lit(base)]);
            assert_eq!(
                error(d),
                named("the base is not a URL that can be used"),
                "{base:?}"
            );
        }
        for port in ["", "-1", "65536", "1.5", "4/evil"] {
            let mut d = draft();
            d.base(&[
                Piece::Lit("https://example.test:"),
                Piece::Val("port", &port),
            ]);
            assert_eq!(error(d), named("the base is not a URL that can be used"));
        }
    }
    #[test]
    /// Also proves C2.10.
    fn c2_2_base_prefix_and_with_base_replace_only_the_base() {
        let mut d = draft();
        d.base_from(&"https://EXAMPLE.test:443/caf%C3%A9/".parse().unwrap());
        d.segment(&[Piece::Lit("items")]);
        assert_eq!(
            finish(d).unwrap().url().as_str(),
            "https://example.test/caf%C3%A9/items"
        );
        let core = Core::<()>::default().with_base("http://other.test:8080/root/");
        let mut d = draft();
        d.segment(&[Piece::Lit("items")]);
        assert_eq!(
            d.finish(&core, &Options::default(), "tasks.get")
                .unwrap()
                .url()
                .as_str(),
            "http://other.test:8080/root/items"
        );
        for _ in 0..2 {
            let core = Core::<()>::default().with_base("bad base");
            assert_eq!(
                draft()
                    .finish(&core.clone(), &Options::default(), "tasks.get")
                    .unwrap_err()
                    .to_string(),
                named("the base is not a URL that can be used (with_base)")
            );
        }
        // A caller replacement replaces even a deferred error in the description's base.
        let mut d = draft();
        d.base(&[Piece::Lit("bad")]);
        assert!(d.finish(&core, &Options::default(), "tasks.get").is_ok());
    }
    #[test]
    fn c2_3_path_encoding_dots_and_last_separator() {
        let mut d = draft();
        d.segment(&[Piece::Val("id", &"a/b ?#%é~._-")]);
        assert_eq!(
            finish(d).unwrap().url().path(),
            "/prefix/a%2Fb%20%3F%23%25%C3%A9~._-"
        );
        for value in ["", ".", ".."] {
            let mut d = draft();
            d.segment(&[Piece::Val("id", &value)]);
            assert_eq!(
                error(d),
                named("input id gives an empty or dot path segment")
            );
        }
        let mut d = draft();
        d.segment(&[Piece::Lit("v"), Piece::Val("id", &".."), Piece::Lit("/")]);
        assert_eq!(finish(d).unwrap().url().path(), "/prefix/v../");
        let mut d = draft();
        d.segment(&[Piece::Lit("v1/")]);
        d.segment(&[Piece::Lit("items")]);
        assert_eq!(finish(d).unwrap().url().path(), "/prefix/v1/items");
        let mut d = draft();
        d.segment(&[Piece::Lit("items/")]);
        d.segment(&[Piece::Lit("")]);
        assert_eq!(finish(d).unwrap().url().path(), "/prefix/items/");
        let mut d = draft();
        d.segment(&[Piece::Lit("café{}")]);
        assert_eq!(finish(d).unwrap().url().path(), "/prefix/caf%C3%A9%7B%7D");
        let mut d = draft();
        d.base(&[
            Piece::Lit("https://example.test/v"),
            Piece::Val("id", &"a/b ?#%"),
            Piece::Lit("/"),
        ]);
        assert_eq!(finish(d).unwrap().url().path(), "/va%2Fb%20%3F%23%25/");
    }

    #[test]
    fn c2_4_a_whole_hole_template_rejects_a_list() {
        let mut d = draft();
        d.query("q", &[Piece::Lit(""), Piece::Val("ids", &vec!["a", "b"])]);
        assert_eq!(error(d), named("input ids cannot be written in a query"));
    }

    #[test]
    fn c2_3_an_empty_fixed_segment_does_nothing() {
        let mut d = draft();
        d.segment(&[Piece::Lit("")]);
        assert_eq!(finish(d).unwrap().url().path(), "/prefix/");
        let mut d = draft();
        d.segment(&[Piece::Lit("items")]);
        d.segment(&[Piece::Lit("")]);
        assert_eq!(finish(d).unwrap().url().path(), "/prefix/items");
    }

    #[test]
    /// Also proves C2.7.
    fn c2_3_invalid_description_names_are_not_input_failures() {
        for key in ["Host", "bad header", ""] {
            let mut d = draft();
            d.header(key, &[Piece::Lit("value")]);
            assert_eq!(
                error(d),
                named("the description is not valid (a header name)")
            );
        }
        for path in ["bad text", "bad?text", "bad#text", "bad/text", ".", ".."] {
            let mut d = draft();
            d.segment(&[Piece::Lit(path)]);
            assert_eq!(
                error(d),
                named("the description is not valid (fixed path text)")
            );
        }
    }
    #[test]
    fn c2_3_splat_preserves_every_empty_piece_and_rejects_dots() {
        for value in ["a//b/", "/a", "//a/", "/", "///", "a/%/é"] {
            let mut d = draft();
            d.segments("file", &value);
            assert_eq!(
                finish(d).unwrap().url().path(),
                format!(
                    "/prefix/{}",
                    value.split('/').map(encode).collect::<Vec<_>>().join("/")
                )
            );
        }
        for value in ["", "a/./b", "../a", "a/..", "."] {
            let mut d = draft();
            d.segments("file", &value);
            assert_eq!(
                error(d),
                named("input file gives an empty or dot path segment")
            );
        }
    }
    #[test]
    fn c2_3_a_literal_separator_after_a_splat_is_independent_of_its_value() {
        for value in ["a/b", "a/b/"] {
            let mut d = draft();
            d.segments("file", &value);
            d.trailing_separator();
            assert_eq!(finish(d).unwrap().url().path(), format!("/prefix/{value}/"));
        }
    }
    #[test]
    fn c2_5_table_4_three_shapes_in_each_placement() {
        #[derive(serde::Serialize)]
        struct Record {
            field: u8,
        }
        let none = None::<u8>;
        let nested = Some(None::<u8>);
        let list = ["a", "b"];
        let empty: [u8; 0] = [];
        let record = Record { field: 1 };
        let shapes: [(&dyn Value, bool, bool); 9] = [
            (&none, false, true),
            (&(), false, true),
            (&nested, false, true),
            (&"text", true, true),
            (&5, true, true),
            (&true, true, true),
            (&list, false, true),
            (&empty, false, true),
            (&record, false, false),
        ];
        for (value, single_ok, entry_ok) in shapes {
            let mut d = draft();
            d.segment(&[Piece::Val("value", value)]);
            assert_eq!(finish(d).is_ok(), single_ok);
            let mut d = draft();
            d.segments("value", value);
            assert_eq!(finish(d).is_ok(), single_ok);
            let mut d = draft();
            d.base(&[
                Piece::Lit("https://"),
                Piece::Val("value", value),
                Piece::Lit(".test/"),
            ]);
            assert_eq!(finish(d).is_ok(), single_ok);
            for header in [false, true] {
                let mut d = draft();
                if header {
                    d.header("x-value", &[Piece::Val("value", value)]);
                } else {
                    d.query("value", &[Piece::Val("value", value)]);
                }
                assert_eq!(finish(d).is_ok(), entry_ok);
                let mut d = draft();
                let pieces = [Piece::Lit("prefix"), Piece::Val("value", value)];
                if header {
                    d.header("x-value", &pieces);
                } else {
                    d.query("value", &pieces);
                }
                let template_ok = entry_ok
                    && (!matches!(write("value", value, ValuePlace::Query), Ok((Shape::List(ref list), _)) if !list.is_empty()));
                assert_eq!(finish(d).is_ok(), template_ok);
            }
        }
    }
    #[test]
    fn c2_6_query_order_lists_encoding_and_override_none() {
        let mut d = draft();
        d.query("first", &[Piece::Val("first", &vec!["old", "values"])]);
        d.query("a +&é", &[Piece::Val("value", &" +&é")]);
        d.query("last", &[Piece::Lit("z")]);
        d.query("first", &[Piece::Val("first", &vec!["new", "two"])]);
        assert_eq!(
            finish(d).unwrap().url().query(),
            Some("first=new&first=two&a%20%2B%26%C3%A9=%20%2B%26%C3%A9&last=z")
        );
        let mut d = draft();
        d.query("a", &[Piece::Lit("old")]);
        d.query("b", &[Piece::Lit("kept")]);
        d.query("a", &[Piece::Val("a", &None::<u32>)]);
        assert_eq!(finish(d).unwrap().url().query(), Some("b=kept"));
    }
    #[test]
    fn c2_7_header_lists_case_override_visible_ascii_and_sensitivity() {
        let mut d = draft();
        d.header("X-Note", &[Piece::Lit("bad\nvalue")]);
        d.header("x-middle", &[Piece::Lit("middle")]);
        d.header("x-note", &[Piece::Val("note", &vec!["first", "second"])]);
        let request = finish(d).unwrap();
        assert_eq!(
            request
                .headers()
                .keys()
                .map(|key| key.as_str())
                .collect::<Vec<_>>(),
            ["x-note", "x-middle"]
        );
        assert_eq!(
            request
                .headers()
                .get_all("x-note")
                .iter()
                .map(|v| v.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        for value in ["a\nb", "a\rb", "\x7f", "é"] {
            let mut d = draft();
            d.header("x-note", &[Piece::Val("note", &value)]);
            assert_eq!(
                error(d),
                named("the value of header x-note is not a legal header value")
            );
        }
        let secret = Secret::new("token");
        let mut d = draft();
        d.header(
            "authorization",
            &[Piece::Lit("Bearer "), Piece::Val("token", &secret)],
        );
        let request = finish(d).unwrap();
        assert!(request
            .headers()
            .get("authorization")
            .unwrap()
            .is_sensitive());
        assert_eq!(
            request.headers().get("authorization").unwrap(),
            "Bearer token"
        );
        let mut d = draft();
        d.header("x-note", &[Piece::Lit("bad\n")]);
        d.header("X-NOTE", &[Piece::Val("note", &None::<u8>)]);
        assert!(!finish(d).unwrap().headers().contains_key("x-note"));
    }
    #[test]
    fn c2_8_none_checks_all_holes_even_after_failure_and_empty_list_omits() {
        struct Count(Arc<AtomicUsize>);
        impl serde::Serialize for Count {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                self.0.fetch_add(1, Ordering::SeqCst);
                s.serialize_str("checked")
            }
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let mut d = draft();
        d.query(
            "note",
            &[
                Piece::Val("none", &None::<u8>),
                Piece::Val("list", &vec![1]),
                Piece::Val("count", &Count(calls.clone())),
            ],
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(error(d), named("input list cannot be written in a query"));
        let mut d = draft();
        d.query(
            "note",
            &[Piece::Lit("prefix"), Piece::Val("none", &Vec::<u8>::new())],
        );
        assert_eq!(finish(d).unwrap().url().query(), None);
    }
    #[test]
    fn c2_9_body_label_fallback_explicit_header_and_failure_source() {
        for label in [None, Some("custom/type")] {
            let mut d = draft();
            d.body(Body::new("bytes", "body/type"));
            match label {
                Some(label) => d.header("content-type", &[Piece::Lit(label)]),
                None => d.header("content-type", &[Piece::Val("label", &None::<u8>)]),
            }
            let request = finish(d).unwrap();
            assert_eq!(
                request.headers().get("content-type").unwrap(),
                label.unwrap_or("body/type")
            );
            assert_eq!(
                request.body().unwrap().as_bytes(),
                Some(b"bytes".as_slice())
            );
        }
        let mut d = draft();
        d.body(Err::<Body, _>(std::io::Error::other("writer cause")));
        let failure = finish(d).unwrap_err();
        assert_eq!(failure.to_string(), named("the body could not be written"));
        assert_eq!(
            std::error::Error::source(&failure).unwrap().to_string(),
            "writer cause"
        );
        let failure: Error = Draft::entry_error(
            "tasks.get",
            "limit",
            Box::new(std::io::Error::other("entry cause")),
        );
        assert_eq!(
            std::error::Error::source(&failure).unwrap().to_string(),
            "entry cause"
        );
    }
    #[test]
    fn c2_9_invalid_body_labels_name_the_content_type_header() {
        for stream in [false, true] {
            for empty_header in [false, true] {
                let body = if stream {
                    Body::stream(futures_util::stream::iter([Ok::<_, std::io::Error>(
                        bytes::Bytes::from_static(b"body"),
                    )]))
                    .content_type("text/plain\r\nPRIVATE_LABEL")
                } else {
                    Body::new("body", "text/plain\r\nPRIVATE_LABEL")
                };
                let mut d = draft();
                d.body(body.clone());
                if empty_header {
                    d.header("content-type", &[Piece::Val("label", &None::<String>)]);
                }
                let error: Error = d
                    .finish_for_send(&Core::default(), &Options::default(), "tasks.get")
                    .unwrap_err();
                assert_eq!(
                    error.to_string(),
                    named("the value of header content-type is not a legal header value")
                );
                assert_eq!(error.tries(), 0);
                assert!(std::error::Error::source(&error).is_none());
                assert!(!format!("{error} {error:?}").contains("PRIVATE_LABEL"));
                if stream {
                    assert!(
                        body.into_stream().is_ok(),
                        "failed validation must not claim the body"
                    );
                }
            }
        }
        let mut d = draft();
        d.body(Body::new("body", "bad\r\nlabel"));
        d.header("content-type", &[Piece::Lit("text/plain")]);
        assert_eq!(finish(d).unwrap().headers()["content-type"], "text/plain");
    }
    #[test]
    fn c2_12_buffered_content_length_checked_stream_length_left_alone() {
        for length in ["4", "6", "-5", "+5", "5.0", "", "18446744073709551616"] {
            let mut d = draft();
            d.body(Body::from("bytes"));
            d.header("content-length", &[Piece::Val("size", &length)]);
            assert_eq!(
                error(d),
                named("the content-length is not the length of the body")
            );
        }
        let mut d = draft();
        d.body(Body::from("bytes"));
        d.header("content-length", &[Piece::Val("size", &5)]);
        assert!(finish(d).is_ok());
        let mut d = draft();
        d.body(stream());
        d.header("content-length", &[Piece::Lit("900")]);
        assert_eq!(
            d.finish_for_send(&Core::<()>::default(), &Options::default(), "tasks.get")
                .unwrap()
                .headers()
                .get("content-length")
                .unwrap(),
            "900"
        );
    }
    fn stream() -> Body {
        Body::stream(futures_util::stream::poll_fn(
            |_| -> std::task::Poll<Option<Result<bytes::Bytes, std::io::Error>>> {
                panic!("building must never poll a stream")
            },
        ))
    }
    #[test]
    fn c2_15_inspection_never_claims_composed_streams_send_claims_all_once() {
        let source = stream();
        let body = Body::concat([Body::from("prefix"), source.clone(), Body::from("suffix")]);
        for _ in 0..2 {
            let mut d = draft();
            d.body(&body);
            assert_eq!(
                error(d),
                named("the body is a stream; into_request takes it")
            );
        }
        let mut d = draft();
        d.body(body.clone());
        let request = d
            .finish_for_send(&Core::<()>::default(), &Options::default(), "tasks.get")
            .unwrap();
        assert!(request.body().unwrap().as_bytes().is_none());
        let mut d = draft();
        d.body(body);
        assert_eq!(
            d.finish_for_send(&Core::<()>::default(), &Options::default(), "tasks.get")
                .unwrap_err()
                .to_string(),
            named("the body is a stream that was already sent")
        );
        assert!(source.into_stream().is_err());
    }
    #[test]
    fn c2_13_named_shapes_and_nonfinite_have_exact_safe_errors() {
        let mut d = draft();
        d.query("filter", &[Piece::Val("client.filter", &vec![vec![1]])]);
        assert_eq!(
            error(d),
            named("input client.filter cannot be written in a query")
        );
        let mut d = draft();
        d.header("x-note", &[Piece::Val("note", &f64::NAN)]);
        assert_eq!(error(d), named("input note is not a finite number"));
    }
    #[test]
    /// Also proves C2.7.
    /// Also proves C2.4 and C10.7.
    fn c8_2_secrets_read_once_mark_encoded_ranges_and_redact_debug() {
        let secret = Secret::new("first/key");
        let optional = Some(Some(secret.clone()));
        let adapted = ValueRef(&optional);
        let mut d = draft();
        d.segment(&[
            Piece::Lit("id-"),
            Piece::Val("key", &(&adapted).adapt()),
            Piece::Lit("-tail"),
        ]);
        d.query("key", &[Piece::Val("key", &secret)]);
        d.header("authorization", &[Piece::Val("key", &secret)]);
        assert!(!format!("{d:?}").contains("first"));
        secret.replace("second/key");
        let (request, marks) = d
            .finish_marked(
                &Core::<()>::default(),
                &Options::default(),
                "tasks.get",
                false,
            )
            .unwrap();
        assert_eq!(request.url().path(), "/prefix/id-first%2Fkey-tail");
        assert_eq!(request.url().query(), Some("key=first%2Fkey"));
        let url_marks = marks.url.unwrap();
        assert_eq!(url_marks.path.as_slice(), std::slice::from_ref(&(11..22)));
        assert_eq!(
            request.url().path().get(url_marks.path[0].clone()),
            Some("first%2Fkey")
        );
        assert_eq!(marks.query, ["key"]);
        assert!(request
            .headers()
            .get("authorization")
            .unwrap()
            .is_sensitive());
        let mut written = Written::default();
        secret.write(&mut written);
        assert_eq!(format!("{written:?}"), "Written(..)");
    }
    #[test]
    fn c2_1_secret_base_host_and_prefix_marks_survive_joining() {
        let host = Secret::new("EU");
        let key = Secret::new("key/value");
        let mut d = draft();
        d.base(&[
            Piece::Lit("https://api-"),
            Piece::Val("host", &host),
            Piece::Lit(".test/café/"),
            Piece::Val("key", &key),
            Piece::Lit("/"),
        ]);
        d.segment(&[Piece::Lit("items")]);
        let (request, marks) = d
            .finish_marked(
                &Core::<()>::default(),
                &Options::default(),
                "tasks.get",
                false,
            )
            .unwrap();
        assert_eq!(
            request.url().as_str(),
            "https://api-eu.test/caf%C3%A9/key%2Fvalue/items"
        );
        let marks = marks.url.unwrap();
        assert_eq!(marks.host.as_slice(), std::slice::from_ref(&(4..6)));
        assert_eq!(
            request.url().path().get(marks.path[0].clone()),
            Some("key%2Fvalue")
        );
    }
    #[test]
    fn c8_6_at_url_resolves_against_answer_or_built_url_then_checks_origin_before_edits() {
        for (answer, expected) in [
            (None, "https://example.test/next?q=1"),
            (
                Some("https://example.test/pages/current"),
                "https://example.test/next?q=1",
            ),
        ] {
            let mut options = Options::default();
            if let Some(answer) = answer {
                options.answered_from(answer.parse().unwrap());
            }
            options.at_url("../next?q=1#ignored");
            let calls = Arc::new(AtomicUsize::new(0));
            let observed = calls.clone();
            options.edit_request(move |request| {
                assert_eq!(request.url().as_str(), expected);
                observed.fetch_add(1, Ordering::SeqCst);
                *request.method_mut() = http::Method::POST;
            });
            options.edit_request(|request| {
                assert_eq!(request.method(), http::Method::POST);
                *request.url_mut() = "https://edited.test/".parse().unwrap();
            });
            let request = draft()
                .finish(&Core::<()>::default(), &options.clone(), "tasks.get")
                .unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert_eq!(request.url().as_str(), "https://edited.test/");
        }
        for next in [
            "https://foreign.test/x",
            "http://example.test/x",
            "https://example.test:8443/x",
            "//foreign.test/x",
        ] {
            let mut options = Options::default();
            options.at_url(next);
            options.edit_request(|_| panic!("foreign origins must be refused before any edit"));
            assert_eq!(
                draft()
                    .finish(&Core::<()>::default(), &options, "tasks.get")
                    .unwrap_err()
                    .to_string(),
                named("the URL is at another origin than the call")
            );
        }
    }
    #[test]
    fn c8_6_at_url_uses_the_answer_path_and_caller_base_origin() {
        let mut options = Options::default();
        options.answered_from("https://example.test/answers/current".parse().unwrap());
        options.at_url("next");
        let request = draft()
            .finish(&Core::<()>::default(), &options, "tasks.get")
            .unwrap();
        assert_eq!(request.url().as_str(), "https://example.test/answers/next");
        let mut options = Options::default();
        options.answered_from("https://foreign.test/current".parse().unwrap());
        options.at_url("next");
        assert_eq!(
            draft()
                .finish(&Core::<()>::default(), &options, "tasks.get")
                .unwrap_err()
                .to_string(),
            named("the URL is at another origin than the call")
        );
        options.at_url("https://example.test/okay");
        assert!(draft()
            .finish(&Core::<()>::default(), &options, "tasks.get")
            .is_ok());
    }
    #[test]
    /// Also proves C8.2.
    fn c8_6_secret_query_replaces_all_next_url_collisions_once_and_url_edits_lose_marks() {
        let mut d = draft();
        d.query("token", &[Piece::Val("token", &Secret::new("secret +"))]);
        d.query("ordinary", &[Piece::Lit("old")]);
        let mut options = Options::default();
        options.at_url("/next?token=one&to%6ben=two&public=keep+raw#fragment");
        let (request, marks) = d
            .finish_marked(&Core::<()>::default(), &options, "tasks.get", false)
            .unwrap();
        assert_eq!(
            request.url().as_str(),
            "https://example.test/next?public=keep+raw&token=secret%20%2B"
        );
        assert_eq!(marks.query, ["token"]);
        assert!(marks.url.is_some());
        options.edit_request(|r| r.url_mut().set_path("/changed"));
        let (_, marks) = draft()
            .finish_marked(&Core::<()>::default(), &options, "tasks.get", false)
            .unwrap();
        assert!(marks.url.is_none());
    }
    #[test]
    fn c2_1_base_dot_errors_name_the_hole_in_that_segment_and_marks_keep_dot_prefixes() {
        let mut d = draft();
        d.base(&[
            Piece::Lit("https://example.test/"),
            Piece::Val("first", &"okay"),
            Piece::Lit("/"),
            Piece::Val("id", &".."),
        ]);
        assert_eq!(
            error(d),
            named("input id gives an empty or dot path segment")
        );
        let mut d = draft();
        d.base(&[
            Piece::Lit("https://example.test/."),
            Piece::Val("key", &Secret::new("secret")),
        ]);
        let (request, marks) = d
            .finish_marked(
                &Core::<()>::default(),
                &Options::default(),
                "tasks.get",
                false,
            )
            .unwrap();
        let marks = marks.url.unwrap();
        assert_eq!(
            request.url().path().get(marks.path[0].clone()),
            Some("secret")
        );
        assert_eq!(request.url().path(), "/.secret");
    }
    #[test]
    fn c2_4_secret_splat_separators_are_secret_and_edits_preserve_public_header_values() {
        fn request(key: &str) -> (reqwest::Request, RequestSecrets) {
            let mut d = draft();
            d.segments("key", &Secret::new(key));
            d.header("x-token", &[Piece::Val("key", &Secret::new("key"))]);
            let mut options = Options::default();
            options.edit_request(|r| {
                r.headers_mut()
                    .append("x-token", http::HeaderValue::from_static("public"));
            });
            d.finish_marked(&Core::<()>::default(), &options, "tasks.get", false)
                .unwrap()
        }
        let (a, am) = request("a/b");
        let (b, bm) = request("longer");
        assert!(super::super::same_page_request(
            &a,
            &b,
            &[],
            &[],
            am.url.as_ref(),
            bm.url.as_ref()
        ));
        assert!(a
            .headers()
            .get_all("x-token")
            .iter()
            .any(|v| !v.is_sensitive()));
    }
    #[test]
    fn c2_1_empty_host_is_a_host_error_empty_port_is_a_base_error_and_hosts_are_ascii() {
        for (pieces, expected) in [
            (
                vec![
                    Piece::Lit("https://"),
                    Piece::Val("region", &""),
                    Piece::Lit(".test/"),
                ],
                "input region is not one label of a host",
            ),
            (
                vec![Piece::Lit("https://example.test:"), Piece::Val("port", &"")],
                "the base is not a URL that can be used",
            ),
        ] {
            let mut d = draft();
            d.base(&pieces);
            assert_eq!(error(d), named(expected));
        }
        let mut d = draft();
        d.base(&[
            Piece::Lit("https://"),
            Piece::Val("region", &"é"),
            Piece::Lit(".test/"),
        ]);
        assert_eq!(error(d), named("input region is not one label of a host"));
        let mut d = draft();
        d.base(&[Piece::Lit("https://xn--xample-9ua.test/")]);
        assert!(finish(d).is_ok());
    }
    #[test]
    fn c2_7_tabs_are_legal_and_header_keys_coalesce_without_case() {
        let mut d = draft();
        d.header("X-NOTE", &[Piece::Lit("invalid\n")]);
        d.header("x-note", &[Piece::Lit("Bearer\ttoken")]);
        let request = finish(d).unwrap();
        assert_eq!(request.headers().get_all("X-NoTe").iter().count(), 1);
        assert_eq!(request.headers().get("x-note").unwrap(), "Bearer\ttoken");
        assert_eq!(request.headers().keys().next().unwrap().as_str(), "x-note");
    }
    #[test]
    /// Also proves C2.6.
    fn c2_3_uppercase_escapes_no_empty_query_and_empty_text_pair() {
        let request = finish(draft()).unwrap();
        assert_eq!(request.url().query(), None);
        assert!(!request.url().as_str().ends_with('?'));
        let mut d = draft();
        d.segment(&[Piece::Val("id", &"/")]);
        d.query("empty", &[Piece::Val("empty", &"")]);
        d.query("key/", &[Piece::Val("value", &"/")]);
        let request = finish(d).unwrap();
        assert_eq!(request.url().path(), "/prefix/%2F");
        assert_eq!(request.url().query(), Some("empty=&key%2F=%2F"));
    }
    #[test]
    fn c2_13_public_entry_error_keeps_its_key_and_source() {
        let error: Error = Draft::entry_error(
            "tasks.get",
            "limit",
            Box::new(std::io::Error::other("entry cause")),
        );
        assert_eq!(error.to_string(), named("input limit could not be written"));
        assert_eq!(
            std::error::Error::source(&error).unwrap().to_string(),
            "entry cause"
        );
    }
    #[test]
    fn c2_6_replacement_urls_with_no_query_pair_have_no_question_mark() {
        for next in ["/next?", "/next?&&#fragment"] {
            let mut options = Options::default();
            options.at_url(next);
            let request = draft()
                .finish(&Core::<()>::default(), &options, "tasks.get")
                .unwrap();
            assert_eq!(request.url().as_str(), "https://example.test/next");
            assert_eq!(request.url().query(), None);
        }
    }

    #[test]
    fn c2_7_header_capacity_is_build_for_entries_and_body_label() {
        // Fill the library's actual capacity without depending on a magic threshold.
        let mut map = http::HeaderMap::new();
        let mut names = Vec::new();
        for index in 0..=32_768 {
            let name: &'static str = Box::leak(format!("x-capacity-{index}").into_boxed_str());
            names.push(name);
            if map
                .try_insert(
                    http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                    http::HeaderValue::from_static("one"),
                )
                .is_err()
            {
                break;
            }
        }
        let capacity = map.keys().count();
        assert_eq!(names.len(), capacity + 1);
        for (count, body) in [(capacity, false), (capacity + 1, false), (capacity, true)] {
            let mut draft = draft();
            // Bypass only the unchanged key-merging loop so this capacity test stays linear.
            draft.headers = names
                .iter()
                .take(count)
                .map(|&key| NamedEntry {
                    key,
                    value: Ok(Entry {
                        values: Shape::One("one".to_owned()),
                        secret: false,
                    }),
                })
                .collect();
            if body {
                draft.body(Body::new("body", "text/plain"));
            }
            let result = finish(draft);
            if count == capacity && !body {
                assert_eq!(result.unwrap().headers().keys().count(), capacity);
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.kind(), crate::error::Kind::Build);
                assert_eq!(error.tries(), 0);
                assert_eq!(
                    error.to_string(),
                    "build error: the request has too many headers, in tasks.get"
                );
            }
        }
    }

    #[test]
    fn c2_4_at_url_keeps_secret_marks_when_the_path_is_unchanged() {
        fn request(token: &str, mode: usize) -> (reqwest::Request, RequestSecrets) {
            let mut d = draft();
            d.base(&[
                Piece::Lit("https://example.test/prefix/"),
                Piece::Val("token", &Secret::new(token)),
                Piece::Lit("/tail"),
            ]);
            let mut options = Options::default();
            let absolute = format!("https://example.test/prefix/{}/tail?page=2", encode(token));
            options.at_url(match mode {
                0 => "?page=2",
                1 => "#ignored",
                2 => "",
                _ => &absolute,
            });
            d.finish_marked(&Core::<()>::default(), &options, "tasks.get", false)
                .unwrap()
        }
        for mode in 0..4 {
            let (left, lm) = request("one", mode);
            let (right, rm) = request("two/secret", mode);
            assert_eq!(lm.url.as_ref().unwrap().path.len(), 1);
            assert_eq!(rm.url.as_ref().unwrap().path.len(), 1);
            assert!(super::super::same_page_request(
                &left,
                &right,
                &[],
                &[],
                lm.url.as_ref(),
                rm.url.as_ref()
            ));
        }
        let mut d = draft();
        d.segment(&[Piece::Val("token", &Secret::new("one"))]);
        let mut options = Options::default();
        options.at_url("/different");
        let (_, marks) = d
            .finish_marked(&Core::<()>::default(), &options, "tasks.get", false)
            .unwrap();
        assert!(marks.url.unwrap().path.is_empty());
    }

    #[test]
    fn c2_7_body_content_type_fills_its_first_key_position() {
        for first in [true, false] {
            let mut d = draft();
            if !first {
                d.header("x-before", &[Piece::Lit("before")]);
            }
            d.header("Content-Type", &[Piece::Lit("old/type")]);
            d.header("x-middle", &[Piece::Lit("middle")]);
            d.header("CONTENT-TYPE", &[Piece::Val("label", &None::<u8>)]);
            d.body(Body::new("body", "body/type"));
            let request = finish(d).unwrap();
            let expected = if first {
                vec!["content-type", "x-middle"]
            } else {
                vec!["x-before", "content-type", "x-middle"]
            };
            assert_eq!(
                request
                    .headers()
                    .keys()
                    .map(|name| name.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(request.headers().get("content-type").unwrap(), "body/type");
            assert!(!request
                .headers()
                .get("content-type")
                .unwrap()
                .is_sensitive());
        }
    }
    #[test]
    fn c8_6_an_invalid_next_url_names_the_next_url() {
        let mut options = Options::default();
        options.at_url("http://[bad");
        let error = draft()
            .finish(&Core::<()>::default(), &options, "tasks.get")
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "build error: the next URL is not a URL, in tasks.get"
        );
    }
    #[test]
    fn c2_14_body_template_omits_none_and_accepts_secret_holes() {
        let secret = Secret::from("private");
        let value = ValueRef(&secret);
        let text = Draft::body_template::<()>(
            "body.send",
            &[Piece::Lit("key="), Piece::Val("key", &(&value).adapt())],
        )
        .unwrap();
        assert_eq!(text.as_deref(), Some("key=private"));
        let none: Option<String> = None;
        assert_eq!(
            Draft::body_template::<()>(
                "body.send",
                &[Piece::Lit("prefix"), Piece::Val("none", &none)]
            )
            .unwrap(),
            None
        );
    }
    #[test]
    fn c2_14_body_template_checks_every_hole_after_none_or_failure() {
        struct Noted<'a>(&'a AtomicUsize);
        impl Value for Noted<'_> {
            fn write(&self, to: &mut Written) {
                self.0.fetch_add(1, Ordering::SeqCst);
                7u32.write(to);
            }
        }
        let count = AtomicUsize::new(0);
        let none: Option<String> = None;
        let invalid = vec![1, 2];
        let error = Draft::body_template::<()>(
            "body.send",
            &[
                Piece::Val("none", &none),
                Piece::Val("list", &invalid),
                Piece::Val("last", &Noted(&count)),
            ],
        )
        .unwrap_err();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(
            error.to_string(),
            "build error: the body could not be written, in body.send"
        );
        assert!(!format!("{error:?}").contains("private"));
    }
}

#[cfg(test)]
mod template_depth_test {
    use super::*;
    struct Nested(u16);
    impl serde::Serialize for Nested {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            if self.0 == 0 {
                serializer.serialize_u8(1)
            } else {
                serializer.serialize_some(&Nested(self.0 - 1))
            }
        }
    }
    #[test]
    fn c2_14_a_body_template_depth_failure_names_its_hole() {
        let error =
            Draft::body_template::<()>("template.post", &[Piece::Val("deep", &Nested(129))])
                .unwrap_err();
        assert_eq!(
            error.to_string(),
            "build error: input deep cannot be written in a body, in template.post"
        );
        assert!(
            Draft::body_template::<()>("template.post", &[Piece::Val("deep", &Nested(128))])
                .is_ok()
        );
    }
}
