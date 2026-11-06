//! Consecutive JSON values (a Kubernetes watch stream): an incremental boundary scanner, then
//! serde_json on each complete value.

use std::fmt;
use std::marker::PhantomData;

use concord_core::{CodecError, reqwest};
use serde::de::DeserializeOwned;

use super::policy::{DEFAULT_MAX_RECORD_BYTES, Family, MediaPolicy, check_media};
use crate::stream::{
    ByteSource, Driver, OpenError, Openable, Position, ReadError, ReadErrorKind, SKIP_BUDGET,
    open_input, open_response, yield_now,
};

/// Where the scanner stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Between values (skipping whitespace).
    Between,
    /// Inside a number or literal (`12`, `true`, `null`): it ends at whitespace, at the start of
    /// a self-delimiting value (`{`, `[`, `"`) or at the true end of input — never at a chunk
    /// boundary.
    Primitive,
    /// Inside a top-level string.
    Text { escape: bool },
    /// Inside an object or array.
    Nested {
        depth: u32,
        in_text: bool,
        escape: bool,
    },
}

/// One scanning step.
#[derive(Debug, PartialEq, Eq)]
enum Scanned {
    /// A complete value: `input[start..end]`, or the scanner's buffer when `buffered`.
    Value {
        buffered: bool,
        start: usize,
        end: usize,
    },
    /// The input is used up (inside a value, kept in the buffer, or between values).
    NeedMore,
    /// [`SKIP_BUDGET`] whitespace bytes were skipped: the caller may yield, then call again. (The
    /// reader never passes more than `SKIP_BUDGET` bytes, so it sees `NeedMore` instead.)
    Skipped,
    /// True end of input between values.
    End,
}

struct Fault {
    kind: ReadErrorKind,
    codec: CodecError,
}

/// Finds value boundaries across arbitrary chunk boundaries, retaining only the unfinished
/// value (at most `max` bytes: a longer one is a [`ReadErrorKind::Limit`] fault, detected before
/// the buffer grows past it).
struct Scanner {
    buf: Vec<u8>,
    state: State,
    release: bool,
    /// The most bytes the buffer ever held.
    peak: usize,
    max: usize,
}

const fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

impl Scanner {
    fn new(max: usize) -> Self {
        Self {
            buf: Vec::new(),
            state: State::Between,
            release: false,
            peak: 0,
            max,
        }
    }

    /// Clears a buffered value once its caller is done with it.
    fn release(&mut self) {
        if self.release {
            self.buf.clear();
            self.release = false;
        }
    }

    fn scan(&mut self, input: &[u8], eof: bool) -> Result<(usize, Scanned), Fault> {
        self.release();
        let mut i = 0;
        let start;
        if self.state == State::Between {
            let budget = input.len().min(SKIP_BUDGET);
            while i < budget && is_ws(input[i]) {
                i += 1;
            }
            if i == input.len() {
                return Ok((i, if eof { Scanned::End } else { Scanned::NeedMore }));
            }
            if i == budget {
                return Ok((i, Scanned::Skipped));
            }
            start = i;
            self.state = match input[i] {
                b'{' | b'[' => State::Nested {
                    depth: 0,
                    in_text: false,
                    escape: false,
                },
                b'"' => {
                    i += 1;
                    State::Text { escape: false }
                }
                _ => State::Primitive,
            };
        } else {
            start = 0;
        }
        // The value's bytes may reach `input[..limit]`; one more would exceed `max`.
        let room = self.max.saturating_sub(self.buf.len());
        let limit = input.len().min(start.saturating_add(room));
        let mut end = None;
        while i < limit {
            let b = input[i];
            match &mut self.state {
                State::Between => unreachable!("a value is being scanned"),
                State::Primitive => {
                    if is_ws(b) || matches!(b, b'{' | b'[' | b'"') {
                        end = Some(i);
                        break;
                    }
                }
                State::Text { escape } => {
                    if *escape {
                        *escape = false;
                    } else if b == b'\\' {
                        *escape = true;
                    } else if b == b'"' {
                        end = Some(i + 1);
                        break;
                    }
                }
                State::Nested {
                    depth,
                    in_text,
                    escape,
                } => {
                    if *in_text {
                        if *escape {
                            *escape = false;
                        } else if b == b'\\' {
                            *escape = true;
                        } else if b == b'"' {
                            *in_text = false;
                        }
                    } else {
                        match b {
                            b'{' | b'[' => *depth = depth.saturating_add(1),
                            b'}' | b']' => {
                                *depth = depth.saturating_sub(1);
                                if *depth == 0 {
                                    end = Some(i + 1);
                                    break;
                                }
                            }
                            b'"' => *in_text = true,
                            _ => {}
                        }
                    }
                }
            }
            i += 1;
        }
        // A primitive also ends at the delimiter just past the limit.
        if end.is_none()
            && i == limit
            && limit < input.len()
            && self.state == State::Primitive
            && (is_ws(input[limit]) || matches!(input[limit], b'{' | b'[' | b'"'))
        {
            end = Some(limit);
        }
        match end {
            Some(end) => Ok((end, self.complete(input, start, end))),
            None if limit < input.len() => Err(self.limit()),
            None if eof => match self.state {
                State::Primitive => Ok((input.len(), self.complete(input, start, input.len()))),
                _ => Err(Fault {
                    kind: ReadErrorKind::Format,
                    codec: CodecError::new("the input ends inside a JSON value"),
                }),
            },
            None => {
                self.push(&input[start..]);
                Ok((input.len(), Scanned::NeedMore))
            }
        }
    }

