//! JSON Lines framing (shared by the finite `Decode` and the incremental reader) and the
//! incremental reader itself.

use std::fmt;
use std::marker::PhantomData;

use concord_core::{CodecError, reqwest};
use serde::de::DeserializeOwned;

use super::policy::{
    BlankLines, BomPolicy, Family, Flavor, JsonLinesOptions, RecordErrors, check_media,
};
use crate::stream::{
    ByteSource, Driver, OpenError, Openable, Position, ReadError, ReadErrorKind, SKIP_BUDGET,
    open_input, open_response, yield_now,
};

const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// A failure found while framing or parsing a line.
pub(super) struct Fault {
    pub(super) kind: ReadErrorKind,
    pub(super) codec: CodecError,
}

impl Fault {
    fn new(kind: ReadErrorKind, codec: CodecError) -> Self {
        Self { kind, codec }
    }
}

/// One framing step.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Framed {
    /// A complete line, its terminator (LF, or CRLF) removed: `input[start..end]` of the input
    /// passed in, or, when `buffered`, the framer's [`line`](Framer::line) buffer.
    Line {
        buffered: bool,
        start: usize,
        end: usize,
        /// `false` for a final line without LF at the true end of input: if it does not parse,
        /// the record may have been cut off, so even `Yield` mode treats that fault as terminal.
        terminated: bool,
    },
    /// The input is used up inside a line (kept in the buffer) or between lines.
    NeedMore,
    /// True end of input, after a complete line or at the very start.
    End,
}

/// Splits input into lines across arbitrary chunk boundaries. It retains only the unfinished
/// line, at most `max` bytes plus one pending CR (a line that would need more is a
/// [`ReadErrorKind::Limit`] fault, detected before the buffer grows).
pub(super) struct Framer {
    line: Vec<u8>,
    /// Complete lines framed so far.
    lines: u64,
    bom_done: bool,
    /// The buffer holds the line last returned: clear it on the next step.
    release: bool,
    /// The most bytes the buffer ever held.
    peak: usize,
    max: usize,
    bom: BomPolicy,
    /// Bytes searched for a line feed so far (for the resource tests).
    #[cfg(test)]
    searched: usize,
}

impl Framer {
    pub(super) fn new(options: &JsonLinesOptions) -> Self {
        Self {
            line: Vec::new(),
            lines: 0,
            bom_done: false,
            release: false,
            peak: 0,
            max: options.max_record_bytes,
            bom: options.bom,
            #[cfg(test)]
            searched: 0,
        }
    }

    /// The buffered line of a [`Framed::Line`] with `buffered: true`.
    pub(super) fn line(&self) -> &[u8] {
        &self.line
    }

    /// The 1-based number of the line last framed.
    pub(super) fn line_number(&self) -> u64 {
        self.lines
    }

    /// Clears a buffered line once its caller is done with it.
    pub(super) fn release(&mut self) {
        if self.release {
            self.line.clear();
            self.release = false;
        }
    }

    /// Bytes held in the line buffer now.
    pub(super) fn retained(&self) -> usize {
        self.line.len()
    }

    /// The most bytes the line buffer ever held.
    pub(super) fn peak(&self) -> usize {
        self.peak
    }

    /// The retained buffer's capacity (for the resource tests).
    #[cfg(test)]
    pub(super) fn capacity(&self) -> usize {
        self.line.capacity()
    }

