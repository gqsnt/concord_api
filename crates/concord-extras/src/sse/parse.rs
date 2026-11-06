//! The event stream interpretation (WHATWG HTML, "Interpreting an event stream") over arbitrary
//! chunk boundaries, with Concord's stricter end-of-input, encoding and size rules.

use std::time::Duration;

use concord_core::CodecError;

use super::reader::Event;
use crate::stream::ReadErrorKind;

const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

pub(super) const NOT_UTF8: &str = "a line is not valid UTF-8";
pub(super) const ENDS_IN_LINE: &str = "the input ends inside a line";
pub(super) const ENDS_IN_EVENT: &str =
    "the input ends inside an event, before the blank line that dispatches it";
pub(super) const TOO_LARGE: &str = "an event exceeds max_event_bytes";

/// A failure found while interpreting the input: terminal.
pub(super) struct Fault {
    pub(super) kind: ReadErrorKind,
    pub(super) codec: CodecError,
}

/// One interpretation step.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Step {
    /// A blank line dispatched this event.
    Event(Event),
    /// The input is used up; more is needed.
    NeedMore,
    /// The true end of input, at an event boundary.
    End,
}

/// The state of one event stream. It retains the unfinished line, the data buffer and the event
/// type of the event being assembled — together at most `max` bytes, checked before a buffer
/// grows — and the last event ID.
pub(super) struct Parser {
    /// The unfinished line, without its terminator.
    line: Vec<u8>,
    /// The data buffer: every `data` value of the event being assembled, each followed by LF.
    data: String,
    /// The event type buffer.
    event_type: String,
    /// The last event ID buffer (`id` fields; not reset by a dispatch).
    id_buffer: String,
    /// The last event ID string: the buffer as of the last blank line.
    last_id: String,
    /// The last valid `retry` value.
    retry: Option<Duration>,
    /// Leading bytes matched against the byte order mark so far.
    bom_seen: usize,
    bom_done: bool,
    /// The last line ended with CR: an LF that follows belongs to that line end.
    skip_lf: bool,
    /// A field line arrived since the last blank line.
    in_event: bool,
    /// Complete lines so far.
    lines: u64,
    max: usize,
}

impl Parser {
    pub(super) fn new(max: usize) -> Self {
        Self {
            line: Vec::new(),
            data: String::new(),
            event_type: String::new(),
            id_buffer: String::new(),
            last_id: String::new(),
            retry: None,
            bom_seen: 0,
            bom_done: false,
            skip_lf: false,
            in_event: false,
            lines: 0,
            max,
        }
    }

    /// The last event ID string (as of the last blank line).
    pub(super) fn last_event_id(&self) -> &str {
        &self.last_id
    }

    /// The last valid `retry` value.
    pub(super) fn retry(&self) -> Option<Duration> {
        self.retry
    }

    /// Complete lines interpreted so far.
    pub(super) fn lines(&self) -> u64 {
        self.lines
    }

    /// The bytes of the event being assembled: data buffer, event type and unfinished line.
    fn assembled(&self) -> usize {
        self.data.len() + self.event_type.len() + self.line.len()
    }

    /// Interprets `input` (the unconsumed part of the current chunk, or a slice of it) until one
    /// event is dispatched or `input` is used up. `eof`: the source has ended and `input` is the
    /// last of it. Returns how many bytes of `input` were consumed. A fault is terminal.
    ///
    /// The search for a line end covers at most what the bound still allows: a longer run
    /// without one is a limit fault however much input follows.
    pub(super) fn feed(&mut self, input: &[u8], eof: bool) -> Result<(usize, Step), Fault> {
        let mut used = 0;
        if !self.bom_done {
            // A mark split across chunks is still recognized; a partial one is content.
            while self.bom_seen < BOM.len() {
                match input.get(used) {
                    Some(&b) if b == BOM[self.bom_seen] => {
                        self.bom_seen += 1;
                        used += 1;
                    }
                    Some(_) => break,
                    None if eof => break,
                    None => return Ok((used, Step::NeedMore)),
                }
            }
            self.bom_done = true;
            if self.bom_seen < BOM.len() {
                self.grow(&BOM[..self.bom_seen])?;
            }
        }
        while used < input.len() {
            if self.skip_lf {
                self.skip_lf = false;
                if input[used] == b'\n' {
                    used += 1;
                    continue;
                }
            }
            let rest = &input[used..];
            let allowance = self.max - self.assembled();
            let reach = rest.len().min(allowance.saturating_add(1));
            match rest[..reach].iter().position(|&b| b == b'\n' || b == b'\r') {
                Some(end) => {
                    self.skip_lf = rest[end] == b'\r';
                    used += end + 1;
                    self.lines += 1;
                    let event = if self.line.is_empty() {
                        self.interpret(&rest[..end])?
                    } else {
                        self.grow(&rest[..end])?;
                        let line = std::mem::take(&mut self.line);
                        let event = self.interpret(&line);
                        // Keep the allocation for the next line.
                        self.line = line;
                        self.line.clear();
                        event?
                    };
                    if let Some(event) = event {
                        return Ok((used, Step::Event(event)));
                    }
                }
                None => {
                    self.grow(rest)?;
                    used = input.len();
                }
            }
        }
        if !eof {
            return Ok((used, Step::NeedMore));
        }
        if !self.line.is_empty() {
            return Err(self.fault(ReadErrorKind::Format, ENDS_IN_LINE, self.lines + 1));
        }
        if self.in_event {
            return Err(self.fault(ReadErrorKind::Format, ENDS_IN_EVENT, self.lines));
        }
        Ok((used, Step::End))
    }

