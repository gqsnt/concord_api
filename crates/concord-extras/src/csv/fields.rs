//! The writer's field encoding: `csv-core`'s quoting decision, quoting that this crate's reader
//! inverts byte for byte (escape bytes included), `csv`'s text for floats, and a record buffer
//! that never grows past the record (or header) limit.
//!
//! `csv`'s writer cannot be used for the bytes: inside a quoted field it escapes only the quote,
//! while `csv-core`'s reader treats the escape byte there as "the next byte is literal", so a
//! literal escape byte written by `csv` is dropped on reading (and the byte after it changes
//! meaning). Its buffer also grows with the record before any limit can be checked.

use std::cell::RefCell;
use std::fmt;
use std::io;

use serde::Serialize;

use super::options::Config;

/// The writer's dialect: the bytes of a field, a delimiter and a terminator.
pub(crate) struct Dialect {
    /// `csv-core`'s writer of the same settings: asked only whether a field needs quotes, so
    /// fields that need no escaping get exactly the bytes `csv` writes.
    oracle: csv_core::Writer,
    delimiter: u8,
    quote: u8,
    escape: Option<u8>,
    /// The byte written before a quote inside a quoted field: the quote (doubled quotes) or the
    /// escape byte (`double_quote(false)`, which configuration validation pairs with one).
    quote_prefix: u8,
    terminator: &'static [u8],
}

impl Dialect {
    pub(crate) fn new(config: &Config) -> Self {
        let quote = config.quote();
        let escape = config.escape();
        Self {
            oracle: config.core_writer(),
            delimiter: config.delimiter(),
            quote,
            escape,
            quote_prefix: if config.double_quote() {
                quote
            } else {
                escape.unwrap_or(b'\\')
            },
            terminator: config.write_terminator(),
        }
    }

    /// Whether `b` is written as two bytes inside a quoted field.
    fn is_escaped(&self, b: u8) -> bool {
        b == self.quote || Some(b) == self.escape
    }

    /// `value` between quotes: a quote as `quote_prefix` + quote, an escape byte as two escape
    /// bytes. `csv-core`'s reader, inside a quoted field, reads the escape byte as "take the next
    /// byte literally" and a doubled quote (with `double_quote`) as one quote, so it reads back
    /// exactly `value`.
    fn write_quoted(&self, value: &[u8], out: &mut Vec<u8>) {
        out.push(self.quote);
        let mut start = 0;
        for (i, &b) in value.iter().enumerate() {
            if self.is_escaped(b) {
                out.extend_from_slice(&value[start..i]);
                out.push(if b == self.quote {
                    self.quote_prefix
                } else {
                    b
                });
                out.push(b);
                start = i + 1;
            }
        }
        out.extend_from_slice(&value[start..]);
        out.push(self.quote);
    }
}

/// The record would be larger than its limit. Nothing of the refused field was written.
#[derive(Debug)]
pub(crate) struct Exceeded;

/// One record being encoded into a buffer owned by the encoder.
///
/// The encoded record without its terminator (quotes, escapes and delimiters included) is at
/// most `limit` bytes: a field is measured before it is copied, so the buffer never holds more
/// than `limit` bytes plus the terminator, and its capacity grows by doubling up to that size and
/// no further.
pub(crate) struct Fields<'a> {
    dialect: &'a Dialect,
    floats: &'a mut FloatText,
    buf: &'a mut Vec<u8>,
    limit: usize,
    count: usize,
}

impl<'a> Fields<'a> {
    /// An empty record in `buf` (cleared; its capacity is kept).
    pub(crate) fn new(
        dialect: &'a Dialect,
        floats: &'a mut FloatText,
        buf: &'a mut Vec<u8>,
        limit: usize,
    ) -> Self {
        buf.clear();
        Self {
            dialect,
            floats,
            buf,
            limit,
            count: 0,
        }
    }