    /// Frames from `input` (the unconsumed part of the current chunk, or a slice of it); `eof`: the
    /// source has ended and `input` is the last of it (never set for a slice that leaves input
    /// behind). Returns how many bytes of `input` were consumed. A fault is terminal.
    ///
    /// The search for a line feed covers at most `max` content bytes, a CR and the LF itself from
    /// the line's start: a longer run without one is a limit fault however much input follows.
    pub(super) fn frame(&mut self, input: &[u8], eof: bool) -> Result<(usize, Framed), Fault> {
        self.release();
        let mut used = 0;
        if !self.bom_done {
            // The first bytes go to the line buffer while they match the mark's prefix: a mark
            // split across chunks is still recognized, and anything else is ordinary content.
            while self.line.len() < BOM.len() {
                match input.get(used) {
                    Some(&b) if b == BOM[self.line.len()] => {
                        self.push(&[b]);
                        used += 1;
                    }
                    Some(_) => break,
                    None if eof => break,
                    None => return Ok((used, Framed::NeedMore)),
                }
            }
            self.bom_done = true;
            if self.line == BOM {
                match self.bom {
                    BomPolicy::Refuse => {
                        return Err(Fault::new(
                            ReadErrorKind::Format,
                            CodecError::new(
                                "the input starts with a byte order mark, which this profile refuses",
                            )
                            .at("line", 1),
                        ));
                    }
                    BomPolicy::Skip => self.line.clear(),
                }
            }
        }
        let rest = &input[used..];
        // A terminator can follow at most `max` content bytes and one CR: look no further.
        let reach = rest
            .len()
            .min(self.max.saturating_add(2).saturating_sub(self.line.len()));
        #[cfg(test)]
        {
            self.searched += reach;
        }
        if let Some(i) = rest[..reach].iter().position(|&b| b == b'\n') {
            let cr = if i > 0 {
                rest[i - 1] == b'\r'
            } else {
                self.line.last() == Some(&b'\r')
            };
            let content = self.line.len() + i - usize::from(cr);
            if content > self.max {
                return Err(self.limit());
            }
            self.lines += 1;
            let consumed = used + i + 1;
            if self.line.is_empty() {
                let end = used + i - usize::from(cr);
                return Ok((
                    consumed,
                    Framed::Line {
                        buffered: false,
                        start: used,
                        end,
                        terminated: true,
                    },
                ));
            }
            self.push(&rest[..i]);
            if cr {
                self.line.pop();
            }
            self.release = true;
            return Ok((
                consumed,
                Framed::Line {
                    buffered: true,
                    start: 0,
                    end: self.line.len(),
                    terminated: true,
                },
            ));
        }
        if reach < rest.len() {
            // No LF within reach: more than `max` content bytes precede any terminator.
            return Err(self.limit());
        }
        let total = self.line.len() + rest.len();
        if eof {
            if total == 0 {
                return Ok((used, Framed::End));
            }
            // A final line without LF, accepted at the true end of input. A CR it ends with is
            // content (no LF follows).
            if total > self.max {
                return Err(self.limit());
            }
            self.lines += 1;
            if self.line.is_empty() {
                return Ok((
                    input.len(),
                    Framed::Line {
                        buffered: false,
                        start: used,
                        end: input.len(),
                        terminated: false,
                    },
                ));
            }
            self.push(rest);
            self.release = true;
            return Ok((
                input.len(),
                Framed::Line {
                    buffered: true,
                    start: 0,
                    end: self.line.len(),
                    terminated: false,
                },
            ));
        }
        // Unfinished: a trailing CR may still be the start of a CRLF terminator.
        let last = rest.last().or(self.line.last());
        let content = total - usize::from(last == Some(&b'\r'));
        if content > self.max {
            return Err(self.limit());
        }
        self.push(rest);
        Ok((input.len(), Framed::NeedMore))
    }

    fn limit(&self) -> Fault {
        Fault::new(
            ReadErrorKind::Limit,
            CodecError::new("a line exceeds max_record_bytes")
                .at("line", self.lines + 1)
                .at("limit", self.max as u64),
        )
    }

    /// Appends to the line buffer without letting its capacity exceed the bound (`max` plus a
    /// pending CR) for a line that respects it.
    fn push(&mut self, bytes: &[u8]) {
        let needed = self.line.len() + bytes.len();
        if needed > self.line.capacity() {
            let bound = self.max.saturating_add(1).max(needed);
            let target = (self.line.capacity() * 2).max(needed).min(bound);
            self.line.reserve_exact(target - self.line.len());
        }
        self.line.extend_from_slice(bytes);
        self.peak = self.peak.max(self.line.len());
    }
}

