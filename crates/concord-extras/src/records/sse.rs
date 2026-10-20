use std::num::NonZeroUsize;
use std::time::Duration;

use concord_core::contracts::{
    Buffered, Bytes, BytesMut, ItemStream, ResponseBodyError, ResponseDriver, ResponseHead,
    ResponseRepresentation, Streaming,
};
use concord_core::records::{
    RecordDecoder, RecordError, RecordErrorKind, decode_complete_records, validate_record_decoder,
};

const MAX_LINE_BYTES: usize = 1024 * 1024;
const MAX_DATA_BYTES: usize = 1024 * 1024;
// Room for the first line's ignored UTF-8 BOM and one line terminator.
const MAX_FRAME_BYTES: usize = MAX_LINE_BYTES + 4;
const MEDIA_TYPE: &str = "text/event-stream";

/// One dispatched server-sent event, including the stream state at dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerEvent {
    event: String,
    data: String,
    last_event_id: String,
    retry: Option<Duration>,
}

impl ServerEvent {
    /// The event type, or `"message"` when no nonempty type was supplied.
    pub fn event(&self) -> &str {
        &self.event
    }

    /// Event data, with multiple `data` fields joined by LF.
    pub fn data(&self) -> &str {
        &self.data
    }

    /// The most recently set event ID, persisting across event blocks.
    pub fn last_event_id(&self) -> &str {
        &self.last_event_id
    }

    /// The most recent valid `retry` field, interpreted as milliseconds.
    pub fn retry(&self) -> Option<Duration> {
        self.retry
    }
}

/// WHATWG server-sent event response records (`text/event-stream`).
///
/// Follows [interpreting an event stream](https://html.spec.whatwg.org/multipage/server-sent-events.html#interpreting-an-event-stream).
///
/// Supports complete bounded `Vec<ServerEvent>` responses and managed lazy
/// [`ItemStream<ServerEvent>`] responses. UTF-8 errors become replacement
/// characters, one initial BOM is ignored, and CRLF, CR and LF delimit lines.
/// Only blank-line-terminated blocks containing a `data` field dispatch;
/// an unfinished event at EOF is discarded. IDs and valid retry times persist.
/// This representation does not reconnect or send `Last-Event-ID`.
///
/// Lines (excluding their terminator and initial BOM) and event data are each
/// limited to 1 MiB. Core additionally enforces the runtime staging ceiling,
/// stream byte, record, idle and lease limits. Consumed comments and ignored
/// fields do not accumulate in staging. Limit errors contain no field values
/// and terminate a managed stream with one frame-limit error.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EventStream;

#[derive(Default)]
struct EventDecoder {
    scan_index: usize,
    first_line: bool,
    skip_lf: bool,
    event: String,
    data: Option<String>,
    last_event_id: String,
    retry: Option<Duration>,
}

impl EventDecoder {
    fn new() -> Self {
        Self {
            first_line: true,
            ..Self::default()
        }
    }

