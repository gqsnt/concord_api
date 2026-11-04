//! The one framing implementation of the finite and incremental routes: `csv-core` records from
//! arbitrary input slices, with the record/header/field limits enforced before the owned buffers
//! grow past them, the BOM policy, header consumption and validation, and the width rule.
//!
//! [`Table::step`] is synchronous and takes any slice of the remaining input (an empty slice is
//! the true end of input). The finite decoder hands it the whole body; the incremental reader
//! hands it each chunk, at most [`SKIP_BUDGET`](crate::stream::SKIP_BUDGET) bytes at a time.

use std::collections::HashSet;
use std::sync::Arc;

use ::csv::{ByteRecord, StringRecord};
use concord_core::CodecError;
use csv_core::ReadRecordResult;

use super::options::{BomPolicy, Config};
use super::record::RawRecord;
use crate::stream::ReadErrorKind;

const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
/// The first allocation of the field buffer (or the limit, if smaller).
const MIN_FIELD_BYTES: usize = 256;
/// The first allocation of the field end buffer (or the limit, if smaller).
const MIN_FIELDS: usize = 16;

/// A table failure: the reader's error kind and a static description with positions.
#[derive(Debug)]
pub(crate) struct TableError {
    pub(crate) kind: ReadErrorKind,
    pub(crate) codec: CodecError,
}

impl TableError {
    pub(crate) fn new(kind: ReadErrorKind, codec: CodecError) -> Self {
        Self { kind, codec }
    }

    fn format(codec: CodecError) -> Self {
        Self::new(ReadErrorKind::Format, codec)
    }

    fn limit(codec: CodecError) -> Self {
        Self::new(ReadErrorKind::Limit, codec)
    }
}

/// What one [`Table::step`] produced.
pub(crate) enum Step {
    /// Input consumed (or a buffer grown, or the header taken): call again.
    Progress,
    /// A complete data record of valid width.
    Record(ByteRecord),
    /// The end of the table.
    End,
}

/// Whether records are mapped by name (the header must then be UTF-8, without empty or repeated
/// names) or kept raw/positional.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Typed,
    Raw,
}

enum Bom {
    /// Matching a possible BOM: `held` bytes of it seen so far.
    Detect {
        held: usize,
    },
    /// A partial BOM turned out to be data: its first `len` bytes go to the parser, from `at`.
    Replay {
        at: usize,
        len: usize,
    },
    Done,
}

/// Where the parser stands in the current physical line, tracked only when a comment byte is
/// configured: `csv-core` 0.1.13's DFA turns a final comment line without a terminator into a
/// record of one empty field (its NFA does not), and this tells that record apart from a genuine
/// final `""`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Line {
    /// At the start of a record (blank lines included).
    Start,
    /// Inside a comment line.
    Comment,
    /// Inside a record.
    Record,
}

pub(crate) struct Table {
    config: Arc<Config>,
    named: bool,
    core: csv_core::Reader,
    /// The unescaped field bytes of the record being parsed: `fields[..fields_len]`.
    fields: Vec<u8>,
    fields_len: usize,
    /// Cumulative field end offsets of the record being parsed: `ends[..ends_len]`.
    ends: Vec<usize>,
    ends_len: usize,
    bom: Bom,
    /// The parser has received input (its own BOM stripping is then disabled).
    started: bool,
    comment: Option<u8>,
    line: Line,
    /// The most bytes the field and end buffers have held.
    peak: usize,
    header: Option<RawRecord>,
    names: Option<StringRecord>,
    width: Option<usize>,
    records: u64,
    finished: bool,
}

impl Table {
    pub(crate) fn new(config: Arc<Config>, mode: Mode) -> Self {
        let named = mode == Mode::Typed && config.has_header() && !config.positional();
        let bom = match config.bom() {
            BomPolicy::Preserve => Bom::Done,
            _ => Bom::Detect { held: 0 },
        };
        let width = config.width;
        let comment = config.comment();
        Self {
            core: config.core_reader(),
            named,
            config,
            fields: Vec::new(),
            fields_len: 0,
            ends: Vec::new(),
            ends_len: 0,
            bom,
            started: false,
            comment,
            line: Line::Start,
            peak: 0,
            header: None,
            names: None,
            width,
            records: 0,
            finished: false,
        }
    }

