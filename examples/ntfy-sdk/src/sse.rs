//! Server-Sent Events framing (WHATWG HTML, "Interpreting an event stream"), as far as an ntfy
//! subscription needs it, over arbitrary chunk boundaries.
//!
//! - Lines end with LF, CRLF or a lone CR; a CR at the end of one chunk and an LF at the start of
//!   the next are one line end.
//! - A UTF-8 byte order mark at the very start is skipped.
//! - `:` starts a comment; `field: value` (one leading space removed); a line without `:` is a
//!   field with an empty value.
//! - `data` appends its value and an LF to the data buffer; `event` sets the event type; `id`,
//!   `retry` and unknown fields are ignored (ntfy sends none of them: its replay cursor is the
//!   JSON `id` sent back as `since=`, and reconnection is the caller's policy).
//! - An empty line dispatches the event when the data buffer is not empty (its last LF removed),
//!   then resets the buffers.
//!
//! Two deliberate departures from a browser, because a lost event matters to a subscriber: input
//! that ends inside an event (an unfinished line, or fields without their empty line) is an error
//! instead of a silent discard, and an event larger than the bound is an error instead of
//! unbounded growth. The framer works on bytes; UTF-8 is checked when the `data` payload is parsed
//! as JSON.

use concord::stream::ReadErrorKind;

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// A framing failure: the kind, a static label and the 1-based line it happened on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fault {
    pub(crate) kind: ReadErrorKind,
    pub(crate) label: &'static str,
    pub(crate) line: u64,
}

/// One dispatched event: its `data` (LF-joined lines, last LF removed) and the line that ended it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Dispatched {
    pub(crate) data: Vec<u8>,
    pub(crate) line: u64,
}

/// The framing state of one stream. It retains the unfinished line and the current event's data,
/// together at most `max` bytes.
pub(crate) struct Framer {
    line: Vec<u8>,
    data: Vec<u8>,
    /// The previous chunk ended with a CR: an LF that starts the next one belongs to it.
    pending_cr: bool,
    /// No line has been completed yet (the byte order mark may still be in `line`).
    first_line: bool,
    /// An `event` field or a `data` field was seen since the last dispatch.
    in_event: bool,
    lines: u64,
    max: usize,
}

impl Framer {
    pub(crate) fn new(max: usize) -> Self {
        Self {
            line: Vec::new(),
            data: Vec::new(),
            pending_cr: false,
            first_line: true,
            in_event: false,
            lines: 0,
            max,
        }
    }

    /// Frames `input` until one event is dispatched or `input` is used up. Returns how many bytes
    /// were consumed and the event, if one was dispatched. A fault is terminal.
    pub(crate) fn feed(&mut self, input: &[u8]) -> Result<(usize, Option<Dispatched>), Fault> {
        let mut used = 0;
        while used < input.len() {
            if self.pending_cr {
                self.pending_cr = false;
                if input[used] == b'\n' {
                    used += 1;
                    continue;
                }
            }
            let rest = &input[used..];
            match rest.iter().position(|&b| b == b'\n' || b == b'\r') {
                None => {
                    self.push(rest)?;
                    used = input.len();
                }
                Some(end) => {
                    self.push(&rest[..end])?;
                    self.pending_cr = rest[end] == b'\r';
                    used += end + 1;
                    if let Some(event) = self.line_complete()? {
                        return Ok((used, Some(event)));
                    }
                }
            }
        }
        Ok((used, None))
    }

    /// The true end of input: refused when it falls inside a line or an event.
    pub(crate) fn finish(&self) -> Result<(), Fault> {
        let inside_line =
            !self.line.is_empty() && !(self.first_line && BOM.starts_with(&self.line));
        if inside_line || self.in_event {
            return Err(self.fault(
                ReadErrorKind::Format,
                "the event stream ended inside an event",
            ));
        }
        Ok(())
    }

    fn push(&mut self, bytes: &[u8]) -> Result<(), Fault> {
        if self.line.len() + self.data.len() + bytes.len() > self.max {
            return Err(self.fault(
                ReadErrorKind::Limit,
                "an event is larger than the reader's bound",
            ));
        }
        self.line.extend_from_slice(bytes);
        Ok(())
    }

    fn fault(&self, kind: ReadErrorKind, label: &'static str) -> Fault {
        Fault {
            kind,
            label,
            line: self.lines + 1,
        }
    }

    fn line_complete(&mut self) -> Result<Option<Dispatched>, Fault> {
        self.lines += 1;
        let mut start = 0;
        if self.first_line {
            self.first_line = false;
            if self.line.starts_with(BOM) {
                start = BOM.len();
            }
        }
        let line = std::mem::take(&mut self.line);
        let result = self.interpret(&line[start..]);
        // Keep the allocation for the next line.
        self.line = line;
        self.line.clear();
        result
    }