    fn complete(&mut self, input: &[u8], start: usize, end: usize) -> Scanned {
        self.state = State::Between;
        if self.buf.is_empty() {
            Scanned::Value {
                buffered: false,
                start,
                end,
            }
        } else {
            self.push(&input[start..end]);
            self.release = true;
            Scanned::Value {
                buffered: true,
                start: 0,
                end: self.buf.len(),
            }
        }
    }

    /// Appends to the buffer without letting its capacity exceed `max` for a value that
    /// respects it.
    fn push(&mut self, bytes: &[u8]) {
        let needed = self.buf.len() + bytes.len();
        if needed > self.buf.capacity() {
            let target = (self.buf.capacity() * 2)
                .max(needed)
                .min(self.max.max(needed));
            self.buf.reserve_exact(target - self.buf.len());
        }
        self.buf.extend_from_slice(bytes);
        self.peak = self.peak.max(self.buf.len());
    }

    fn limit(&self) -> Fault {
        Fault {
            kind: ReadErrorKind::Limit,
            codec: CodecError::new("a JSON value exceeds max_value_bytes")
                .at("limit", self.max as u64),
        }
    }
}

fn parse_value<T: DeserializeOwned>(value: &[u8]) -> Result<T, Fault> {
    let text = std::str::from_utf8(value).map_err(|e| Fault {
        kind: ReadErrorKind::Format,
        codec: CodecError::new("a JSON value is not valid UTF-8")
            .at("byte", e.valid_up_to() as u64),
    })?;
    serde_json::from_str(text).map_err(|e| {
        use serde_json::error::Category;
        let (kind, label) = match e.classify() {
            Category::Data => (
                ReadErrorKind::Decode,
                "a JSON value does not match the expected type",
            ),
            Category::Eof => (ReadErrorKind::Format, "a JSON value is incomplete"),
            Category::Syntax | Category::Io => {
                (ReadErrorKind::Format, "a JSON value is not valid JSON")
            }
        };
        Fault {
            kind,
            codec: CodecError::new(label)
                .at("line", e.line() as u64)
                .at("column", e.column() as u64)
                .with_source(e),
        }
    })
}

/// The head check of `reader` (its media policy), for the static [`JsonStream`](super::JsonStream).
pub(crate) fn check_head(
    reader: &JsonValuesReader,
    head: &concord_core::ResponseHead<'_>,
) -> Result<(), CodecError> {
    check_media(reader.media, &JSON, head)
}

const JSON: Family = Family {
    standard: |m| {
        m.type_() == mime::APPLICATION
            && match m.suffix() {
                None => m.subtype() == mime::JSON,
                Some(suffix) => suffix == mime::JSON,
            }
    },
    refused: "the response Content-Type is not application/json or an application/*+json type",
};