/// Whether a line is blank: empty, or only spaces, tabs and CR.
pub(super) fn is_blank(line: &[u8]) -> bool {
    line.iter().all(|b| matches!(b, b' ' | b'\t' | b'\r'))
}

/// A blank line where the profile refuses them.
pub(super) fn blank_fault(line: u64) -> Fault {
    Fault::new(
        ReadErrorKind::Format,
        CodecError::new("a line is blank, which this profile refuses").at("line", line),
    )
}

/// Parses one complete, non-blank line: UTF-8 first (whatever `T` reads), then one JSON value.
pub(super) fn parse_line<T: DeserializeOwned>(line: &[u8], number: u64) -> Result<T, Fault> {
    let text = std::str::from_utf8(line).map_err(|e| {
        Fault::new(
            ReadErrorKind::Format,
            CodecError::new("a line is not valid UTF-8")
                .at("line", number)
                .at("column", e.valid_up_to() as u64 + 1),
        )
    })?;
    serde_json::from_str(text).map_err(|e| {
        use serde_json::error::Category;
        let (kind, label) = match e.classify() {
            Category::Data => (
                ReadErrorKind::Decode,
                "a record does not match the expected type",
            ),
            Category::Eof => (ReadErrorKind::Format, "a line ends inside a JSON value"),
            Category::Syntax | Category::Io => {
                (ReadErrorKind::Format, "a line is not one valid JSON value")
            }
        };
        Fault::new(
            kind,
            CodecError::new(label)
                .at("line", number)
                .at("column", e.column() as u64)
                .with_source(e),
        )
    })
}

/// The finite path: every record of a complete body, with the same framing and parsing as the
/// incremental reader. Used with the presets (record errors `Stop`, no totals).
pub(super) fn decode_all<T: DeserializeOwned>(
    options: &JsonLinesOptions,
    body: &[u8],
) -> Result<Vec<T>, CodecError> {
    let mut framer = Framer::new(options);
    let mut records = Vec::new();
    let mut at = 0;
    loop {
        let (used, framed) = framer.frame(&body[at..], true).map_err(|f| f.codec)?;
        let base = at;
        at += used;
        let (buffered, start, end) = match framed {
            Framed::End => return Ok(records),
            Framed::NeedMore => continue,
            Framed::Line {
                buffered,
                start,
                end,
                ..
            } => (buffered, start, end),
        };
        let line = if buffered {
            framer.line()
        } else {
            &body[base + start..base + end]
        };
        if is_blank(line) {
            match options.blank_lines {
                BlankLines::Ignore => continue,
                BlankLines::Refuse => return Err(blank_fault(framer.line_number()).codec),
            }
        }
        records.push(parse_line(line, framer.line_number()).map_err(|f| f.codec)?);
    }
}

pub(super) const JSON_LINES: Family = Family {
    standard: |m| {
        is_one_of(
            m,
            &[
                "application/jsonl",
                "application/jsonlines",
                "application/x-jsonlines",
            ],
        )
    },
    refused: "the response Content-Type is not a JSON Lines media type (application/jsonl, application/jsonlines, application/x-jsonlines)",
};

pub(super) const NDJSON: Family = Family {
    standard: |m| is_one_of(m, &["application/x-ndjson", "application/ndjson"]),
    refused: "the response Content-Type is not an NDJSON media type (application/x-ndjson, application/ndjson)",
};

fn is_one_of(m: &mime::Mime, list: &[&str]) -> bool {
    list.iter().any(|e| m.essence_str().eq_ignore_ascii_case(e))
}

pub(super) fn family(flavor: Flavor) -> &'static Family {
    match flavor {
        Flavor::JsonLines => &JSON_LINES,
        Flavor::Ndjson => &NDJSON,
    }
}