    fn line(&mut self, bytes: &[u8]) -> Result<Option<ServerEvent>, RecordError> {
        let bytes = if self.first_line {
            self.first_line = false;
            bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes)
        } else {
            bytes
        };
        if bytes.len() > MAX_LINE_BYTES {
            return Err(limit_error());
        }
        if bytes.is_empty() {
            let event = std::mem::take(&mut self.event);
            return Ok(self.data.take().map(|data| ServerEvent {
                event: if event.is_empty() {
                    "message".into()
                } else {
                    event
                },
                data,
                last_event_id: self.last_event_id.clone(),
                retry: self.retry,
            }));
        }
        if bytes[0] == b':' {
            return Ok(None);
        }
        // ASCII delimiters cannot occur within a UTF-8 sequence, so decoding
        // a complete line lossily is equivalent to streaming UTF-8 decoding.
        let line = String::from_utf8_lossy(bytes);
        let (field, value) = line
            .split_once(':')
            .map_or((line.as_ref(), ""), |(field, value)| {
                (field, value.strip_prefix(' ').unwrap_or(value))
            });
        match field {
            "event" => self.event = value.into(),
            "data" => {
                let previous = self.data.as_ref().map_or(0, |data| data.len() + 1);
                if previous + value.len() > MAX_DATA_BYTES {
                    return Err(limit_error());
                }
                if let Some(data) = &mut self.data {
                    data.push('\n');
                    data.push_str(value);
                } else {
                    self.data = Some(value.into());
                }
            }
            "id" if !value.contains('\0') => self.last_event_id = value.into(),
            "retry" if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
                if let Ok(milliseconds) = value.parse::<u64>() {
                    self.retry = Some(Duration::from_millis(milliseconds));
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn decode_available(
        &mut self,
        buffer: &mut BytesMut,
    ) -> Result<Option<ServerEvent>, RecordError> {
        loop {
            if self.skip_lf && !buffer.is_empty() {
                self.skip_lf = false;
                if buffer[0] == b'\n' {
                    drop(buffer.split_to(1));
                }
            }
            let Some(relative) = buffer[self.scan_index..]
                .iter()
                .position(|byte| matches!(byte, b'\r' | b'\n'))
            else {
                self.scan_index = buffer.len();
                let bom = usize::from(self.first_line && buffer.starts_with(b"\xef\xbb\xbf")) * 3;
                if buffer.len() - bom > MAX_LINE_BYTES {
                    return Err(limit_error());
                }
                return Ok(None);
            };
            let end = self.scan_index + relative;
            let line = buffer.split_to(end + 1);
            self.scan_index = 0;
            self.skip_lf = line[end] == b'\r';
            if let Some(event) = self.line(&line[..end])? {
                return Ok(Some(event));
            }
        }
    }
}

fn limit_error() -> RecordError {
    RecordError::new(RecordErrorKind::LimitExceeded)
}

impl RecordDecoder for EventDecoder {
    type Record = ServerEvent;
    const ACCEPT: Option<&'static str> = Some(MEDIA_TYPE);

    fn validate(head: &ResponseHead<'_>) -> Result<(), RecordError> {
        if head
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(is_event_stream_media_type)
        {
            Ok(())
        } else {
            Err(RecordError::new(RecordErrorKind::InvalidMediaType))
        }
    }

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self::new())
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<ServerEvent>, RecordError> {
        self.decode_available(buffer)
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<ServerEvent>, RecordError> {
        if let Some(event) = self.decode_available(buffer)? {
            return Ok(Some(event));
        }
        buffer.clear();
        self.scan_index = 0;
        self.data = None;
        self.event.clear();
        Ok(None)
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(MAX_FRAME_BYTES).expect("the SSE staging bound is positive")
    }
}

fn is_event_stream_media_type(value: &str) -> bool {
    value.parse::<mime::Mime>().is_ok_and(|media| {
        media.type_() == mime::TEXT
            && media.subtype().as_str() == "event-stream"
            && media.suffix().is_none()
    })
}

impl ResponseRepresentation<Vec<ServerEvent>> for EventStream {
    type Driver = Buffered;
    const ACCEPT: Option<&'static str> = Some(MEDIA_TYPE);

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        validate_record_decoder::<EventDecoder>(head)
    }

    fn from_body(
        body: Bytes,
        head: &ResponseHead<'_>,
    ) -> Result<Vec<ServerEvent>, ResponseBodyError> {
        decode_complete_records::<EventDecoder>(body, head)
    }
}

impl ResponseRepresentation<ItemStream<ServerEvent>> for EventStream {
    type Driver = Streaming;
    const ACCEPT: Option<&'static str> = Some(MEDIA_TYPE);

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        validate_record_decoder::<EventDecoder>(head)
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<ItemStream<ServerEvent>, ResponseBodyError> {
        ItemStream::decode_with::<EventDecoder>(body, head)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(chunks: impl IntoIterator<Item = Vec<u8>>) -> Result<Vec<ServerEvent>, RecordError> {
        let mut decoder = EventDecoder::new();
        let mut buffer = BytesMut::new();
        let mut events = Vec::new();
        for chunk in chunks {
            buffer.extend_from_slice(&chunk);
            while let Some(event) = decoder.decode(&mut buffer)? {
                events.push(event);
            }
        }
        while let Some(event) = decoder.decode_eof(&mut buffer)? {
            events.push(event);
        }
        assert!(buffer.is_empty());
        Ok(events)
    }

    #[test]
    fn interpretation_is_invariant_at_every_split_and_one_byte_chunks() {
        let body = b"\xef\xbb\xbf: comment\r\nid: first\revent: ignored\n\ndata: caf\xc3\xa9\r\ndata:\rdata: \xff\nretry: 0012\nevent: update\n\nid: bad\0id\ndata: next\n\nid\nretry: +2\nretry: 18446744073709551616\nevent:\ndata\n\nevent: unfinished\ndata: discarded\n";
        let expected = feed([body.to_vec()]).expect("whole body");
        assert_eq!(expected.len(), 3);
        assert_eq!(expected[0].event(), "update");
        assert_eq!(expected[0].data(), "café\n\n\u{fffd}");
        assert_eq!(expected[0].last_event_id(), "first");
        assert_eq!(expected[0].retry(), Some(Duration::from_millis(12)));
        assert_eq!(expected[1].event(), "message");
        assert_eq!(expected[1].data(), "next");
        assert_eq!(expected[1].last_event_id(), "first");
        assert_eq!(expected[2].data(), "");
        assert_eq!(expected[2].last_event_id(), "");
        assert_eq!(expected[2].retry(), Some(Duration::from_millis(12)));
        for split in 0..=body.len() {
            assert_eq!(
                feed([body[..split].to_vec(), body[split..].to_vec()]).expect("split body"),
                expected,
                "split {split}"
            );
        }
        assert_eq!(
            feed(body.iter().map(|byte| vec![*byte])).expect("bytes"),
            expected
        );
    }

    #[test]
    fn fields_are_case_sensitive_and_remove_only_one_space() {
        let events = feed([b"Data: ignored\n data: ignored\ndata:  spaced:colon\nevent: named\nevent: final\nid:  id\nretry: 0\n\ndata:\xef\xbb\xbf\n\n".to_vec()]).expect("fields");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event(), "final");
        assert_eq!(events[0].data(), " spaced:colon");
        assert_eq!(events[0].last_event_id(), " id");
        assert_eq!(events[0].retry(), Some(Duration::ZERO));
        assert_eq!(events[1].data(), "\u{feff}");
    }