/// A reader of consecutive JSON values: objects, arrays, strings, numbers, booleans or `null`,
/// one after another, separated by optional whitespace (a Kubernetes watch stream:
/// `{"type":"ADDED",..}\n{"type":"MODIFIED",..}`). Values may span lines; no newline is required
/// between them.
///
/// - A value is complete when its closing `}`, `]` or `"` arrives. A number or literal is complete
///   only at whitespace, at the start of an object, array or string, or at the **true** end of
///   input: chunks `1` and `2 ` are the one value `12`, never two.
/// - A top-level array is one value (its elements are not yielded separately).
/// - Input that ends inside a value is a [`Format`](ReadErrorKind::Format) error; anything that
///   is not JSON (a byte order mark, `,` between values) is a `Format` error when the value it
///   belongs to is parsed. Errors are terminal; the reader is then fused. It does not reconnect
///   or resume a watch: that is the application's job.
/// - Media policy: [`standard`](MediaPolicy::standard) accepts `application/json` and
///   `application/*+json` (Kubernetes serves its JSON watch streams as `application/json`).
/// - Limits: [`max_value_bytes`](Self::max_value_bytes) (default 16 MiB) bounds the retained
///   unfinished value; optional totals report a [`Limit`](ReadErrorKind::Limit) error, never a
///   clean end. An unbounded watch needs no total.
#[derive(Clone, Debug)]
pub struct JsonValuesReader {
    max_value_bytes: usize,
    max_total_bytes: Option<u64>,
    max_values: Option<u64>,
    media: MediaPolicy,
}

impl Default for JsonValuesReader {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonValuesReader {
    /// A reader with the defaults: 16 MiB per value, no totals, the standard media policy.
    pub fn new() -> Self {
        Self {
            max_value_bytes: DEFAULT_MAX_RECORD_BYTES,
            max_total_bytes: None,
            max_values: None,
            media: MediaPolicy::standard(),
        }
    }

    /// The largest value, in bytes (from its first byte to its last, surrounding whitespace
    /// excluded). A longer one is a terminal [`Limit`](ReadErrorKind::Limit) error, detected before
    /// the reader buffers past this size.
    #[must_use]
    pub fn max_value_bytes(mut self, max: usize) -> Self {
        self.max_value_bytes = max;
        self
    }

    /// The most input bytes to read (a [`Limit`](ReadErrorKind::Limit) error past it).
    #[must_use]
    pub fn max_total_bytes(mut self, max: u64) -> Self {
        self.max_total_bytes = Some(max);
        self
    }

    /// The most values to deliver (a [`Limit`](ReadErrorKind::Limit) error for a further one).
    #[must_use]
    pub fn max_values(mut self, max: u64) -> Self {
        self.max_values = Some(max);
        self
    }

    /// The response media policy applied by [`open`](Self::open).
    #[must_use]
    pub fn media(mut self, media: MediaPolicy) -> Self {
        self.media = media;
        self
    }

    /// Opens a response ([`Openable`]: the [`Incoming`](concord_core::Incoming) body of an
    /// endpoint declared `-> stream`, a [`Received`](concord_core::Received) response, or a native
    /// response): requires a 2xx status and a `Content-Type` accepted by the media policy,
    /// before polling the body. A refusal keeps the response unread
    /// ([`OpenError::into_response`]).
    ///
    /// # Errors
    ///
    /// An [`OpenError`] of kind `Status` or `Metadata`.
    pub fn open<T: DeserializeOwned>(
        &self,
        response: impl Openable,
    ) -> Result<JsonValues<T>, OpenError> {
        let (response, operation) =
            open_input(response, |head| check_media(self.media, &JSON, head))?;
        Ok(self.values(response, operation))
    }

    /// The values of a body whose head a static format already checked (`Incremental`).
    pub(crate) fn open_checked<T: DeserializeOwned>(
        &self,
        body: concord_core::Incoming,
    ) -> JsonValues<T> {
        let (response, operation) = body.into_parts();
        self.values(response, operation.map(|op| op.name()))
    }