    /// The header record, once parsed.
    pub(crate) fn header(&self) -> Option<&RawRecord> {
        self.header.as_ref()
    }

    /// The names typed rows are mapped with, if any.
    pub(crate) fn names(&self) -> Option<&StringRecord> {
        if self.named {
            self.names.as_ref()
        } else if self.config.positional() {
            None
        } else {
            self.config.supplied.as_ref()
        }
    }

    /// Data records parsed so far (the 1-based number of the last one).
    pub(crate) fn records(&self) -> u64 {
        self.records
    }

    /// Bytes of field buffer currently allocated (for tests of the allocation boundary).
    #[cfg(test)]
    pub(crate) fn allocated(&self) -> usize {
        self.fields.capacity()
    }

    /// Bytes allocated by the framing buffers (field bytes and field end offsets).
    pub(crate) fn retained(&self) -> usize {
        self.fields.capacity() + self.ends.capacity() * std::mem::size_of::<usize>()
    }

    /// The most bytes [`retained`](Self::retained) has been.
    pub(crate) fn peak_retained(&self) -> usize {
        self.peak
    }

    /// Follows the physical lines of `consumed` (see [`Line`]).
    fn track_lines(&mut self, consumed: &[u8]) {
        let Some(comment) = self.comment else {
            return;
        };
        for &b in consumed {
            self.line = match self.line {
                Line::Start if self.config.is_terminator(b) => Line::Start,
                Line::Start if b == comment => Line::Comment,
                Line::Start => Line::Record,
                Line::Comment if self.config.is_terminator(b) => Line::Start,
                other => other,
            };
        }
    }

    fn header_pending(&self) -> bool {
        self.config.has_header() && self.header.is_none()
    }

    /// Parses from `input` (an empty slice: the true end of input). Returns the number of input
    /// bytes consumed and what happened. Each call makes bounded progress: it consumes input,
    /// grows a buffer up to its limit, completes a record, or ends.
    pub(crate) fn step(&mut self, input: &[u8]) -> Result<(usize, Step), TableError> {
        if self.finished {
            return Ok((0, Step::End));
        }
        match self.bom {
            Bom::Detect { held } => {
                return self.detect_bom(held, input).map(|n| (n, Step::Progress));
            }
            Bom::Replay { at, len } => {
                // The first parser call gets at most one byte (see `parse`), so feed one at a time.
                let byte = [BOM[at]];
                let (n, step) = self.parse(&byte)?;
                self.bom = if at + n >= len {
                    Bom::Done
                } else {
                    Bom::Replay { at: at + n, len }
                };
                return Ok((0, step));
            }
            Bom::Done => {}
        }
        self.parse(input)
    }

    fn detect_bom(&mut self, mut held: usize, input: &[u8]) -> Result<usize, TableError> {
        if input.is_empty() {
            // The end of input inside a possible BOM: what was held is data.
            self.bom = Self::after_mismatch(held);
            return Ok(0);
        }
        let mut n = 0;
        while n < input.len() && held < BOM.len() && input[n] == BOM[held] {
            held += 1;
            n += 1;
        }
        if held == BOM.len() {
            match self.config.bom() {
                BomPolicy::Reject => {
                    return Err(TableError::format(CodecError::new(
                        "the CSV input starts with a byte order mark, which this profile refuses",
                    )));
                }
                _ => self.bom = Bom::Done, // stripped
            }
        } else if n < input.len() {
            self.bom = Self::after_mismatch(held);
        } else {
            self.bom = Bom::Detect { held };
        }
        Ok(n)
    }

    fn after_mismatch(held: usize) -> Bom {
        if held == 0 {
            Bom::Done
        } else {
            Bom::Replay { at: 0, len: held }
        }
    }