    /// The next field, quoted as `csv` would quote it.
    pub(crate) fn field(&mut self, value: &[u8]) -> Result<(), Exceeded> {
        let d = self.dialect;
        let delimiter = usize::from(self.count > 0);
        let room = self.limit - self.buf.len();
        // An encoded field is never smaller than its value: refuse a large value unscanned.
        if value.len().saturating_add(delimiter) > room {
            return Err(Exceeded);
        }
        let quoted = d.oracle.should_quote(value);
        let size = if quoted {
            let escaped = value.iter().filter(|&&b| d.is_escaped(b)).count();
            value.len() + escaped + 2
        } else {
            value.len()
        } + delimiter;
        if size > room {
            return Err(Exceeded);
        }
        self.reserve(size);
        if delimiter == 1 {
            self.buf.push(d.delimiter);
        }
        if quoted {
            d.write_quoted(value, self.buf);
        } else {
            self.buf.extend_from_slice(value);
        }
        self.count += 1;
        Ok(())
    }

    /// The next field: a float as `csv` writes it.
    pub(crate) fn float<F: Serialize>(&mut self, value: F) -> Result<(), FieldError> {
        let mut text = Short::default();
        self.floats
            .write(value, &mut text)
            .ok_or(FieldError::Text)?;
        self.field(text.as_bytes())
            .map_err(|Exceeded| FieldError::Exceeded)
    }

    /// The next field: an integer, whose `Display` text is the `itoa` text `csv` writes.
    pub(crate) fn integer(&mut self, value: impl fmt::Display) -> Result<(), FieldError> {
        let mut text = Short::default();
        fmt::Write::write_fmt(&mut text, format_args!("{value}")).map_err(|_| FieldError::Text)?;
        self.field(text.as_bytes())
            .map_err(|Exceeded| FieldError::Exceeded)
    }

    /// Ends the record: two quotes for a record of nothing (one empty field, as `csv-core` writes
    /// it, so that it is not a blank line), then the terminator.
    pub(crate) fn end(self) -> Result<(), Exceeded> {
        let Self {
            dialect,
            buf,
            limit,
            ..
        } = self;
        if buf.is_empty() {
            if limit < 2 {
                return Err(Exceeded);
            }
            reserve(buf, 2, limit, dialect.terminator.len());
            buf.extend_from_slice(&[dialect.quote, dialect.quote]);
        }
        reserve(
            buf,
            dialect.terminator.len(),
            limit,
            dialect.terminator.len(),
        );
        buf.extend_from_slice(dialect.terminator);
        Ok(())
    }

    fn reserve(&mut self, additional: usize) {
        reserve(
            self.buf,
            additional,
            self.limit,
            self.dialect.terminator.len(),
        );
    }
}

/// Grows `buf` for `additional` more bytes: by doubling, but never past `limit + terminator`.
fn reserve(buf: &mut Vec<u8>, additional: usize, limit: usize, terminator: usize) {
    let needed = buf.len() + additional;
    if needed <= buf.capacity() {
        return;
    }
    let ceiling = limit.saturating_add(terminator);
    let target = buf
        .capacity()
        .saturating_mul(2)
        .max(64)
        .min(ceiling)
        .max(needed);
    buf.reserve_exact(target - buf.len());
}

/// Why a formatted field was not written.
#[derive(Debug)]
pub(crate) enum FieldError {
    /// Over the record limit.
    Exceeded,
    /// The value has no text (not expected for numbers; reported rather than written wrong).
    Text,
}

/// A number's text, on the stack: an integer's (`i128::MIN` is 40 bytes) or a float's (at most
/// 24 bytes).
struct Short {
    buf: [u8; 40],
    len: usize,
}

impl Default for Short {
    fn default() -> Self {
        Self {
            buf: [0; 40],
            len: 0,
        }
    }
}

impl Short {
    fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    fn push(&mut self, bytes: &[u8]) -> Option<()> {
        let end = self.len + bytes.len();
        self.buf.get_mut(self.len..end)?.copy_from_slice(bytes);
        self.len = end;
        Some(())
    }
}