    /// [`open`](Self::open), naming the operation in later errors.
    ///
    /// # Errors
    ///
    /// An [`OpenError`] of kind `Status` or `Metadata`.
    pub fn open_for<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
        operation: &'static str,
    ) -> Result<JsonValues<T>, OpenError> {
        self.open_with(response, Some(operation))
    }

    fn open_with<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
        operation: Option<&'static str>,
    ) -> Result<JsonValues<T>, OpenError> {
        let response = open_response(response, operation, |head| {
            check_media(self.media, &JSON, head)
        })?;
        Ok(self.values(response, operation))
    }

    /// Reads any byte source, without status or head checks.
    pub fn read<T: DeserializeOwned, S: ByteSource>(&self, source: S) -> JsonValues<T, S> {
        self.values(source, None)
    }

    fn values<T, S: ByteSource>(
        &self,
        source: S,
        operation: Option<&'static str>,
    ) -> JsonValues<T, S> {
        JsonValues {
            driver: Driver::new(source, operation)
                .with_totals(self.max_total_bytes, self.max_values),
            scanner: Scanner::new(self.max_value_bytes),
            _values: PhantomData,
        }
    }
}

/// The values of one stream of consecutive JSON values (from [`JsonValuesReader::open`] or
/// [`read`](JsonValuesReader::read)). Like the JSON Lines records: nothing runs ahead of the
/// caller, at most the current chunk and one unfinished value are retained, and dropping it drops
/// the source.
pub struct JsonValues<T, S = reqwest::Response> {
    driver: Driver<S>,
    scanner: Scanner,
    _values: PhantomData<fn() -> T>,
}

impl<T: DeserializeOwned, S: ByteSource> JsonValues<T, S> {
    /// Whether the input ended cleanly ([`Completion::EndOfInput`](crate::stream::Completion)),
    /// a terminal error ended the reader (`Failed`), or records may follow (`Reading`). Only
    /// `EndOfInput` proves every record was read; stopping early or dropping the reader is not
    /// an end of input.
    pub fn completion(&self) -> crate::stream::Completion {
        self.driver.completion()
    }

    /// The next value; `None` at the end of input and after a terminal error (fused).
    ///
    /// Cancel-safe: dropping the returned future loses no value (a partly received value stays
    /// buffered for the next call, the rest of the chunk pending). Work per poll is bounded: it
    /// scans at most 64 KiB of input between two yields to the executor (whitespace, a large
    /// value, many ready chunks or one large chunk), and yields after every 64 empty chunks from
    /// the source. Parsing the completed value is not divided.
    pub async fn next(&mut self) -> Option<Result<T, ReadError>> {
        if self.driver.is_fused() {
            return None;
        }
        let mut work = 0usize;
        loop {
            if self.driver.input().is_empty() && !self.driver.at_eof() {
                match self.driver.refill().await {
                    Ok(_) => continue,
                    Err(e) => return Some(Err(e)),
                }
            }
            if work >= SKIP_BUDGET {
                work = 0;
                yield_now().await;
            }
            // At most `SKIP_BUDGET` input bytes are scanned between two yields; the rest of the
            // chunk stays in the driver. The end of a slice is not the end of input: `at_eof`
            // holds only once the chunk is used up (the slice is then empty).
            let input = self.driver.input();
            let input = &input[..input.len().min(SKIP_BUDGET - work)];
            let eof = self.driver.at_eof();
            let (used, scanned) = match self.scanner.scan(input, eof) {
                Ok(step) => step,
                Err(fault) => return Some(Err(self.driver.error(fault.kind, fault.codec))),
            };
            match scanned {
                Scanned::NeedMore | Scanned::Skipped => {
                    self.driver.consume(used);
                    work += used;
                }
                Scanned::End => {
                    self.driver.consume(used);
                    self.driver.finish();
                    return None;
                }
                Scanned::Value {
                    buffered,
                    start,
                    end,
                } => {
                    let taken;
                    let value = if buffered {
                        self.driver.consume(used);
                        &self.scanner.buf[..]
                    } else {
                        taken = self.driver.take(used);
                        &taken[start..end]
                    };
                    if let Err(e) = self.driver.check_record_total() {
                        return Some(Err(e));
                    }
                    let parsed = parse_value::<T>(value);
                    self.scanner.release();
                    return Some(match parsed {
                        Ok(v) => {
                            self.driver.delivered();
                            Ok(v)
                        }
                        Err(fault) => Err(self.driver.error(fault.kind, fault.codec)),
                    });
                }
            }
        }
    }