    /// One `csv-core` call over `input`, with the buffers bounded by the limits.
    fn parse(&mut self, input: &[u8]) -> Result<(usize, Step), TableError> {
        // `csv-core` strips a BOM itself on its first call if that input holds three or more
        // bytes; the BOM policy is ours, so the first call never gets more than one byte.
        let input = if self.started || input.is_empty() {
            input
        } else {
            &input[..1]
        };
        self.started = true;

        let header = self.header_pending();
        let byte_limit = if header {
            self.config.max_header_bytes()
        } else {
            self.config.max_record_bytes()
        };
        let field_limit = self.config.max_fields();

        if self.fields_len >= self.fields.len().min(byte_limit) && self.fields_len < byte_limit {
            let grown = (self.fields.len() * 2).max(MIN_FIELD_BYTES).min(byte_limit);
            // `reserve_exact`: an amortized `reserve` could allocate past the limit.
            self.fields.reserve_exact(grown - self.fields.len());
            self.fields.resize(grown, 0);
        }
        if self.ends_len >= self.ends.len().min(field_limit) && self.ends_len < field_limit {
            let grown = (self.ends.len() * 2).max(MIN_FIELDS).min(field_limit);
            self.ends.reserve_exact(grown - self.ends.len());
            self.ends.resize(grown, 0);
        }
        self.peak = self.peak.max(self.retained());
        // At a limit, the parser writes into a one-element probe instead: anything written there
        // is the byte or field end that exceeds the limit. The owned buffers never pass it.
        let mut byte_probe = [0u8; 1];
        let mut end_probe = [0usize; 1];
        let fields_end = self.fields.len().min(byte_limit);
        let at_byte_limit = self.fields_len >= fields_end;
        let ends_end = self.ends.len().min(field_limit);
        let at_field_limit = self.ends_len >= ends_end;
        let output: &mut [u8] = if at_byte_limit {
            &mut byte_probe
        } else {
            &mut self.fields[self.fields_len..fields_end]
        };
        let ends: &mut [usize] = if at_field_limit {
            &mut end_probe
        } else {
            &mut self.ends[self.ends_len..ends_end]
        };
        let (result, nin, nout, nend) = self.core.read_record(input, output, ends);
        if at_byte_limit && nout > 0 {
            let codec = if header {
                CodecError::new("the CSV header exceeds the configured size")
            } else {
                CodecError::new("a CSV record exceeds the configured size")
                    .at("record", self.records + 1)
            };
            return Err(TableError::limit(codec.at("limit", byte_limit as u64)));
        }
        if at_field_limit && nend > 0 {
            let codec = if header {
                CodecError::new("the CSV header has more fields than the configured limit")
            } else {
                CodecError::new("a CSV record has more fields than the configured limit")
                    .at("record", self.records + 1)
            };
            return Err(TableError::limit(codec.at("limit", field_limit as u64)));
        }
        self.fields_len += nout;
        self.ends_len += nend;
        let line = self.line;
        self.track_lines(&input[..nin]);
        match result {
            ReadRecordResult::InputEmpty
            | ReadRecordResult::OutputFull
            | ReadRecordResult::OutputEndsFull => Ok((nin, Step::Progress)),
            ReadRecordResult::Record if input.is_empty() && line == Line::Comment => {
                // The DFA's record for an unterminated final comment line: not a record.
                self.fields_len = 0;
                self.ends_len = 0;
                self.line = Line::Start;
                Ok((nin, Step::Progress))
            }
            ReadRecordResult::Record => {
                self.line = Line::Start;
                let record = self.take_record();
                self.complete(record).map(|step| (nin, step))
            }
            ReadRecordResult::End => {
                if self.header_pending() {
                    return Err(TableError::format(CodecError::new(
                        "the CSV input has no header record",
                    )));
                }
                self.finished = true;
                Ok((nin, Step::End))
            }
        }
    }

    /// The parsed record as an owned `ByteRecord`; the buffers are kept (at most their limits)
    /// for the next one.
    fn take_record(&mut self) -> ByteRecord {
        let ends = &self.ends[..self.ends_len];
        let mut record = ByteRecord::with_capacity(self.fields_len, ends.len());
        let mut start = 0;
        for &end in ends {
            record.push_field(&self.fields[start..end]);
            start = end;
        }
        self.fields_len = 0;
        self.ends_len = 0;
        record
    }