/// An incremental JSON Lines / NDJSON reader: an immutable, validated configuration that opens
/// responses (or reads any [`ByteSource`]) as [`JsonLinesRecords`].
///
/// ```no_run
/// # async fn demo(response: concord_core::reqwest::Response) -> Result<(), Box<dyn std::error::Error>> {
/// use concord_extras::jsonl::{JsonLinesOptions, JsonLinesReader, MediaPolicy};
///
/// #[derive(serde::Deserialize)]
/// struct Product { id: String }
///
/// let reader = JsonLinesReader::new(
///     JsonLinesOptions::json_lines()
///         .media(MediaPolicy::standard_or(&["application/octet-stream"]))
///         .max_record_bytes(1024 * 1024),
/// );
/// let mut records = reader.open_for::<Product>(response, "bulk_export")?;
/// while let Some(product) = records.next().await {
///     let product = product?;
///     # let _ = product.id;
/// }
/// # Ok(()) }
/// ```
#[derive(Clone, Debug)]
pub struct JsonLinesReader {
    options: JsonLinesOptions,
}

impl JsonLinesReader {
    /// A reader with these options.
    pub fn new(options: JsonLinesOptions) -> Self {
        Self { options }
    }

    /// The options.
    pub fn options(&self) -> &JsonLinesOptions {
        &self.options
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
    ) -> Result<JsonLinesRecords<T>, OpenError> {
        let family = family(self.options.flavor);
        let (response, operation) = open_input(response, |head| {
            check_media(self.options.media, family, head)
        })?;
        Ok(self.records(response, operation))
    }

    /// The values of a body whose head a static format already checked (`Incremental`).
    pub(crate) fn open_checked<T: DeserializeOwned>(
        &self,
        body: concord_core::Incoming,
    ) -> JsonLinesRecords<T> {
        let (response, operation) = body.into_parts();
        self.records(response, operation.map(|op| op.name()))
    }

    /// [`open`](Self::open), naming the operation in later errors (a static label supplied by the
    /// SDK, never derived from the URL).
    ///
    /// # Errors
    ///
    /// An [`OpenError`] of kind `Status` or `Metadata`.
    pub fn open_for<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
        operation: &'static str,
    ) -> Result<JsonLinesRecords<T>, OpenError> {
        self.open_with(response, Some(operation))
    }

    fn open_with<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
        operation: Option<&'static str>,
    ) -> Result<JsonLinesRecords<T>, OpenError> {
        let family = family(self.options.flavor);
        let response = open_response(response, operation, |head| {
            check_media(self.options.media, family, head)
        })?;
        Ok(self.records(response, operation))
    }

    /// Reads any byte source, without status or head checks.
    pub fn read<T: DeserializeOwned, S: ByteSource>(&self, source: S) -> JsonLinesRecords<T, S> {
        self.records(source, None)
    }

    fn records<T, S: ByteSource>(
        &self,
        source: S,
        operation: Option<&'static str>,
    ) -> JsonLinesRecords<T, S> {
        JsonLinesRecords {
            driver: Driver::new(source, operation)
                .with_totals(self.options.max_total_bytes, self.options.max_records),
            framer: Framer::new(&self.options),
            blank_lines: self.options.blank_lines,
            record_errors: self.options.record_errors,
            _records: PhantomData,
        }
    }
}

/// The records of one JSON Lines body, read as chunks arrive (from
/// [`JsonLinesReader::open`] or [`read`](JsonLinesReader::read)).
///
/// Nothing runs ahead of the caller: each [`next`](Self::next) reads only the chunks needed for
/// one record. It retains the current chunk and the unfinished line (bounded by
/// `max_record_bytes`), never a queue of records. Dropping it drops the source (a response's
/// connection is closed, not drained).
///
/// With [`RecordErrors::Yield`], a bad line ended by a line feed is a nonterminal `Err` item;
/// a bad final line without one (it may have been cut off) is terminal, like every limit, byte
/// order mark and transport failure ([`ReadError::is_terminal`]).
pub struct JsonLinesRecords<T, S = reqwest::Response> {
    driver: Driver<S>,
    framer: Framer,
    blank_lines: BlankLines,
    record_errors: RecordErrors,
    _records: PhantomData<fn() -> T>,
}