    /// Values delivered and input bytes consumed so far.
    pub fn position(&self) -> Position {
        self.driver.position()
    }

    /// Library-owned bytes held now for the unfinished value: the part of a value that started
    /// in an earlier chunk, or in an earlier scanning step of the current one (a step scans at
    /// most 64 KiB), and was copied into the reader's buffer (`0` between values, and while a
    /// value lies wholly inside the input of one step). The buffer's length; its allocation is
    /// bounded by `max_value_bytes` too. Excludes the unconsumed remainder of the current
    /// transport chunk ([`pending_input_bytes`]), which is the source's buffer, and the values
    /// already returned.
    ///
    /// [`pending_input_bytes`]: Self::pending_input_bytes
    pub fn retained_bytes(&self) -> usize {
        self.scanner.buf.len()
    }

    /// The largest [`retained_bytes`](Self::retained_bytes) over this reader's life, counting a
    /// complete value while it was parsed from the buffer. At most `max_value_bytes`.
    pub fn peak_retained_bytes(&self) -> usize {
        self.scanner.peak
    }

    /// The unconsumed remainder of the current transport chunk: bytes the source delivered and
    /// the reader holds (shared, not copied) but has not scanned yet.
    pub fn pending_input_bytes(&self) -> usize {
        self.driver.input().len()
    }

    /// The source, as the reader left it. Bytes the reader already took from it are not in it
    /// any more: a response recovered this way is partially consumed, never unread.
    pub fn into_source(self) -> S {
        self.driver.into_source()
    }
}