    fn complete(&mut self, mut record: ByteRecord) -> Result<Step, TableError> {
        if self.header_pending() {
            if self.config.trim_headers() {
                record.trim();
            }
            self.accept_header(record)?;
            return Ok(Step::Progress);
        }
        self.records += 1;
        if self.config.trim_fields() {
            record.trim();
        }
        if !self.config.flexible() {
            match self.width {
                Some(width) if width != record.len() => {
                    let label = if self.config.has_header() {
                        "a CSV record does not have as many fields as the header"
                    } else if self.config.width.is_some() {
                        "a CSV record does not have as many fields as the columns"
                    } else {
                        "a CSV record does not have as many fields as the first record"
                    };
                    return Err(TableError::format(
                        CodecError::new(label)
                            .at("record", self.records)
                            .at("fields", record.len() as u64)
                            .at("expected", width as u64),
                    ));
                }
                Some(_) => {}
                None => self.width = Some(record.len()),
            }
        }
        Ok(Step::Record(record))
    }

    fn accept_header(&mut self, record: ByteRecord) -> Result<(), TableError> {
        let field = |i: usize| i as u64 + 1;
        if let Some(columns) = self.config.columns() {
            let mismatch = (0..columns.len().max(record.len()))
                .find(|&i| record.get(i) != columns.get(i).map(String::as_bytes));
            if let Some(i) = mismatch {
                return Err(TableError::format(
                    CodecError::new("the CSV header does not match the expected columns")
                        .at("field", field(i)),
                ));
            }
        }
        if self.named {
            let names = StringRecord::from_byte_record(record.clone()).map_err(|e| {
                TableError::format(
                    CodecError::new("the CSV header is not valid UTF-8")
                        .at("field", field(e.utf8_error().field())),
                )
            })?;
            let mut seen = HashSet::with_capacity(names.len());
            for (i, name) in names.iter().enumerate() {
                if name.is_empty() {
                    return Err(TableError::format(
                        CodecError::new("the CSV header has an empty name").at("field", field(i)),
                    ));
                }
                if !seen.insert(name) {
                    return Err(TableError::format(
                        CodecError::new("the CSV header repeats a name").at("field", field(i)),
                    ));
                }
            }
            self.names = Some(names);
        }
        self.width = Some(record.len());
        self.header = Some(RawRecord::from_byte_record(record));
        Ok(())
    }
}