impl<T: DeserializeOwned, S: ByteSource> JsonLinesRecords<T, S> {
    /// Whether the input ended cleanly ([`Completion::EndOfInput`](crate::stream::Completion)),
    /// a terminal error ended the reader (`Failed`), or records may follow (`Reading`). Only
    /// `EndOfInput` proves every record was read; stopping early or dropping the reader is not
    /// an end of input.
    pub fn completion(&self) -> crate::stream::Completion {
        self.driver.completion()
    }

    /// The next record; `None` at the end of input and after a terminal error (fused).
    ///
    /// Cancel-safe: dropping the returned future loses no record (a partly received line stays
    /// buffered for the next call, the rest of the chunk pending). Work per poll is bounded: it
    /// frames at most 64 KiB of input between two yields to the executor (blank lines, a long
    /// line, many ready chunks or one large chunk), and yields after every 64 empty chunks from
    /// the source. Parsing the completed line is not divided.
    pub async fn next(&mut self) -> Option<Result<T, ReadError>> {
        if self.driver.is_fused() {
            return None;
        }
        let mut work = 0usize;
        let mut unterminated = false;
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
            // At most `SKIP_BUDGET` input bytes are framed between two yields; the rest of the
            // chunk stays in the driver. The end of a slice is not the end of input: `at_eof`
            // holds only once the chunk is used up (the slice is then empty).
            let input = self.driver.input();
            let input = &input[..input.len().min(SKIP_BUDGET - work)];
            let eof = self.driver.at_eof();
            let (used, framed) = match self.framer.frame(input, eof) {
                Ok(step) => step,
                Err(fault) => return Some(Err(self.driver.error(fault.kind, fault.codec))),
            };
            work += used;
            let outcome = match framed {
                Framed::NeedMore => {
                    self.driver.consume(used);
                    None
                }
                Framed::End => {
                    self.driver.consume(used);
                    self.driver.finish();
                    return None;
                }
                Framed::Line {
                    buffered,
                    start,
                    end,
                    terminated,
                } => {
                    unterminated = !terminated;
                    let taken;
                    let line = if buffered {
                        self.driver.consume(used);
                        self.framer.line()
                    } else {
                        taken = self.driver.take(used);
                        &taken[start..end]
                    };
                    let number = self.framer.line_number();
                    let outcome = if is_blank(line) {
                        match self.blank_lines {
                            BlankLines::Ignore => None,
                            BlankLines::Refuse => Some(Err(blank_fault(number))),
                        }
                    } else if let Err(e) = self.driver.check_record_total() {
                        return Some(Err(e));
                    } else {
                        Some(parse_line::<T>(line, number))
                    };
                    self.framer.release();
                    outcome
                }
            };
            match outcome {
                Some(Ok(record)) => {
                    self.driver.delivered();
                    return Some(Ok(record));
                }
                Some(Err(fault)) => {
                    return Some(Err(match self.record_errors {
                        RecordErrors::Stop => self.driver.error(fault.kind, fault.codec),
                        // A final line without LF that does not parse may be a cut-off record.
                        RecordErrors::Yield if unterminated => {
                            self.driver.error(fault.kind, fault.codec)
                        }
                        RecordErrors::Yield => {
                            ReadError::new(fault.kind, fault.codec, self.driver.position())
                                .with_operation(self.driver.operation())
                                .recoverable()
                        }
                    }));
                }
                None => {}
            }
        }
    }

    /// Records delivered and input bytes consumed so far.
    pub fn position(&self) -> Position {
        self.driver.position()
    }

    /// Library-owned bytes held now for the unfinished line: the part of a line that started in
    /// an earlier chunk, or in an earlier framing step of the current one (a step frames at most
    /// 64 KiB), and was copied into the reader's line buffer (`0` between lines, and while a line
    /// lies wholly inside the input of one step). The buffer's length, not its allocation; that
    /// is bounded too, by `max_record_bytes` plus one pending CR. Excludes the unconsumed
    /// remainder of the current transport chunk ([`pending_input_bytes`]), which is the source's
    /// buffer, and the records already returned.
    ///
    /// [`pending_input_bytes`]: Self::pending_input_bytes
    pub fn retained_bytes(&self) -> usize {
        self.framer.retained()
    }

    /// The largest [`retained_bytes`](Self::retained_bytes) over this reader's life, counting a
    /// complete line while it was parsed from the buffer. At most `max_record_bytes + 1`.
    pub fn peak_retained_bytes(&self) -> usize {
        self.framer.peak()
    }

    /// The unconsumed remainder of the current transport chunk: bytes the source delivered and
    /// the reader holds (shared, not copied) but has not framed yet.
    pub fn pending_input_bytes(&self) -> usize {
        self.driver.input().len()
    }

    /// The source, as the reader left it. Bytes the reader already took from it (the retained
    /// line, the pending chunk remainder, delivered records) are not in it any more: a response
    /// recovered this way is partially consumed, never unread.
    pub fn into_source(self) -> S {
        self.driver.into_source()
    }
}