impl<T, S> fmt::Debug for JsonValues<T, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JsonValues")
            .field("state", &self.scanner.state)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds `chunks`, returning the complete value texts or the first fault label.
    fn values(scanner: &mut Scanner, chunks: &[&[u8]]) -> Result<Vec<Vec<u8>>, &'static str> {
        let mut out = Vec::new();
        let mut queue: std::collections::VecDeque<&[u8]> = chunks.iter().copied().collect();
        let mut input: &[u8] = &[];
        loop {
            if input.is_empty()
                && let Some(next) = queue.pop_front()
            {
                input = next;
                continue;
            }
            let eof = input.is_empty() && queue.is_empty();
            let (used, scanned) = scanner.scan(input, eof).map_err(|f| f.codec.label())?;
            match scanned {
                Scanned::End => return Ok(out),
                Scanned::NeedMore => assert_eq!(used, input.len()),
                Scanned::Skipped => {}
                Scanned::Value {
                    buffered,
                    start,
                    end,
                } => out.push(if buffered {
                    scanner.buf.clone()
                } else {
                    input[start..end].to_vec()
                }),
            }
            input = &input[used..];
        }
    }

    #[test]
    fn primitives_end_only_at_delimiters_or_true_eof() {
        let mut s = Scanner::new(100);
        assert_eq!(
            values(&mut s, &[b"1", b"2 "]).unwrap(),
            vec![b"12".to_vec()]
        );
        let mut s = Scanner::new(100);
        assert_eq!(values(&mut s, &[b"1", b"2"]).unwrap(), vec![b"12".to_vec()]);
        let mut s = Scanner::new(100);
        assert_eq!(
            values(&mut s, &[b"12{\"a\":1}tr", b"ue[1]\"x\"null"]).unwrap(),
            vec![
                b"12".to_vec(),
                b"{\"a\":1}".to_vec(),
                b"true".to_vec(),
                b"[1]".to_vec(),
                b"\"x\"".to_vec(),
                b"null".to_vec()
            ]
        );
    }

    #[test]
    fn strings_and_escapes_hide_structure() {
        let mut s = Scanner::new(100);
        let input = br#"{"a":"}{\"]["}"\\""x\"""#;
        for split in 0..=input.len() {
            let (a, b) = input.split_at(split);
            assert_eq!(
                values(&mut Scanner::new(100), &[a, b]).unwrap(),
                vec![
                    br#"{"a":"}{\"]["}"#.to_vec(),
                    br#""\\""#.to_vec(),
                    br#""x\"""#.to_vec()
                ],
                "split {split}"
            );
        }
        assert_eq!(
            values(&mut s, &[b"\"abc"]),
            Err("the input ends inside a JSON value")
        );
    }

    #[test]
    fn limit_before_buffering_past_it() {
        // Exactly `max` passes, in any chunking; one more byte fails without buffering it.
        let max = 16;
        let exact = format!("[\"{}\"]", "x".repeat(max - 4));
        let over = format!("[\"{}\"]", "x".repeat(max - 3));
        let number = "1".repeat(max + 1);
        for chunk in 1..=over.len() {
            let mut s = Scanner::new(max);
            let chunks: Vec<&[u8]> = exact.as_bytes().chunks(chunk).collect();
            assert_eq!(values(&mut s, &chunks).unwrap().len(), 1);
            assert!(s.buf.capacity() <= max, "capacity {}", s.buf.capacity());
            let mut s = Scanner::new(max);
            let chunks: Vec<&[u8]> = over.as_bytes().chunks(chunk).collect();
            assert_eq!(
                values(&mut s, &chunks),
                Err("a JSON value exceeds max_value_bytes")
            );
            assert!(s.buf.capacity() <= max);
            let mut s = Scanner::new(max);
            let chunks: Vec<&[u8]> = number.as_bytes().chunks(chunk).collect();
            assert_eq!(
                values(&mut s, &chunks),
                Err("a JSON value exceeds max_value_bytes")
            );
            assert!(s.buf.capacity() <= max);
        }
        // A primitive of exactly `max` bytes followed by its delimiter.
        let mut s = Scanner::new(4);
        assert_eq!(
            values(&mut s, &[b"1234 5"]).unwrap(),
            vec![b"1234".to_vec(), b"5".to_vec()]
        );
        let mut s = Scanner::new(4);
        assert_eq!(values(&mut s, &[b"1234"]).unwrap(), vec![b"1234".to_vec()]);
    }

    #[test]
    fn whitespace_runs_are_skipped_in_budgeted_steps() {
        let input = [vec![b' '; 3 * SKIP_BUDGET + 5], b"7".to_vec()].concat();
        let mut s = Scanner::new(100);
        let mut steps = 0;
        let mut rest: &[u8] = &input;
        loop {
            let (used, scanned) = s.scan(rest, true).map_err(|f| f.codec.label()).unwrap();
            steps += 1;
            rest = &rest[used..];
            if let Scanned::Value { .. } = scanned {
                break;
            }
            assert_eq!(scanned, Scanned::Skipped);
            assert!(used <= SKIP_BUDGET);
        }
        assert_eq!(steps, 4);
    }
}

impl<T, S> crate::stream::RecordSealed for JsonValues<T, S> {}

impl<T, S> crate::stream::RecordReader for JsonValues<T, S>
where
    T: DeserializeOwned + Send + 'static,
    S: ByteSource + 'static,
{
    type Record = T;

    fn next_record(
        &mut self,
    ) -> impl std::future::Future<Output = Option<Result<T, ReadError>>> + Send {
        self.next()
    }

    fn record_position(&self) -> Position {
        self.position()
    }

    fn record_operation(&self) -> Option<&'static str> {
        self.driver.operation()
    }

    fn record_completion(&self) -> crate::stream::Completion {
        self.driver.completion()
    }
}

impl<T, S> JsonValues<T, S>
where
    T: DeserializeOwned + Send + 'static,
    S: ByteSource + 'static,
{
    /// This reader as an owning standard [`Stream`](futures_core::Stream) of
    /// `Result<T, ReadError>` ([`RecordStream`](crate::stream::RecordStream)): no read-ahead,
    /// cancellation-safe, errors and completion unchanged.
    pub fn into_stream(self) -> crate::stream::RecordStream<Self> {
        crate::stream::RecordStream::new(self)
    }
}