/// Runs a table over a complete body: the finite route. `map` turns each data record into a row.
pub(crate) fn decode_all<T>(
    config: Arc<Config>,
    mode: Mode,
    body: &[u8],
    map: super::record::MapFn<T>,
) -> Result<(Option<RawRecord>, Vec<T>), CodecError> {
    if let Some(max) = config.max_total_bytes()
        && body.len() as u64 > max
    {
        return Err(
            CodecError::new("the input exceeds the configured total bytes").at("limit", max),
        );
    }
    let max_records = config.max_records();
    let mut table = Table::new(config, mode);
    let mut rows = Vec::new();
    let mut input = body;
    loop {
        let (n, step) = table.step(input).map_err(|e| e.codec)?;
        input = &input[n..];
        match step {
            Step::Progress => {}
            Step::Record(record) => {
                if let Some(max) = max_records
                    && rows.len() as u64 >= max
                {
                    return Err(
                        CodecError::new("the input exceeds the configured total records")
                            .at("limit", max),
                    );
                }
                rows.push(map(&table, record).map_err(|e| e.codec)?);
            }
            Step::End => return Ok((table.header.take(), rows)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv::options::{CsvOptions, HeaderMode};

    fn table(options: CsvOptions) -> Table {
        Table::new(Arc::new(Config::new(options).unwrap()), Mode::Raw)
    }

    /// Feeds `input` whole and returns the records' field counts, or the error label.
    fn run(table: &mut Table, mut input: &[u8]) -> Result<Vec<usize>, &'static str> {
        let mut out = Vec::new();
        loop {
            let (n, step) = table.step(input).map_err(|e| e.codec.label())?;
            input = &input[n..];
            match step {
                Step::Progress => {}
                Step::Record(r) => out.push(r.len()),
                Step::End => return Ok(out),
            }
        }
    }

    #[test]
    fn the_field_buffer_never_grows_past_the_record_limit() {
        for limit in [1usize, 7, 255, 256, 257, 1000] {
            let options = CsvOptions::new(HeaderMode::Absent, b',').max_record_bytes(limit);
            // Exactly the limit: accepted, buffer at most the limit.
            let exact = format!("{}\n", "a".repeat(limit));
            let mut t = table(options.clone());
            assert_eq!(run(&mut t, exact.as_bytes()), Ok(vec![1]), "limit {limit}");
            assert!(t.allocated() <= limit, "limit {limit}: {}", t.allocated());
            // Split across two fields, quoted, still exactly the limit.
            let split = format!(
                "\"{}\",\"{}\"\r\n",
                "b".repeat(limit / 2),
                "c".repeat(limit - limit / 2)
            );
            let mut t = table(options.clone());
            assert_eq!(run(&mut t, split.as_bytes()), Ok(vec![2]), "limit {limit}");
            // One more byte: a limit error, and the buffer stayed at the limit.
            let over = format!("{}\n", "a".repeat(limit + 1));
            let mut t = table(options);
            assert_eq!(
                run(&mut t, over.as_bytes()),
                Err("a CSV record exceeds the configured size")
            );
            assert!(t.allocated() <= limit, "limit {limit}: {}", t.allocated());
        }
    }

    #[test]
    fn the_end_buffer_never_grows_past_the_field_limit() {
        for limit in [1usize, 2, 16, 17, 40] {
            let options = CsvOptions::new(HeaderMode::Absent, b',').max_fields(limit);
            let exact = vec!["x"; limit].join(",");
            for body in [format!("{exact}\n"), exact.clone()] {
                let mut t = table(options.clone());
                assert_eq!(
                    run(&mut t, body.as_bytes()),
                    Ok(vec![limit]),
                    "limit {limit}"
                );
                assert!(t.ends.capacity() <= limit);
            }
            let over = format!("{exact},x\n");
            let mut t = table(options);
            assert_eq!(
                run(&mut t, over.as_bytes()),
                Err("a CSV record has more fields than the configured limit")
            );
            assert!(t.ends.capacity() <= limit);
        }
    }

    #[test]
    fn the_header_has_its_own_limit() {
        let options = CsvOptions::new(HeaderMode::Present, b',')
            .max_header_bytes(4)
            .max_record_bytes(64);
        let mut t = table(options.clone());
        assert_eq!(run(&mut t, b"ab,cd\nlonger,values\n"), Ok(vec![2]));
        let mut t = table(options);
        assert_eq!(
            run(&mut t, b"abc,de\n1,2\n"),
            Err("the CSV header exceeds the configured size")
        );
    }

    #[test]
    fn an_unterminated_final_comment_line_is_not_a_record() {
        // `csv-core` 0.1.13's DFA yields `[""]` for the final `# trailer` (its NFA yields
        // nothing); a genuine final `""` is one empty field on both.
        let options = CsvOptions::new(HeaderMode::Absent, b',').comment(Some(b'#'));
        for (body, expected) in [
            (&b"a,b\n# trailer"[..], vec![2]),
            (b"a,b\n# trailer\n", vec![2]),
            (b"a,b\r\n#", vec![2]),
            (b"# only", vec![]),
            (b"a\n\"\"", vec![1, 1]),
            (b"a\n#c\n\"\"", vec![1, 1]),
            (b"a\n #c", vec![1, 1]),
        ] {
            for split in 0..=body.len() {
                let mut t = table(options.clone());
                let mut out = Vec::new();
                for part in [&body[..split], &body[split..]] {
                    let mut part = part;
                    while !part.is_empty() {
                        let (n, step) = t.step(part).unwrap();
                        part = &part[n..];
                        if let Step::Record(r) = step {
                            out.push(r.len());
                        }
                    }
                }
                out.extend(run(&mut t, b"").unwrap());
                assert_eq!(out, expected, "{body:?} split {split}");
            }
        }
    }

    #[test]
    fn a_long_comment_run_allocates_nothing() {
        let options = CsvOptions::new(HeaderMode::Absent, b',').comment(Some(b'#'));
        let mut body = Vec::new();
        for _ in 0..10_000 {
            body.extend_from_slice(b"# a comment line that is skipped by the parser\n\n");
        }
        body.extend_from_slice(b"a,b\n");
        let mut t = table(options);
        assert_eq!(run(&mut t, &body), Ok(vec![2]));
        assert!(t.allocated() <= MIN_FIELD_BYTES);
    }
}
