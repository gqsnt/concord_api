use std::fmt;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord_core::contracts::{
    Buffered, Bytes, BytesMut, ItemStream, ResponseBodyError, ResponseBodyErrorKind,
    ResponseDriver, ResponseHead, ResponseRepresentation, Streaming,
};
use concord_core::records::{
    RecordDecoder, RecordError, RecordErrorKind, decode_complete_records, validate_record_decoder,
};

const MEDIA_TYPE: &str = "text/event-stream";
const MAX_LINE_BYTES: usize = 1024 * 1024;
const MAX_DATA_BYTES: usize = 1024 * 1024;
const MAX_FRAME_BYTES: usize = MAX_LINE_BYTES + 2;
const BOM: &[u8] = b"\xef\xbb\xbf";

/// WHATWG server-sent events, buffered or streamed through Concord's record driver.
///
/// `EventStream` with `Vec<ServerEvent>` acquires the complete bounded body;
/// with [`ItemStream<ServerEvent>`] it returns a lazy managed stream. Both
/// forms accept `text/event-stream` with optional media parameters. An
/// accepted HTTP 204 response produces no events, regardless of media type.
///
/// UTF-8 decoding replaces malformed sequences and strips one leading BOM.
/// LF, CRLF and CR delimit lines, including across transport chunks. A blank
/// line dispatches accumulated `data` fields, joined with LF; the default
/// event type is `message`. Comments and unknown fields are ignored, field
/// names are case sensitive, and exactly one space after a colon is removed.
/// An unfinished event at EOF is discarded, as required by the event format.
///
/// Each line and each event's returned UTF-8 data have a 1 MiB ceiling. The
/// record driver's staging ceiling also includes accumulated event data,
/// event type and an uncommitted ID, and may be reduced by the runtime frame
/// limit. Errors contain no event data or parser-authored text. This
/// representation performs one request; reconnection belongs to `EventSource`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EventStream;

/// One dispatched server-sent event and the connection's current control values.
///
/// The last event ID and retry hint persist until changed by the server.
/// An empty `id` clears the remembered ID; an ID containing NUL is ignored.
/// Retry hints are nonempty ASCII decimal milliseconds representable by
/// `u64`; malformed and overflowing hints are ignored.
#[derive(Clone, Eq, PartialEq)]
pub struct ServerEvent {
    data: String,
    event_type: String,
    id: String,
    retry: Option<Duration>,
}

impl ServerEvent {
    /// Returns the data fields joined by LF, without their final framing LF.
    #[must_use]
    pub fn data(&self) -> &str {
        &self.data
    }

    /// Returns the event type, or `message` when no nonempty type was supplied.
    #[must_use]
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    /// Returns the event type, or `message` when no nonempty type was supplied.
    #[must_use]
    pub fn event(&self) -> &str {
        &self.event_type
    }

    /// Returns the remembered event ID, including the empty string after a clear.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the remembered event ID, as in WHATWG's `lastEventId`.
    #[must_use]
    pub fn last_event_id(&self) -> &str {
        &self.id
    }

    /// Returns the most recently accepted reconnection delay hint.
    #[must_use]
    pub const fn retry(&self) -> Option<Duration> {
        self.retry
    }
}

impl fmt::Debug for ServerEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerEvent")
            .field("data_bytes", &self.data.len())
            .field("event_type_bytes", &self.event_type.len())
            .field("id_bytes", &self.id.len())
            .field("has_retry", &self.retry.is_some())
            .finish()
    }
}

/// Control state shared with the reconnection adapter, including control-only blocks.
pub(super) type SseState = Arc<Mutex<SseConnectionState>>;

#[derive(Default)]
pub(super) struct SseConnectionState {
    pub(super) id: String,
    pub(super) retry: Option<Duration>,
}

struct SseDecoder {
    state: SseState,
    data: String,
    event_type: String,
    pending_id: Option<String>,
    scan_index: usize,
    data_prefix: usize,
    strip_space_pending: bool,
    ignore_line: bool,
    at_start: bool,
    skip_lf: bool,
    stopped: bool,
}