impl fmt::Write for Short {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push(s.as_bytes()).ok_or(fmt::Error)
    }
}

/// `csv`'s text for floats: `csv` formats them with `ryu`, which this crate does not depend on,
/// so a `csv` writer formats them here, as a one-field record that is never quoted. The text is
/// the value's shortest round-trip form (`f32` and `f64` differ), at most 24 bytes.
pub(crate) struct FloatText {
    writer: ::csv::Writer<Captured>,
}

/// The float writer's output, readable through `csv::Writer::get_ref`.
struct Captured(RefCell<Vec<u8>>);

impl io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.get_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl FloatText {
    pub(crate) fn new() -> Self {
        let writer = ::csv::WriterBuilder::new()
            .has_headers(false)
            .flexible(true)
            .quote_style(::csv::QuoteStyle::Never)
            .terminator(::csv::Terminator::Any(b'\n'))
            .buffer_capacity(64)
            .from_writer(Captured(RefCell::new(Vec::with_capacity(64))));
        Self { writer }
    }

    /// Writes the text of `value` (an `f32` or `f64`) to `text`; `None` if `csv` could not
    /// write it.
    fn write<F: Serialize>(&mut self, value: F, text: &mut Short) -> Option<()> {
        let written = self.writer.serialize(value).is_ok() && self.writer.flush().is_ok();
        let mut captured = self.writer.get_ref().0.borrow_mut();
        let result = if written {
            text.push(captured.strip_suffix(b"\n").unwrap_or(&captured))
        } else {
            None
        };
        captured.clear();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv::{CsvOptions, HeaderMode};

    fn dialect(options: CsvOptions) -> Dialect {
        Dialect::new(&Config::new(options).unwrap())
    }

    #[test]
    fn an_oversized_field_is_refused_before_anything_is_allocated() {
        let d = dialect(CsvOptions::new(HeaderMode::Absent, b','));
        let mut floats = FloatText::new();
        let mut buf = Vec::new();
        {
            let mut fields = Fields::new(&d, &mut floats, &mut buf, 64);
            assert!(fields.field(&vec![b'x'; 1 << 20]).is_err());
            // Escaping makes a field that fits unescaped too large: refused after measuring.
            assert!(fields.field(&[b'"'; 40]).is_err());
        }
        assert_eq!(buf.capacity(), 0);
    }

    #[test]
    fn quoted_fields_escape_the_quote_and_the_escape_byte() {
        let cases = [
            (
                CsvOptions::new(HeaderMode::Absent, b','),
                &br#""a""b\c""#[..],
            ),
            (
                CsvOptions::new(HeaderMode::Absent, b',').escape(Some(b'\\')),
                br#""a""b\\c""#,
            ),
            (
                CsvOptions::new(HeaderMode::Absent, b',')
                    .escape(Some(b'\\'))
                    .double_quote(false),
                br#""a\"b\\c""#,
            ),
        ];
        for (options, expected) in cases {
            let d = dialect(options);
            let mut out = Vec::new();
            d.write_quoted(br#"a"b\c"#, &mut out);
            assert_eq!(out, expected, "{}", String::from_utf8_lossy(&out));
        }
    }

    #[test]
    fn numbers_have_the_csv_text() {
        let d = dialect(CsvOptions::new(HeaderMode::Absent, b','));
        let mut floats = FloatText::new();
        let mut buf = Vec::new();
        let mut fields = Fields::new(&d, &mut floats, &mut buf, 1024);
        fields.integer(i128::MIN).unwrap();
        fields.integer(u128::MAX).unwrap();
        fields.float(0.1f32).unwrap();
        fields.float(0.1f64).unwrap();
        fields.float(1e20f64).unwrap();
        fields.float(f64::NAN).unwrap();
        fields.float(f64::MIN_POSITIVE).unwrap();
        fields.end().unwrap();
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "-170141183460469231731687303715884105728,340282366920938463463374607431768211455,\
             0.1,0.1,1e20,NaN,2.2250738585072014e-308\r\n"
        );
    }
}