    fn interpret(&mut self, line: &[u8]) -> Result<Option<Dispatched>, Fault> {
        if line.is_empty() {
            self.in_event = false;
            if self.data.is_empty() {
                return Ok(None);
            }
            let mut data = std::mem::take(&mut self.data);
            data.pop(); // the LF after the last data line
            return Ok(Some(Dispatched {
                data,
                line: self.lines,
            }));
        }
        if line[0] == b':' {
            return Ok(None); // a comment
        }
        let (name, value) = match line.iter().position(|&b| b == b':') {
            Some(colon) => {
                let value = &line[colon + 1..];
                (&line[..colon], value.strip_prefix(b" ").unwrap_or(value))
            }
            None => (line, &b""[..]),
        };
        match name {
            b"data" => {
                if self.data.len() + value.len() + 1 > self.max {
                    return Err(self.fault_here(
                        ReadErrorKind::Limit,
                        "an event is larger than the reader's bound",
                    ));
                }
                self.data.extend_from_slice(value);
                self.data.push(b'\n');
                self.in_event = true;
            }
            // The JSON payload's own `event` member is authoritative (ntfy omits the SSE field
            // for `message` and other kinds); the field only marks that an event has started.
            b"event" => self.in_event = true,
            _ => {}
        }
        Ok(None)
    }

    /// A fault on the line just completed.
    fn fault_here(&self, kind: ReadErrorKind, label: &'static str) -> Fault {
        Fault {
            kind,
            label,
            line: self.lines,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds `chunks` and returns every dispatched payload, or the first fault, then `finish`.
    fn frame(chunks: &[&[u8]], max: usize) -> Result<Vec<Vec<u8>>, Fault> {
        let mut framer = Framer::new(max);
        let mut out = Vec::new();
        for chunk in chunks {
            let mut rest = *chunk;
            while !rest.is_empty() {
                let (used, event) = framer.feed(rest)?;
                rest = &rest[used..];
                if let Some(event) = event {
                    out.push(event.data);
                }
            }
        }
        framer.finish()?;
        Ok(out)
    }

    /// Every way of cutting `input` in two gives the same events as the whole input.
    fn same_at_every_split(input: &[u8]) -> Vec<Vec<u8>> {
        let whole = frame(&[input], 1 << 20).expect("frames");
        for cut in 0..=input.len() {
            let (a, b) = input.split_at(cut);
            assert_eq!(
                frame(&[a, b], 1 << 20).expect("frames"),
                whole,
                "cut at {cut}"
            );
        }
        let bytes: Vec<&[u8]> = input.chunks(1).collect();
        assert_eq!(
            frame(&bytes, 1 << 20).expect("frames"),
            whole,
            "one byte per chunk"
        );
        whole
    }

    #[test]
    fn line_ends_bom_comments_and_fields() {
        let input = b"\xEF\xBB\xBFevent: open\r\ndata: {\"a\":1}\r\n\r\n: comment\rdata:x\ndata\nid: 7\nretry: 10\nunknown: y\n\nevent: keepalive\n\n";
        let events = same_at_every_split(input);
        assert_eq!(events, vec![b"{\"a\":1}".to_vec(), b"x\n".to_vec()]);
    }

    #[test]
    fn empty_data_is_an_event_and_fieldless_blank_lines_are_not() {
        assert_eq!(
            same_at_every_split(b"\n\n\ndata:\n\n"),
            vec![Vec::<u8>::new()]
        );
    }

    #[test]
    fn end_inside_an_event_is_an_error() {
        for input in [
            &b"data: {}"[..],
            b"data: {}\n",
            b"event: open\n",
            b"data: {}\r",
        ] {
            // `data: {}\r` ends a line with a lone CR, but not the event.
            let fault = frame(&[input], 1 << 20).unwrap_err();
            assert_eq!(fault.kind, ReadErrorKind::Format, "{input:?}");
        }
        assert_eq!(
            frame(&[b"\xEF\xBB"], 1 << 20).unwrap(),
            Vec::<Vec<u8>>::new()
        );
        assert_eq!(frame(&[b""], 1 << 20).unwrap(), Vec::<Vec<u8>>::new());
    }

    #[test]
    fn the_bound_counts_the_line_and_the_data() {
        // The raw line (`data: 12345`, 11 bytes) and the retained data count together.
        assert_eq!(
            frame(&[b"data: 12345\n\n"], 11).unwrap(),
            vec![b"12345".to_vec()]
        );
        assert_eq!(
            frame(&[b"data: 12345\n\n"], 10).unwrap_err().kind,
            ReadErrorKind::Limit
        );
        let fault = frame(&[b"data: 1234\ndata: 5\n\n"], 11).unwrap_err();
        assert_eq!((fault.kind, fault.line), (ReadErrorKind::Limit, 2));
        let fault = frame(&[b"data: 1234567"], 8).unwrap_err();
        assert_eq!(fault.kind, ReadErrorKind::Limit);
    }
}
