//! The late-exception guard: a byte source between the response body and the row reader that
//! finds the exception block ClickHouse writes into a body it has already started.
//!
//! When an exception happens after the status and headers were sent, the server writes (with
//! `http_write_exception_in_output_format=0`, the default):
//!
//! ```text
//! \r\n__exception__\r\n<TAG>\r\n<error message>\n<message_length> <TAG>\r\n__exception__\r\n
//! ```
//!
//! where `<TAG>` is the 16-byte value of the `X-ClickHouse-Exception-Tag` response header and
//! `<message_length>` counts the message with its final line feed. (The HTTP interface page
//! writes the message line as ending in `\r\n`; the server source the SDK checked writes `\n`
//! and counts it. The parser accepts both: it locates the trailer by the tag, not by the line
//! ending.) The server then breaks the HTTP framing (no last chunk), which the transport reports
//! as an error.
//!
//! The guard forwards every byte before the opening marker to the reader, holds back at most
//! `marker length - 1` bytes that could be the start of a marker split across chunks, and turns
//! the block into a source failure carrying a [`BodyException`]. Without the tag header there
//! is no marker to look for: the guard passes the body through unchanged (see the crate
//! documentation for what that means).
//!
//! Cancel safety: every chunk taken from the inner source is stored in `self` before the next
//! await, so a dropped `next_chunk` future loses nothing (the inner `reqwest::Response` is
//! cancel-safe itself).

use std::error::Error as StdError;
use std::fmt;

use concord::bytes::Bytes;
use concord::stream::{ByteSource, SourceError};

/// The whole exception block is at most 16 KiB (the documented and source bound); the guard
/// stops collecting a block after this many bytes and reports it incomplete.
pub(crate) const MAX_BLOCK_BYTES: usize = 16 * 1024;

const MARKER: &[u8] = b"__exception__";

/// The exception block found in a started body.
#[derive(Clone, PartialEq, Eq)]
pub struct BodyException {
    /// The message bytes, decoded lossily, trailing CR/LF removed.
    pub(crate) message: String,
    /// The `<message_length>` the trailer declared.
    pub(crate) declared_length: Option<usize>,
    /// Whether the trailer was found and its length equals the message bytes.
    pub(crate) complete: bool,
}

impl BodyException {
    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Debug for BodyException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BodyException")
            .field("message", &"<withheld>")
            .field("declared_length", &self.declared_length)
            .field("complete", &self.complete)
            .finish()
    }
}

impl fmt::Display for BodyException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the server wrote an exception block into the response body")
    }
}

impl StdError for BodyException {}

/// The label of the source failure that carries a [`BodyException`].
pub(crate) const LATE_EXCEPTION: &str = "the server reported an exception in the response body";

enum State {
    /// Looking for the opening marker; `held` may be its start.
    Scanning,
    /// The opening marker was found; collecting the block.
    Block(Vec<u8>),
    /// The inner source ended cleanly.
    Ended,
    /// An exception or a failure was reported.
    Failed,
}

/// A [`ByteSource`] that stops at ClickHouse's in-body exception block.
pub(crate) struct Guarded<S> {
    inner: S,
    /// `\r\n__exception__\r\n<TAG>\r\n`, when the response announced a tag.
    opening: Option<Vec<u8>>,
    /// ` <TAG>\r\n__exception__\r\n`.
    closing: Vec<u8>,
    held: Vec<u8>,
    state: State,
}

impl<S: ByteSource> Guarded<S> {
    /// Guards `inner` with the response's exception tag (`None`: pass-through).
    pub(crate) fn new(inner: S, tag: Option<&str>) -> Self {
        let (opening, closing) = match tag {
            Some(tag) if !tag.is_empty() => {
                let mut opening = b"\r\n".to_vec();
                opening.extend_from_slice(MARKER);
                opening.extend_from_slice(b"\r\n");
                opening.extend_from_slice(tag.as_bytes());
                opening.extend_from_slice(b"\r\n");
                let mut closing = b" ".to_vec();
                closing.extend_from_slice(tag.as_bytes());
                closing.extend_from_slice(b"\r\n");
                closing.extend_from_slice(MARKER);
                closing.extend_from_slice(b"\r\n");
                (Some(opening), closing)
            }
            _ => (None, Vec::new()),
        };
        Self {
            inner,
            opening,
            closing,
            held: Vec::new(),
            state: State::Scanning,
        }
    }