impl<T, S> fmt::Debug for JsonLinesRecords<T, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JsonLinesRecords")
            .field("lines", &self.framer.lines)
            .field("blank_lines", &self.blank_lines)
            .field("record_errors", &self.record_errors)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn framer(max: usize) -> Framer {
        Framer::new(&JsonLinesOptions::json_lines().max_record_bytes(max))
    }

    /// Feeds `chunks`, returning the lines framed (terminators removed) or the first fault label.
    fn lines(framer: &mut Framer, chunks: &[&[u8]]) -> Result<Vec<Vec<u8>>, &'static str> {
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
            let (used, framed) = framer.frame(input, eof).map_err(|f| f.codec.label())?;
            match framed {
                Framed::End => return Ok(out),
                Framed::NeedMore => assert_eq!(used, input.len()),
                Framed::Line {
                    buffered,
                    start,
                    end,
                    ..
                } => out.push(if buffered {
                    framer.line().to_vec()
                } else {
                    input[start..end].to_vec()
                }),
            }
            input = &input[used..];
        }
    }

    #[test]
    fn terminators_and_final_line() {
        let mut f = framer(100);
        assert_eq!(
            lines(&mut f, &[b"a\r\nb\nc"]).unwrap(),
            vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]
        );
        let mut f = framer(100);
        assert_eq!(
            lines(&mut f, &[b"a\r", b"\nb\r", b"x\n"]).unwrap(),
            vec![b"a".to_vec(), b"b\rx".to_vec()]
        );
        let mut f = framer(100);
        assert_eq!(lines(&mut f, &[b"a\n"]).unwrap(), vec![b"a".to_vec()]);
        let mut f = framer(100);
        assert_eq!(lines(&mut f, &[]).unwrap(), Vec::<Vec<u8>>::new());
    }

    #[test]
    fn bom_split_and_lookalikes() {
        let mut f = framer(100);
        assert_eq!(
            lines(&mut f, &[b"\xEF", b"\xBB", b"\xBF1\n"]),
            Err("the input starts with a byte order mark, which this profile refuses")
        );
        let mut f = Framer::new(&JsonLinesOptions::json_lines().bom(BomPolicy::Skip));
        assert_eq!(
            lines(&mut f, &[b"\xEF", b"\xBB", b"\xBF1\n"]).unwrap(),
            vec![b"1".to_vec()]
        );
        // A partial mark is content.
        let mut f = framer(100);
        assert_eq!(
            lines(&mut f, &[b"\xEF\xBB", b"1\n"]).unwrap(),
            vec![b"\xEF\xBB1".to_vec()]
        );
        let mut f = framer(100);
        assert_eq!(lines(&mut f, &[b"\xEF"]).unwrap(), vec![b"\xEF".to_vec()]);
    }

    /// A line longer than the limit fails before the buffer grows past `max + 1`, whatever the
    /// chunking; a line of exactly `max` content bytes (plus CRLF) passes.
    #[test]
    fn limit_before_growth() {
        let max = 64;
        let exact = [b"x".repeat(max), b"\r\n".to_vec()].concat();
        for chunk in 1..=exact.len() {
            let mut f = framer(max);
            let chunks: Vec<&[u8]> = exact.chunks(chunk).collect();
            assert_eq!(lines(&mut f, &chunks).unwrap(), vec![b"x".repeat(max)]);
            assert!(f.capacity() <= max + 1, "capacity {}", f.capacity());
        }
        let over = [b"x".repeat(max + 1), b"\n".to_vec()].concat();
        for chunk in 1..=over.len() {
            let mut f = framer(max);
            let chunks: Vec<&[u8]> = over.chunks(chunk).collect();
            assert_eq!(
                lines(&mut f, &chunks),
                Err("a line exceeds max_record_bytes")
            );
            assert!(f.capacity() <= max + 1, "capacity {}", f.capacity());
        }
        // A CR that turns out not to start a terminator counts.
        let mut f = framer(3);
        assert_eq!(
            lines(&mut f, &[b"abc\r", b"d\n"]),
            Err("a line exceeds max_record_bytes")
        );
        let mut f = framer(3);
        assert_eq!(
            lines(&mut f, &[b"abc\r", b"\n"]).unwrap(),
            vec![b"abc".to_vec()]
        );
        // At EOF a trailing CR is content.
        let mut f = framer(3);
        assert_eq!(
            lines(&mut f, &[b"abc\r"]),
            Err("a line exceeds max_record_bytes")
        );
    }

    /// The search for a line feed stops within reach of the limit: a run without one is refused
    /// after inspecting at most `max + 2` bytes however large the input (the reach shrinks with
    /// what is already buffered), while a line of exactly `max` bytes before a CRLF is found.
    #[test]
    fn lookahead_stops_within_reach_of_the_limit() {
        let large = vec![b'x'; 1 << 20];
        for eof in [false, true] {
            let mut f = framer(64);
            let step = f.frame(&large, eof).map_err(|f| f.codec.label());
            assert_eq!(step, Err("a line exceeds max_record_bytes"));
            assert!(f.searched <= 66, "searched {}", f.searched);
        }
        let mut f = framer(64);
        let step = f.frame(&[b'x'; 60], false).map_err(|f| f.codec.label());
        assert_eq!(step, Ok((60, Framed::NeedMore)));
        f.searched = 0;
        let step = f.frame(&large, false).map_err(|f| f.codec.label());
        assert_eq!(step, Err("a line exceeds max_record_bytes"));
        assert!(f.searched <= 6, "searched {}", f.searched);

        let mut f = framer(64);
        let input = [b"y".repeat(64), b"\r\n".to_vec(), large].concat();
        let step = f.frame(&input, false).map_err(|f| f.codec.label());
        assert_eq!(
            step,
            Ok((
                66,
                Framed::Line {
                    buffered: false,
                    start: 0,
                    end: 64,
                    terminated: true
                }
            ))
        );
    }

    /// The buffer holds one unfinished line, not the input: feeding many records in tiny chunks
    /// keeps its capacity at the size of the longest line.
    #[test]
    fn retains_only_the_unfinished_line() {
        let mut f = framer(DEFAULT);
        let record = b"{\"id\":12345,\"name\":\"abcdefghij\"}\n";
        for _ in 0..1000 {
            for mut chunk in record.chunks(3) {
                while !chunk.is_empty() {
                    let (used, _) = f.frame(chunk, false).map_err(|f| f.codec.label()).unwrap();
                    chunk = &chunk[used..];
                }
            }
        }
        assert!(f.capacity() < 2 * record.len(), "capacity {}", f.capacity());
    }

    const DEFAULT: usize = super::super::policy::DEFAULT_MAX_RECORD_BYTES;
}

impl<T, S> crate::stream::RecordSealed for JsonLinesRecords<T, S> {}

impl<T, S> crate::stream::RecordReader for JsonLinesRecords<T, S>
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

impl<T, S> JsonLinesRecords<T, S>
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