    /// Appends to the unfinished line, refusing (before any growth) a line that would take the
    /// event being assembled past `max`. The buffer's capacity never exceeds `max`.
    fn grow(&mut self, bytes: &[u8]) -> Result<(), Fault> {
        if bytes.len() > self.max - self.assembled() {
            let max = self.max as u64;
            return Err(Fault {
                kind: ReadErrorKind::Limit,
                codec: CodecError::new(TOO_LARGE)
                    .at("line", self.lines + 1)
                    .at("limit", max),
            });
        }
        let needed = self.line.len() + bytes.len();
        if needed > self.line.capacity() {
            let target = (self.line.capacity() * 2).max(needed).min(self.max);
            self.line.reserve_exact(target - self.line.len());
        }
        self.line.extend_from_slice(bytes);
        Ok(())
    }

    /// Interprets one complete line (its terminator removed).
    fn interpret(&mut self, line: &[u8]) -> Result<Option<Event>, Fault> {
        let line = match std::str::from_utf8(line) {
            Ok(line) => line,
            Err(e) => {
                let column = e.valid_up_to() as u64 + 1;
                let fault = self.fault(ReadErrorKind::Format, NOT_UTF8, self.lines);
                return Err(Fault {
                    codec: fault.codec.at("column", column),
                    ..fault
                });
            }
        };
        if line.is_empty() {
            return Ok(self.dispatch());
        }
        if line.starts_with(':') {
            return Ok(None); // a comment
        }
        self.in_event = true;
        let (name, value) = match line.split_once(':') {
            Some((name, value)) => (name, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match name {
            // Neither buffer passes `max`: the line also held the field name.
            "event" => {
                self.event_type.clear();
                reserve_bounded(&mut self.event_type, value.len(), self.max);
                self.event_type.push_str(value);
            }
            "data" => {
                reserve_bounded(&mut self.data, value.len() + 1, self.max);
                self.data.push_str(value);
                self.data.push('\n');
            }
            "id" if !value.contains('\0') => value.clone_into(&mut self.id_buffer),
            "retry" if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => {
                // Base ten; a value beyond `u64` milliseconds saturates.
                let ms = value.bytes().fold(0u64, |ms, digit| {
                    ms.saturating_mul(10)
                        .saturating_add(u64::from(digit - b'0'))
                });
                self.retry = Some(Duration::from_millis(ms));
            }
            _ => {} // an unknown field, an `id` containing NUL, an invalid `retry`
        }
        Ok(None)
    }

    /// A blank line: the last event ID string takes the buffer's value; the event is dispatched
    /// unless its data buffer is empty; the data and event type buffers are reset either way.
    fn dispatch(&mut self) -> Option<Event> {
        self.in_event = false;
        if self.last_id != self.id_buffer {
            self.last_id.clone_from(&self.id_buffer);
        }
        if self.data.is_empty() {
            self.event_type.clear();
            return None;
        }
        self.data.pop(); // the LF after the last value
        let event = if self.event_type.is_empty() {
            "message".to_owned()
        } else {
            std::mem::take(&mut self.event_type)
        };
        Some(Event::new(
            event,
            std::mem::take(&mut self.data),
            self.last_id.clone(),
        ))
    }

    fn fault(&self, kind: ReadErrorKind, label: &'static str, line: u64) -> Fault {
        Fault {
            kind,
            codec: CodecError::new(label).at("line", line),
        }
    }
}

/// Reserves room for `more` bytes without letting the capacity exceed `bound` when the content
/// stays within it.
fn reserve_bounded(buffer: &mut String, more: usize, bound: usize) {
    let needed = buffer.len() + more;
    if needed > buffer.capacity() {
        let target = (buffer.capacity() * 2).max(needed).min(bound.max(needed));
        buffer.reserve_exact(target - buffer.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(type, data, id)` of an event.
    type Seen = (String, String, String);

    /// Feeds `chunks` (then the end of input) and returns every event seen, or the first fault's
    /// label and line.
    fn events(max: usize, chunks: &[&[u8]]) -> Result<Vec<Seen>, (&'static str, Option<u64>)> {
        let mut parser = Parser::new(max);
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
            let (used, step) = parser
                .feed(input, eof)
                .map_err(|f| (f.codec.label(), f.codec.get("line")))?;
            input = &input[used..];
            match step {
                Step::Event(e) => out.push((e.event().into(), e.data().into(), e.id().into())),
                Step::NeedMore => assert!(input.is_empty(), "NeedMore leaves no input"),
                Step::End => return Ok(out),
            }
        }
    }

    fn ev(event: &str, data: &str, id: &str) -> Seen {
        (event.into(), data.into(), id.into())
    }

    #[test]
    fn crlf_split_is_one_line_end() {
        let one = events(1024, &[b"data: a\r\n\r\n"]).unwrap();
        let split = events(1024, &[b"data: a\r", b"\n\r", b"\n"]).unwrap();
        assert_eq!(one, vec![ev("message", "a", "")]);
        assert_eq!(split, one);
        // A CR then a non-LF byte: two line ends, CR alone counts.
        assert_eq!(
            events(1024, &[b"data: a\r", b"\rdata: b\r\r"]).unwrap(),
            vec![ev("message", "a", ""), ev("message", "b", "")]
        );
    }

    #[test]
    fn bom_once_and_split() {
        for chunks in [
            &[&b"\xEF\xBB\xBFdata: x\n\n"[..]][..],
            &[b"\xEF", b"\xBB", b"\xBF", b"data: x\n\n"],
            &[b"\xEF\xBB", b"\xBFdata: x\n\n"],
        ] {
            assert_eq!(
                events(1024, chunks).unwrap(),
                vec![ev("message", "x", "")],
                "{chunks:?}"
            );
        }
        // A second mark is content: a field named "\u{FEFF}data", unknown.
        assert_eq!(
            events(1024, &[b"\xEF\xBB\xBF\xEF\xBB\xBFdata: x\n\n"]).unwrap(),
            vec![]
        );
        // A partial mark is content: invalid UTF-8, or an unfinished line at the end.
        assert_eq!(
            events(1024, &[b"\xEF\xBB", b"x\n\n"]),
            Err((NOT_UTF8, Some(1)))
        );
        assert_eq!(events(1024, &[b"\xEF\xBB"]), Err((ENDS_IN_LINE, Some(1))));
        assert_eq!(events(1024, &[b"\xEF\xBB\xBF"]).unwrap(), vec![]);
    }

    /// The bound counts the data buffer, the event type and the unfinished line; it is checked
    /// before growth, and the buffers' capacity stays within it.
    #[test]
    fn bound_before_growth() {
        // `data: 12345` is 11 bytes.
        assert_eq!(
            events(11, &[b"data: 12345\n\n"]).unwrap(),
            vec![ev("message", "12345", "")]
        );
        assert_eq!(events(10, &[b"data: 12345\n\n"]), Err((TOO_LARGE, Some(1))));
        // Data buffer "1234\n" (5) + `data: 56` (8) = 13.
        assert_eq!(
            events(13, &[b"data: 1234\ndata: 56\n\n"]).unwrap(),
            vec![ev("message", "1234\n56", "")]
        );
        assert_eq!(
            events(12, &[b"data: 1234\ndata: 56\n\n"]),
            Err((TOO_LARGE, Some(2)))
        );
        // The event type counts too; a dispatch frees the budget.
        assert_eq!(
            events(
                13,
                &[b"event: abcde\ndata: x\n\n", b"event: abcde\ndata: y\n\n"]
            )
            .unwrap(),
            vec![ev("abcde", "x", ""), ev("abcde", "y", "")]
        );
        assert_eq!(
            events(12, &[b"event: abcde\ndata: xy\n\n"]),
            Err((TOO_LARGE, Some(2)))
        );
        let big = [b"data: ".to_vec(), vec![b'x'; 1000], b"\n\n".to_vec()].concat();
        for size in [1, 7, 64, 1006] {
            let mut parser = Parser::new(1006);
            for chunk in big.chunks(size) {
                let mut chunk = chunk;
                while !chunk.is_empty() {
                    let (used, _) = parser.feed(chunk, false).map_err(|_| ()).unwrap();
                    chunk = &chunk[used..];
                    assert!(parser.assembled() <= 1006);
                    assert!(parser.line.capacity() <= 1006, "{}", parser.line.capacity());
                    assert!(parser.data.capacity() <= 1006, "{}", parser.data.capacity());
                }
            }
        }
    }

    /// A run without a line end is refused after inspecting at most what the bound allows.
    #[test]
    fn search_stops_within_reach_of_the_bound() {
        let large = vec![b'x'; 1 << 20];
        let mut parser = Parser::new(64);
        assert!(matches!(parser.feed(&large, false), Err(f) if f.codec.label() == TOO_LARGE));
        assert!(parser.line.capacity() <= 64);
    }

    #[test]
    fn retry_saturates_and_requires_digits() {
        let mut parser = Parser::new(1024);
        let input = b"retry: 99999999999999999999999\n\n";
        parser.feed(input, false).map_err(|_| ()).unwrap();
        assert_eq!(parser.retry(), Some(Duration::from_millis(u64::MAX)));
        let mut parser = Parser::new(1024);
        parser
            .feed(b"retry: 10\nretry:\nretry: 1x\nretry:  5\n\n", false)
            .map_err(|_| ())
            .unwrap();
        assert_eq!(parser.retry(), Some(Duration::from_millis(10)));
    }
}