    /// The inner source.
    pub(crate) fn into_inner(self) -> S {
        self.inner
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// The length of the longest suffix of `buf` that is a proper prefix of `marker`.
fn partial_suffix(buf: &[u8], marker: &[u8]) -> usize {
    let max = (marker.len() - 1).min(buf.len());
    (1..=max)
        .rev()
        .find(|&k| buf.ends_with(&marker[..k]))
        .unwrap_or(0)
}

/// Parses a collected block (after the opening marker): `Some` once the trailer is found.
fn parse_block(block: &[u8], closing: &[u8]) -> Option<BodyException> {
    let end = find(block, closing)?;
    let body = &block[..end];
    // The length is the last line before the trailer: digits after the message's line feed.
    let (message, digits) = match body.iter().rposition(|&b| b == b'\n') {
        Some(lf) => (&body[..=lf], &body[lf + 1..]),
        None => (&body[..0], body),
    };
    let declared_length = std::str::from_utf8(digits)
        .ok()
        .and_then(|d| d.parse::<usize>().ok());
    Some(BodyException {
        message: text(message),
        declared_length,
        complete: declared_length == Some(message.len()),
    })
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}

fn incomplete(block: &[u8]) -> BodyException {
    BodyException {
        message: text(&block[..block.len().min(MAX_BLOCK_BYTES)]),
        declared_length: None,
        complete: false,
    }
}

fn late(exception: BodyException) -> SourceError {
    SourceError::new(LATE_EXCEPTION).with_source(exception)
}

impl<S: ByteSource> ByteSource for Guarded<S> {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        let Self {
            inner,
            opening,
            closing,
            held,
            state,
        } = self;
        loop {
            match state {
                State::Ended => return Ok(None),
                State::Failed => {
                    return Err(SourceError::new(
                        "the response body already reported a failure",
                    ));
                }
                State::Block(block) => {
                    if let Some(exception) = parse_block(block, closing) {
                        *state = State::Failed;
                        return Err(late(exception));
                    }
                    if block.len() > MAX_BLOCK_BYTES {
                        let exception = incomplete(block);
                        *state = State::Failed;
                        return Err(late(exception));
                    }
                    match inner.next_chunk().await {
                        Ok(Some(chunk)) => block.extend_from_slice(&chunk),
                        // The body ended (or broke) inside the block: still an exception.
                        Ok(None) | Err(_) => {
                            let exception = incomplete(block);
                            *state = State::Failed;
                            return Err(late(exception));
                        }
                    }
                }
                State::Scanning => {
                    let Some(marker) = opening.as_deref() else {
                        return match inner.next_chunk().await {
                            Ok(None) => {
                                *state = State::Ended;
                                Ok(None)
                            }
                            Err(e) => {
                                *state = State::Failed;
                                Err(e)
                            }
                            chunk => chunk,
                        };
                    };
                    match inner.next_chunk().await {
                        Ok(Some(chunk)) => {
                            let buf = if held.is_empty() {
                                chunk
                            } else {
                                let mut joined = std::mem::take(held);
                                joined.extend_from_slice(&chunk);
                                Bytes::from(joined)
                            };
                            if let Some(at) = find(&buf, marker) {
                                *state = State::Block(buf[at + marker.len()..].to_vec());
                                if at > 0 {
                                    return Ok(Some(buf.slice(..at)));
                                }
                                continue;
                            }
                            let keep = partial_suffix(&buf, marker);
                            let forward = buf.len() - keep;
                            *held = buf[forward..].to_vec();
                            if forward > 0 {
                                return Ok(Some(buf.slice(..forward)));
                            }
                        }
                        Ok(None) => {
                            *state = State::Ended;
                            if held.is_empty() {
                                return Ok(None);
                            }
                            // A held start of a marker that never completed is data.
                            return Ok(Some(Bytes::from(std::mem::take(held))));
                        }
                        Err(e) => {
                            *state = State::Failed;
                            return Err(e);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concord::stream::Chunks;

    const TAG: &str = "rumfyutuqkncbgau";

    async fn drain(mut source: Guarded<Chunks>) -> (Vec<u8>, Result<(), SourceError>) {
        let mut data = Vec::new();
        loop {
            match source.next_chunk().await {
                Ok(Some(chunk)) => data.extend_from_slice(&chunk),
                Ok(None) => return (data, Ok(())),
                Err(e) => return (data, Err(e)),
            }
        }
    }

    fn block(message: &str) -> Vec<u8> {
        format!(
            "\r\n__exception__\r\n{TAG}\r\n{message}\n{} {TAG}\r\n__exception__\r\n",
            message.len() + 1
        )
        .into_bytes()
    }

    fn exception_of(error: &SourceError) -> &BodyException {
        error
            .source_unredacted()
            .and_then(|s| s.downcast_ref::<BodyException>())
            .expect("a body exception")
    }

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(f)
    }

    #[test]
    fn a_block_split_at_every_byte_is_found() {
        let mut body = b"0,0\n0,0\n".to_vec();
        body.extend_from_slice(&block("Code: 395. DB::Exception: boom"));
        for split in 1..body.len() {
            let source = Guarded::new(
                Chunks::new([body[..split].to_vec(), body[split..].to_vec()]),
                Some(TAG),
            );
            let (data, result) = run(drain(source));
            assert_eq!(data, b"0,0\n0,0\n", "split at {split}");
            let error = result.unwrap_err();
            assert_eq!(error.label(), LATE_EXCEPTION);
            let exception = exception_of(&error);
            assert_eq!(exception.message(), "Code: 395. DB::Exception: boom");
            assert!(exception.complete, "split at {split}");
        }
    }

    #[test]
    fn data_that_only_looks_like_a_marker_start_is_forwarded() {
        let body = b"a,b\r\n__exception_not\r\n".to_vec();
        let source = Guarded::new(Chunks::new([body.clone()]), Some(TAG));
        let (data, result) = run(drain(source));
        assert!(result.is_ok());
        assert_eq!(data, body);
        // A body ending with a marker prefix keeps its bytes.
        let source = Guarded::new(Chunks::new([b"x\r\n__exc".to_vec()]), Some(TAG));
        let (data, result) = run(drain(source));
        assert!(result.is_ok());
        assert_eq!(data, b"x\r\n__exc");
    }

    #[test]
    fn another_tag_is_not_this_responses_marker() {
        let body = b"1\r\n__exception__\r\nxxxxxxxxxxxxxxxx\r\nfake\n5 xxxxxxxxxxxxxxxx\r\n__exception__\r\n".to_vec();
        let source = Guarded::new(Chunks::new([body.clone()]), Some(TAG));
        let (data, result) = run(drain(source));
        assert!(result.is_ok());
        assert_eq!(data, body);
    }

    #[test]
    fn a_truncated_block_is_still_an_exception() {
        let mut body = b"0\n".to_vec();
        body.extend_from_slice(format!("\r\n__exception__\r\n{TAG}\r\nCode: 1").as_bytes());
        for source in [
            Chunks::new([body.clone()]),
            Chunks::new([body.clone()]).then_fail("connection reset"),
        ] {
            let (data, result) = run(drain(Guarded::new(source, Some(TAG))));
            assert_eq!(data, b"0\n");
            let error = result.unwrap_err();
            let exception = exception_of(&error);
            assert!(!exception.complete);
            assert_eq!(exception.message(), "Code: 1");
        }
    }

    #[test]
    fn a_wrong_length_is_reported_incomplete() {
        let body =
            format!("\r\n__exception__\r\n{TAG}\r\nCode: 2. x\n99 {TAG}\r\n__exception__\r\n");
        let (_, result) = run(drain(Guarded::new(
            Chunks::new([body.into_bytes()]),
            Some(TAG),
        )));
        let error = result.unwrap_err();
        let exception = exception_of(&error);
        assert_eq!(exception.declared_length, Some(99));
        assert!(!exception.complete);
    }

    #[test]
    fn without_a_tag_the_body_passes_through() {
        let body = b"0\nCode: 395. DB::Exception: boom\n".to_vec();
        let (data, result) = run(drain(Guarded::new(Chunks::new([body.clone()]), None)));
        assert!(result.is_ok());
        assert_eq!(data, body);
    }

    #[test]
    fn the_documented_example_length_counts_the_line_feed() {
        // The CSV example of "HTTP response codes caveats": a 261-byte message, `262 <TAG>`.
        let message = "Code: 395. DB::Exception: Value passed to 'throwIf' function is non-zero: while executing 'FUNCTION throwIf(equals(__table1.number, 2_UInt8) :: 1) -> throwIf(equals(__table1.number, 2_UInt8)) UInt8 : 0'. (FUNCTION_THROW_IF_VALUE_IS_NON_ZERO) (version 25.11.1.1)";
        assert_eq!(message.len(), 261);
        let parsed = parse_block(&block(message)[2 + 13 + 2 + 16 + 2..], &{
            let mut c = format!(" {TAG}\r\n").into_bytes();
            c.extend_from_slice(b"__exception__\r\n");
            c
        })
        .unwrap();
        assert_eq!(parsed.declared_length, Some(262));
        assert!(parsed.complete);
    }
}