    #[test]
    fn eof_discards_every_unterminated_block_and_empty_blocks_dispatch_nothing() {
        for body in [
            &b"data: unfinished"[..],
            &b"data: unfinished\n"[..],
            &b"data: unfinished\r\n"[..],
            &b": comment\n\nid: retained\n\nevent: unused\n\n"[..],
            &b"\xef\xbb\xbf"[..],
            &b""[..],
        ] {
            assert!(
                feed([body.to_vec()])
                    .expect("EOF is not an error")
                    .is_empty()
            );
        }
        assert_eq!(
            feed([b"data: dispatched\r\r".to_vec()])
                .expect("CR blank line")
                .len(),
            1
        );
    }

    #[test]
    fn utf8_replacement_matches_whole_stream_decoding() {
        for invalid in [
            &b"\xf0\x90\x80"[..],
            &b"\xc0\xaf"[..],
            &b"\xed\xa0\x80"[..],
            &b"\xe2\x82x\x80"[..],
            &b"\xef\xbb"[..],
        ] {
            let mut body = b"data: ".to_vec();
            body.extend_from_slice(invalid);
            body.extend_from_slice(b"\n\n");
            let events = feed(body.iter().map(|byte| vec![*byte])).expect("lossy UTF-8");
            assert_eq!(events[0].data(), String::from_utf8_lossy(invalid));
        }
    }

    #[test]
    fn exact_line_and_data_bounds_and_first_excess_are_enforced_without_values() {
        let exact_comment = format!(":{}\n\n", "x".repeat(MAX_LINE_BYTES - 1));
        assert!(
            feed([exact_comment.into_bytes()])
                .expect("exact line")
                .is_empty()
        );
        let exact_data = format!(
            "data:{}\ndata:{}\n\n",
            "a".repeat(MAX_DATA_BYTES - 6),
            "b".repeat(5)
        );
        let events = feed([exact_data.into_bytes()]).expect("exact event data");
        assert_eq!(events[0].data().len(), MAX_DATA_BYTES);
        let oversized_line = format!(":{}", "private".repeat(MAX_LINE_BYTES / 7 + 1));
        let oversized_data = format!(
            "data:{}\ndata:{}\n\n",
            "a".repeat(MAX_DATA_BYTES - 5),
            "b".repeat(5)
        );
        // Invalid UTF-8 expands the decoded data beyond the raw line size.
        let mut expanded_data = b"data:".to_vec();
        expanded_data.extend(std::iter::repeat_n(0xff, MAX_DATA_BYTES / 3 + 1));
        expanded_data.extend_from_slice(b"\n\n");
        for body in [
            oversized_line.into_bytes(),
            oversized_data.into_bytes(),
            expanded_data,
        ] {
            let error = feed([body]).expect_err("limit exceeded");
            assert_eq!(error.kind(), RecordErrorKind::LimitExceeded);
            assert!(!format!("{error} {error:?}").contains("private"));
        }
    }

    #[test]
    fn completed_keepalives_and_ignored_blocks_release_staging() {
        let mut decoder = EventDecoder::new();
        let mut buffer = BytesMut::new();
        for _ in 0..10_000 {
            buffer.extend_from_slice(b": ping\nunknown: ignored\nid: same\nevent: ignored\n\n");
            assert!(decoder.decode(&mut buffer).expect("keepalive").is_none());
            assert!(buffer.is_empty());
            assert!(decoder.data.is_none());
            assert!(decoder.event.is_empty());
        }
    }

    #[test]
    fn media_type_is_case_insensitive_with_parameters() {
        for media in ["text/event-stream", "TEXT/EVENT-STREAM; charset=UTF-8"] {
            assert!(is_event_stream_media_type(media));
        }
        for media in [
            "application/json",
            "text/event-stream-extra",
            "text/event-stream+json",
            "text/event-stream; invalid=\"",
        ] {
            assert!(!is_event_stream_media_type(media));
        }
    }
}