impl SseDecoder {
    fn new(stopped: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(SseConnectionState::default())),
            data: String::new(),
            event_type: String::new(),
            pending_id: None,
            scan_index: 0,
            data_prefix: 0,
            strip_space_pending: false,
            ignore_line: false,
            at_start: true,
            skip_lf: false,
            stopped,
        }
    }

    fn process_line(&mut self, raw: &[u8]) -> Result<Option<ServerEvent>, RecordError> {
        // ASCII line boundaries cannot split a valid UTF-8 sequence, so decoding
        // complete lines has the same replacement behavior as stream decoding.
        let line = String::from_utf8_lossy(raw);
        if line.is_empty() {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(id) = self.pending_id.take() {
                state.id = id;
            }
            if self.data.is_empty() {
                self.event_type.clear();
                return Ok(None);
            }
            let mut data = std::mem::take(&mut self.data);
            data.pop(); // Every data field adds exactly one framing LF.
            let event_type = match std::mem::take(&mut self.event_type) {
                value if value.is_empty() => "message".to_owned(),
                value => value,
            };
            return Ok(Some(ServerEvent {
                data,
                event_type,
                id: state.id.clone(),
                retry: state.retry,
            }));
        }
        if line.starts_with(':') {
            return Ok(None);
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line.as_ref(), ""),
        };
        match field {
            "data" => self.append_data(value)?,
            "event" => {
                self.event_type.clear();
                self.event_type.push_str(value);
            }
            "id" if !value.contains('\0') => self.pending_id = Some(value.to_owned()),
            "retry" => {
                if let Some(milliseconds) = parse_retry(value) {
                    self.state
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .retry = Some(Duration::from_millis(milliseconds));
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn append_data(&mut self, value: &str) -> Result<(), RecordError> {
        let next = self
            .data
            .len()
            .checked_add(value.len())
            .and_then(|length| length.checked_add(1));
        if !next.is_some_and(|length| length <= MAX_DATA_BYTES + 1) {
            return Err(RecordError::new(RecordErrorKind::LimitExceeded));
        }
        self.data.push_str(value);
        self.data.push('\n');
        Ok(())
    }

    fn decode_available(
        &mut self,
        buffer: &mut BytesMut,
        eof: bool,
    ) -> Result<Option<ServerEvent>, RecordError> {
        if self.stopped {
            buffer.clear();
            return Ok(None);
        }
        if self.at_start {
            if buffer.is_empty() || (!eof && buffer.len() < BOM.len() && BOM.starts_with(buffer)) {
                return Ok(None);
            }
            self.at_start = false;
            if buffer.starts_with(BOM) {
                drop(buffer.split_to(BOM.len()));
            }
        }
        if self.skip_lf {
            if buffer.is_empty() && !eof {
                return Ok(None);
            }
            if buffer.first() == Some(&b'\n') {
                drop(buffer.split_to(1));
            }
            self.skip_lf = false;
        }
        // Consume even a partial prefix: near the event ceiling there may be
        // too little staging space left to hold all five framing bytes.
        // A mismatching field beginning with "d" is necessarily unknown.
        if !self.ignore_line && self.data_prefix < 5 {
            while self.data_prefix < 5 {
                let Some(byte) = buffer.first() else {
                    break;
                };
                if *byte != b"data:"[self.data_prefix] {
                    if self.data_prefix != 0 {
                        self.ignore_line =
                            !(self.data_prefix == 4 && matches!(*byte, b'\r' | b'\n'));
                    }
                    break;
                }
                drop(buffer.split_to(1));
                self.data_prefix += 1;
                self.scan_index = 0;
                if self.data_prefix == 5 {
                    self.strip_space_pending = true;
                }
            }
        }
        if self.strip_space_pending {
            if buffer.is_empty() && !eof {
                return Ok(None);
            }
            if buffer.first() == Some(&b' ') {
                drop(buffer.split_to(1));
                self.data_prefix += 1;
            }
            self.strip_space_pending = false;
        }
        let end = buffer[self.scan_index..]
            .iter()
            .position(|byte| matches!(*byte, b'\r' | b'\n'))
            .map(|relative| self.scan_index + relative);
        if let Some(end) = end {
            if self.data_prefix + end > MAX_LINE_BYTES {
                return Err(RecordError::new(RecordErrorKind::LimitExceeded));
            }
            self.skip_lf = buffer[end] == b'\r';
            let mut line = buffer.split_to(end + 1);
            line.truncate(end);
            self.scan_index = 0;
            let data_line = !self.ignore_line && self.data_prefix >= 4;
            let ignored_line = self.ignore_line || (self.data_prefix != 0 && !data_line);
            self.data_prefix = 0;
            self.ignore_line = false;
            if data_line {
                self.append_data(&String::from_utf8_lossy(&line))?;
            } else if !ignored_line {
                return self.process_line(&line);
            }
            // Let core recheck accumulated storage and cancellation after
            // each consumed line, including a control-only heartbeat.
            return Ok(None);
        }
        self.scan_index = buffer.len();
        if self.data_prefix + buffer.len() > MAX_LINE_BYTES {
            return Err(RecordError::new(RecordErrorKind::LimitExceeded));
        }
        if eof {
            buffer.clear();
            self.data.clear();
            self.event_type.clear();
            self.pending_id = None;
            self.scan_index = 0;
            self.data_prefix = 0;
            self.strip_space_pending = false;
            self.ignore_line = false;
        }
        Ok(None)
    }
}

impl RecordDecoder for SseDecoder {
    type Record = ServerEvent;

    const ACCEPT: Option<&'static str> = Some(MEDIA_TYPE);

    fn validate(head: &ResponseHead<'_>) -> Result<(), RecordError> {
        if head.status.as_u16() == 204
            || head
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

    fn start(head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self::new(head.status.as_u16() == 204))
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        self.decode_available(buffer, false)
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        self.decode_available(buffer, true)
    }

    fn buffered_bytes(&self) -> usize {
        self.data.len() + self.event_type.len() + self.pending_id.as_ref().map_or(0, String::len)
    }

    fn input_bytes(&self, input: &[u8]) -> usize {
        input
            .iter()
            .position(|byte| matches!(*byte, b'\r' | b'\n'))
            .map_or(input.len(), |end| end + 1)
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(MAX_FRAME_BYTES).expect("the built-in event staging bound is positive")
    }
}

impl ResponseRepresentation<Vec<ServerEvent>> for EventStream {
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = Some(MEDIA_TYPE);

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        validate_record_decoder::<SseDecoder>(head)
    }

    fn from_body(
        body: Bytes,
        head: &ResponseHead<'_>,
    ) -> Result<Vec<ServerEvent>, ResponseBodyError> {
        decode_complete_records::<SseDecoder>(body, head)
    }
}

impl ResponseRepresentation<ItemStream<ServerEvent>> for EventStream {
    type Driver = Streaming;

    const ACCEPT: Option<&'static str> = Some(MEDIA_TYPE);

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        validate_record_decoder::<SseDecoder>(head)
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<ItemStream<ServerEvent>, ResponseBodyError> {
        let decoder = SseDecoder::start(head)
            .map_err(|_| ResponseBodyError::new(ResponseBodyErrorKind::Internal))?;
        let state = Arc::clone(&decoder.state);
        let mut stream = ItemStream::decode_with_decoder(body, decoder);
        stream.set_extension(state);
        Ok(stream)
    }
}

fn parse_retry(value: &str) -> Option<u64> {
    if value.is_empty() {
        return None;
    }
    value.bytes().try_fold(0_u64, |milliseconds, byte| {
        if !byte.is_ascii_digit() {
            return None;
        }
        milliseconds
            .checked_mul(10)?
            .checked_add(u64::from(byte - b'0'))
    })
}

fn is_event_stream_media_type(value: &str) -> bool {
    value.parse::<mime::Mime>().is_ok_and(|media| {
        media.type_() == mime::TEXT && media.subtype().as_str() == "event-stream"
    })
}

#[cfg(test)]
mod tests;
