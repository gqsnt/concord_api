//! Records that go through a mapping of the author's own: a typed stream whose errors are the
//! client's own error.

#[cfg(feature = "json")]
use super::JsonLines;
use super::{Events, Lines, Pull, RecordError, RecordStream, Source, Sse, SseEvent, Work};
use concord_core::read::{Incoming, ReadError, Reader, RecordBody, RecordFault};
use futures_core::Stream;
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

/// What a mapping does with a record that fits: yield a value, skip the record, or end the
/// stream. Why a stream fails is the other half of the mapping's result, a [`RecordFail`].
///
/// A mapping is a function of your own that receives each record (an [`SseEvent`] for
/// [`Sse::try_map`], a `JsonLine` for `JsonLines::try_map`) and returns
/// `Result<Step<T>, RecordFail<P>>`, where `T` is the item of the stream and `P` the problem type
/// of the client. Each outcome reaches the caller of `try_next` as follows:
///
/// | The mapping returns | `try_next` returns | And then |
/// |---|---|---|
/// | `Ok(Next(value))` | `Ok(Some(value))` | The next record is read. |
/// | `Ok(Skip)` | nothing for this record | The next record is read, in the same call, within its work budget. |
/// | `Ok(Last(value))` | `Ok(Some(value))` | `Ok(None)`. The body is released at once: nothing after this record is read. |
/// | `Ok(End)` | `Ok(None)` | `Ok(None)`. The body is released at once: nothing after this record is read. |
/// | `Err(fail)` | `Err(error)`, as [`RecordFail`] says | `Ok(None)`. The body is released. |
///
/// `Debug` prints the name of the variant, never its value.
///
/// ```no_run
/// use concord::read::{RecordFail, RecordStream, Sse, SseEvent, Step};
/// use serde::Deserialize;
///
/// /// `{"type": "error", "error": {"type": "overloaded_error", "message": "…"}}`.
/// #[derive(Debug, Deserialize)]
/// pub struct ApiProblem {
///     pub error: ErrorObject,
/// }
/// #[derive(Debug, Deserialize)]
/// pub struct ErrorObject {
///     #[serde(rename = "type")]
///     pub kind: String,
/// }
///
/// /// The data of every event but `ping` and `error`: its `type` names the variant.
/// #[derive(Debug, Deserialize)]
/// #[serde(tag = "type", rename_all = "snake_case")]
/// pub enum StreamEvent {
///     MessageStart,
///     ContentBlockDelta { index: usize },
///     MessageStop,
///     #[serde(other)]
///     Unknown,
/// }
///
/// /// The event's name chooses what the event is.
/// pub fn event(event: SseEvent) -> Result<Step<StreamEvent>, RecordFail<ApiProblem>> {
///     Ok(match event.event.as_str() {
///         "ping" => Step::Skip,
///         "error" => return Err(RecordFail::problem(event.json()?)),
///         _ => match event.json()? {
///             StreamEvent::MessageStop => Step::Last(StreamEvent::MessageStop),
///             typed => Step::Next(typed),
///         },
///     })
/// }
///
/// concord::api! {
///     pub client Chat {
///         base "https://chat.example.test/v1";
///         problem ApiProblem read concord::Json;
///
///         POST stream() -> RecordStream<StreamEvent, chat::Error> {
///             path "messages";
///             read Sse::new(64 * concord::KIB).try_map(event).until_last_record();
///         }
///     }
/// }
///
/// async fn deltas(chat: &Chat) -> chat::Result<usize> {
///     let mut events = chat.stream().await?;                  // the call's error
///     let mut deltas = 0;
///     while let Some(event) = events.try_next().await? {      // the same error type
///         if let StreamEvent::ContentBlockDelta { .. } = event {
///             deltas += 1;
///         }
///     }
///     Ok(deltas)                                              // `message_stop` was the last event
/// }
/// ```
#[non_exhaustive]
pub enum Step<T> {
    /// Yields this value; the stream goes on.
    Next(T),
    /// Yields nothing for this record; the stream goes on.
    Skip,
    /// Yields this value, the last one: the stream ends after it, and nothing after this
    /// record is read.
    Last(T),
    /// Ends the stream at this record, which yields nothing (a `[DONE]` that is not data).
    End,
}

impl<T> fmt::Debug for Step<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Next(_) => "Next(..)",
            Self::Skip => "Skip",
            Self::Last(_) => "Last(..)",
            Self::End => "End",
        })
    }
}

/// Why a mapping fails the stream: the API's own problem, a record that does not fit, or a
/// record that does not decode. The stream ends with the client's own error, `Error<P>`.
///
/// | Made by | The caller's error |
/// |---|---|
/// | [`RecordFail::problem`] | Refused, with `problem()`: `refused: status 200, in chat.stream` |
/// | [`RecordFail::mismatch`] | Mismatch, `reason()` is the fixed reason: `mismatch: status 200, record 7 does not fit the stream, in chat.stream` |
/// | `?` on `SseEvent::json` or `JsonLine::json` | Mismatch: `mismatch: status 200, record 7 could not be decoded, in chat.stream`, with the line and the column in `details()` |
/// | `?` on any other [`RecordError`] | Mismatch, with no reason: `record 7 does not fit the stream` |
///
/// **Only the stream at hand supplies the error.** As for an arm that decodes a body itself
/// ([`Fail::from_read`](concord_core::Fail::from_read)), a `RecordFail` keeps only what a
/// decoder's failure is: the line and the column where the decoder stopped, inside the record.
/// Concord does not verify that the decoder was given this record, so they are said of the
/// decoder's input, in `details()` only. Any other `RecordError` (a fault of framing, the
/// failure of a body, including one taken from another stream) keeps nothing: the stream ends
/// with the generic Mismatch above. The status, the headers, the endpoint, the attempts and the
/// record's position are always those of the stream that the mapping runs on, the position
/// counted by the stream's own framer.
///
/// `?` converts a `RecordError` into a `RecordFail`, so that a mapping decodes with
/// `event.json()?`. It cannot also convert the problem: `Err(RecordFail::problem(p))`.
///
/// `Debug` prints the kind of failure, never the problem.
///
/// ```no_run
/// use concord::read::{JsonLine, RecordFail, Step};
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize)]
/// struct Status { code: Option<u16> }
/// #[derive(Deserialize)]
/// #[serde(tag = "type", content = "object", rename_all = "UPPERCASE")]
/// enum Line { Added(String), Bookmark { version: String }, Error(Status) }
///
/// fn line(line: JsonLine) -> Result<Step<String>, RecordFail<Status>> {
///     match line.json()? {
///         Line::Added(name) => Ok(Step::Next(name)),
///         Line::Bookmark { version } if version.is_empty() => Err(RecordFail::mismatch("a bookmark without a version")),
///         Line::Bookmark { .. } => Ok(Step::Skip),
///         Line::Error(status) => Err(RecordFail::problem(status)),
///     }
/// }
/// ```
pub struct RecordFail<P> {
    cause: Cause<P>,
}

enum Cause<P> {
    Problem(P),
    Rejected(&'static str),
    Decode { line: usize, column: usize },
    Other,
}

impl<P> RecordFail<P> {
    /// The record is the API's own error: the stream ends with a Refused error whose `problem()`
    /// is `problem`, with the status and the headers of the response.
    ///
    /// ```no_run
    /// use concord::read::RecordFail;
    /// struct Status { code: u16 }
    /// let fail = RecordFail::problem(Status { code: 410 });
    /// ```
    pub fn problem(problem: P) -> Self {
        Self {
            cause: Cause::Problem(problem),
        }
    }

    /// The record, or the sequence of records so far, does not fit what the client expects:
    /// the stream ends with the Mismatch `record 7 does not fit the stream`, whose `reason()`
    /// and `details()` give `reason`. It is never printed in the error's text, `Debug` or cause.
    /// The reason is a fixed sentence of your program and must hold nothing of the response, as
    /// the reason of [`concord::mismatch`](concord_core::mismatch) in an arm.
    ///
    /// ```no_run
    /// use concord::read::RecordFail;
    /// let fail: RecordFail<()> = RecordFail::mismatch("message_stop before message_start");
    /// ```
    pub fn mismatch(reason: &'static str) -> Self {
        Self {
            cause: Cause::Rejected(reason),
        }
    }
}

impl<P> RecordFail<P> {
    // The only way a failure reaches the caller: through the body of the stream at hand, which
    // supplies the status, the headers, the endpoint, the attempts and the record's position.
    fn into_error(self, body: RecordBody<P>) -> concord_core::Error<P> {
        match self.cause {
            Cause::Problem(problem) => body.refuse(problem),
            Cause::Rejected(reason) => body.fail(RecordFault::rejected(reason)),
            Cause::Decode { line, column } => {
                body.fail(RecordFault::not_decoded().at(line, column))
            }
            Cause::Other => body.fail(RecordFault::does_not_fit()),
        }
    }
}

impl<P> From<RecordError> for RecordFail<P> {
    fn from(error: RecordError) -> Self {
        Self {
            cause: match error {
                RecordError::Decode { line, column, .. } => Cause::Decode { line, column },
                _ => Cause::Other,
            },
        }
    }
}

impl<P> fmt::Debug for RecordFail<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.cause {
            Cause::Problem(_) => "RecordFail(problem)",
            Cause::Rejected(_) => "RecordFail(mismatch)",
            Cause::Decode { .. } => "RecordFail(decode)",
            Cause::Other => "RecordFail(other)",
        })
    }
}

/// One line of JSON Lines, checked as UTF-8, for a mapping of [`JsonLines::try_map`]
/// (feature `json`).
///
/// `Debug` prints no text.
///
/// ```no_run
/// use concord::read::JsonLine;
/// fn id(line: &JsonLine) -> Result<u64, concord::read::RecordError> {
///     line.json()
/// }
/// ```
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
pub struct JsonLine {
    text: String,
}

#[cfg(feature = "json")]
impl JsonLine {
    /// Decodes the line as JSON, discarding the decoder's message: the sibling of
    /// [`SseEvent::json`]. An empty line does not decode.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError::Decode`] when the line does not decode as `T`, with the line and
    /// the column of the decoder; `?` in a mapping turns it into a [`RecordFail`].
    ///
    /// ```no_run
    /// use concord::read::{JsonLine, RecordFail, Step};
    /// fn id(line: JsonLine) -> Result<Step<u64>, RecordFail<()>> {
    ///     Ok(Step::Next(line.json()?))
    /// }
    /// ```
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, RecordError> {
        super::decode(self.text.as_bytes(), None)
    }

    /// Returns the text of the line, without its line end.
    ///
    /// ```no_run
    /// fn text(line: &concord::read::JsonLine) -> &str {
    ///     line.text()
    /// }
    /// ```
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[cfg(feature = "json")]
impl fmt::Debug for JsonLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JsonLine { .. }")
    }
}

/// A reader of records whose mapping is given, and whose end is not chosen yet: finish it with
/// [`until_last_record`](TryMap::until_last_record) or
/// [`until_end_of_body`](TryMap::until_end_of_body). Without one of the two, it reads nothing,
/// and the description does not compile.
///
/// The finished reader reads as the reader that it maps does, with the same bound, and gives a
/// `RecordStream<T, concord::Error<P>>`, read only by a client whose problem type is `P`. Each
/// record is held to the bound of the reader, and the mapping receives it whole; nothing is held
/// beyond one record and the value that the mapping returns. The mapping runs inside
/// `try_next`, to its end: dropping a `try_next` never leaves a record half mapped. All the
/// records skipped in one poll share one work budget (16 KiB and 64 chunks); when it is spent,
/// the call's timeout is checked, the poll yields to the executor, and the next poll goes on,
/// so a server that sends only skipped records holds the caller, not a thread, until the call's
/// timeout ends it. A panic of
/// the mapping reaches the caller of `try_next`; the body and its place in any cap on concurrency
/// are released as it unwinds, and a later pull returns `Ok(None)`.
///
/// The mapping is cloned for each response: state that it owns starts afresh with each stream,
/// and state that it holds behind an `Arc` is shared by every stream. It must own what it holds (`'static`): a closure written in `read` that needs an input
/// of the call takes an owned copy first,
/// `read { let prefix = prefix.clone(); Sse::new(n).try_map(move |event| …).until_end_of_body() };`
/// because the description gives `read` each input by reference.
///
/// The errors of the stream, each once, before `Ok(None)`:
///
/// | When | Kind | Text |
/// |---|---|---|
/// | The mapping returns [`RecordFail::problem`] | Refused, with `problem()` | `refused: status 200, in chat.stream` |
/// | The mapping returns [`RecordFail::mismatch`], or `?` on a `RecordError` that is not a decoder's | Mismatch | `mismatch: status 200, record 3 does not fit the stream, in chat.stream` |
/// | The mapping's decoder fails (`?` on `json()`) | Mismatch | `mismatch: status 200, record 3 could not be decoded, in chat.stream` |
/// | A record exceeds the bound | Mismatch | `mismatch: status 200, record 3 exceeds the reader's size limit, in chat.stream` |
/// | A record is not UTF-8 | Mismatch | `mismatch: status 200, record 3 is not UTF-8, in chat.stream` |
/// | The body ends before a record ended the stream, under `until_last_record` | Mismatch | `mismatch: status 200, the body ended before its last record, in chat.stream` |
/// | The body breaks, or a timeout expires | Transport | the text of the core, as for a stream of bytes |
///
/// No text holds a value of the response, and no error keeps a byte of a record or a message
/// of a decoder.
#[must_use = "finish the reader with `.until_last_record()` or `.until_end_of_body()`"]
pub struct TryMap<R, F> {
    reader: R,
    map: F,
}

/// The two words that finish [`try_map`](Sse::try_map). No type implements this trait: it is
/// the bound that the compiler names when a mapped reader is used before its end is chosen.
#[diagnostic::on_unimplemented(
    message = "the reader made by `try_map` does not say how its stream ends",
    label = "finish this reader with `.until_last_record()` or `.until_end_of_body()`",
    note = "`.until_last_record()`: a record ends the stream (`Step::Last` or `Step::End`), and a body that ends before it is an error",
    note = "`.until_end_of_body()`: the end of the body ends the stream"
)]
pub trait EndOfRecords {}

// A `TryMap` is not a reader: it only names the missing word in the compiler's message.
impl<R, F, T, P> Reader<RecordStream<T, concord_core::Error<P>>> for TryMap<R, F>
where
    Self: EndOfRecords + Send + Sync,
{
    async fn read(
        &self,
        body: Incoming,
    ) -> Result<RecordStream<T, concord_core::Error<P>>, ReadError> {
        let _ = (body, &self.reader, &self.map);
        Err(ReadError::wrong_content_type())
    }
}

impl<R: fmt::Debug, F> fmt::Debug for TryMap<R, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TryMap")
            .field("reader", &self.reader)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug)]
enum End {
    LastRecord,
    EndOfBody,
}

// The finished reader. Its type is never written by an author: the two words return it as an
// `impl Reader`.
#[derive(Clone)]
struct Mapped<R, F> {
    reader: R,
    map: F,
    end: End,
}

impl<R: fmt::Debug, F> fmt::Debug for Mapped<R, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mapped")
            .field("reader", &self.reader)
            .field("end", &self.end)
            .finish_non_exhaustive()
    }
}

impl Sse {
    /// Gives each event to `map`, which says what the event is ([`Step`]) or why the stream
    /// fails ([`RecordFail`]). Finish the reader with one of the two words of [`TryMap`].
    ///
    /// ```no_run
    /// use concord::read::{RecordFail, RecordStream, Sse, Step};
    ///
    /// concord::api! {
    ///     client Feed {
    ///         base "https://feed.example.test";
    ///         GET lines() -> RecordStream<String, feed::Error> {
    ///             path "lines";
    ///             read Sse::new(16 * concord::KIB)
    ///                 .try_map(|event| Ok(match event.event.as_str() {
    ///                     "keepalive" => Step::Skip,
    ///                     _ => Step::Next(event.data),
    ///                 }))
    ///                 .until_end_of_body();
    ///         }
    ///     }
    /// }
    /// ```
    pub fn try_map<T, P, F>(self, map: F) -> TryMap<Sse, F>
    where
        F: FnMut(SseEvent) -> Result<Step<T>, RecordFail<P>> + Clone + Send + Sync + 'static,
    {
        TryMap { reader: self, map }
    }
}

#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
impl JsonLines {
    /// Gives each line to `map`, which decodes it with [`JsonLine::json`] and says what the
    /// record is ([`Step`]) or why the stream fails ([`RecordFail`]). Finish the reader with one
    /// of the two words of [`TryMap`].
    ///
    /// ```no_run
    /// use concord::read::{JsonLine, JsonLines, RecordFail, Step};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Line { id: u64, done: bool }
    ///
    /// fn line(line: JsonLine) -> Result<Step<u64>, RecordFail<()>> {
    ///     let line: Line = line.json()?;
    ///     Ok(if line.done { Step::Last(line.id) } else { Step::Next(line.id) })
    /// }
    /// let reader = JsonLines::new(4 * concord::KIB).try_map(line).until_last_record();
    /// ```
    pub fn try_map<T, P, F>(self, map: F) -> TryMap<JsonLines, F>
    where
        F: FnMut(JsonLine) -> Result<Step<T>, RecordFail<P>> + Clone + Send + Sync + 'static,
    {
        TryMap { reader: self, map }
    }
}

impl<F> TryMap<Sse, F> {
    /// Finishes the reader: a record ends the stream ([`Step::Last`] or [`Step::End`]), and a
    /// body that ends cleanly before one did is the Mismatch
    /// `the body ended before its last record`. For an API whose stream ends with a record of
    /// its own (`message_stop`, `[DONE]`), so that a reply cut short at a record boundary is
    /// never taken for a whole one.
    ///
    /// ```no_run
    /// use concord::read::{RecordFail, Sse, SseEvent, Step};
    ///
    /// fn text(event: SseEvent) -> Result<Step<String>, RecordFail<()>> {
    ///     Ok(if event.data == "[DONE]" { Step::End } else { Step::Next(event.data) })
    /// }
    /// let reader = Sse::new(16 * concord::KIB).try_map(text).until_last_record();
    /// ```
    pub fn until_last_record<T, P>(
        self,
    ) -> impl Reader<RecordStream<T, concord_core::Error<P>>> + Clone + fmt::Debug
    where
        F: FnMut(SseEvent) -> Result<Step<T>, RecordFail<P>> + Clone + Send + Sync + 'static,
        T: Send + 'static,
        P: Send + 'static,
    {
        self.finish(End::LastRecord)
    }

    /// Finishes the reader: the end of the body ends the stream with `Ok(None)`. A
    /// [`Step::Last`] or a [`Step::End`] still ends the stream before the body does. For a
    /// stream that the server ends when it is done, such as a watch.
    ///
    /// ```no_run
    /// use concord::read::{RecordFail, Sse, SseEvent, Step};
    ///
    /// fn text(event: SseEvent) -> Result<Step<String>, RecordFail<()>> {
    ///     Ok(Step::Next(event.data))
    /// }
    /// let reader = Sse::new(16 * concord::KIB).try_map(text).until_end_of_body();
    /// ```
    pub fn until_end_of_body<T, P>(
        self,
    ) -> impl Reader<RecordStream<T, concord_core::Error<P>>> + Clone + fmt::Debug
    where
        F: FnMut(SseEvent) -> Result<Step<T>, RecordFail<P>> + Clone + Send + Sync + 'static,
        T: Send + 'static,
        P: Send + 'static,
    {
        self.finish(End::EndOfBody)
    }

    fn finish(self, end: End) -> Mapped<Sse, F> {
        Mapped {
            reader: self.reader,
            map: self.map,
            end,
        }
    }
}

#[cfg(feature = "json")]
impl<F> TryMap<JsonLines, F> {
    /// Finishes the reader: a record ends the stream ([`Step::Last`] or [`Step::End`]), and a
    /// body that ends cleanly before one did is the Mismatch
    /// `the body ended before its last record`.
    ///
    /// ```no_run
    /// use concord::read::{JsonLine, JsonLines, RecordFail, Step};
    /// fn id(line: JsonLine) -> Result<Step<u64>, RecordFail<()>> {
    ///     Ok(Step::Last(line.json()?))
    /// }
    /// let reader = JsonLines::new(4 * concord::KIB).try_map(id).until_last_record();
    /// ```
    pub fn until_last_record<T, P>(
        self,
    ) -> impl Reader<RecordStream<T, concord_core::Error<P>>> + Clone + fmt::Debug
    where
        F: FnMut(JsonLine) -> Result<Step<T>, RecordFail<P>> + Clone + Send + Sync + 'static,
        T: Send + 'static,
        P: Send + 'static,
    {
        self.finish(End::LastRecord)
    }

    /// Finishes the reader: the end of the body ends the stream with `Ok(None)`, as a watch
    /// ends. A [`Step::Last`] or a [`Step::End`] still ends the stream before the body does.
    ///
    /// ```no_run
    /// use concord::read::{JsonLine, JsonLines, RecordFail, Step};
    /// fn id(line: JsonLine) -> Result<Step<u64>, RecordFail<()>> {
    ///     Ok(Step::Next(line.json()?))
    /// }
    /// let reader = JsonLines::new(4 * concord::KIB).try_map(id).until_end_of_body();
    /// ```
    pub fn until_end_of_body<T, P>(
        self,
    ) -> impl Reader<RecordStream<T, concord_core::Error<P>>> + Clone + fmt::Debug
    where
        F: FnMut(JsonLine) -> Result<Step<T>, RecordFail<P>> + Clone + Send + Sync + 'static,
        T: Send + 'static,
        P: Send + 'static,
    {
        self.finish(End::EndOfBody)
    }

    fn finish(self, end: End) -> Mapped<JsonLines, F> {
        Mapped {
            reader: self.reader,
            map: self.map,
            end,
        }
    }
}

impl<T, P, F> Reader<RecordStream<T, concord_core::Error<P>>> for Mapped<Sse, F>
where
    T: Send + 'static,
    P: Send + 'static,
    F: FnMut(SseEvent) -> Result<Step<T>, RecordFail<P>> + Clone + Send + Sync + 'static,
{
    async fn read(
        &self,
        body: Incoming,
    ) -> Result<RecordStream<T, concord_core::Error<P>>, ReadError> {
        Ok(typed(
            body,
            self.reader.max_event_bytes,
            Events::new(),
            self.map.clone(),
            self.end,
        ))
    }
}

#[cfg(feature = "json")]
impl<T, P, F> Reader<RecordStream<T, concord_core::Error<P>>> for Mapped<JsonLines, F>
where
    T: Send + 'static,
    P: Send + 'static,
    F: FnMut(JsonLine) -> Result<Step<T>, RecordFail<P>> + Clone + Send + Sync + 'static,
{
    async fn read(
        &self,
        body: Incoming,
    ) -> Result<RecordStream<T, concord_core::Error<P>>, ReadError> {
        Ok(typed(
            body,
            self.reader.max_record_bytes,
            LineRecords,
            self.map.clone(),
            self.end,
        ))
    }
}

// How the lines of a body make records: what a line completes, and whether a record (a line,
// an event block) ends with it.
trait Framing: Send + 'static {
    type Record;
    const SSE: bool;
    fn line(&mut self, line: Vec<u8>) -> (Result<Option<Self::Record>, RecordError>, bool);
}

impl Framing for Events {
    type Record = SseEvent;
    const SSE: bool = true;
    fn line(&mut self, line: Vec<u8>) -> (Result<Option<SseEvent>, RecordError>, bool) {
        let ended = line.is_empty();
        (Events::line(self, &line), ended)
    }
}

#[cfg(feature = "json")]
struct LineRecords;

#[cfg(feature = "json")]
impl Framing for LineRecords {
    type Record = JsonLine;
    const SSE: bool = false;
    fn line(&mut self, line: Vec<u8>) -> (Result<Option<JsonLine>, RecordError>, bool) {
        let text = String::from_utf8(line).map_err(|error| RecordError::Utf8 {
            record: 0,
            source: error.utf8_error(),
        });
        (text.map(|text| Some(JsonLine { text })), true)
    }
}

fn typed<K, T, P, F>(
    body: Incoming,
    max: usize,
    framing: K,
    map: F,
    end: End,
) -> RecordStream<T, concord_core::Error<P>>
where
    K: Framing,
    T: Send + 'static,
    P: Send + 'static,
    F: FnMut(K::Record) -> Result<Step<T>, RecordFail<P>> + Send + 'static,
{
    let status = body.status();
    let headers = body.headers().clone();
    let mut input = body.stream().into_records::<P>();
    input.start_record();
    RecordStream {
        records: Some(Box::new(Typed {
            lines: Some(Lines::new(input, max, K::SSE)),
            framing,
            map,
            end,
        })),
        status,
        headers,
    }
}

// The pull of a mapped stream. The body is held in `lines` until the stream ends: a terminal
// record, an end, an error and the end of the body all drop it, before the pull returns.
struct Typed<K, P, F> {
    lines: Option<Lines<RecordBody<P>>>,
    framing: K,
    map: F,
    end: End,
}

// No field is pinned: the pull is driven through `&mut`.
impl<K, P, F> Unpin for Typed<K, P, F> {}

impl<K, P, F> Typed<K, P, F> {
    // A fault of framing, reported through the body at hand, which it consumes.
    fn fault(&mut self, error: RecordError) -> Option<concord_core::Error<P>> {
        let lines = self.lines.take()?;
        let body = lines.input;
        Some(match error {
            RecordError::Stream(error) => error.into(),
            RecordError::TooLarge { .. } => body.fail(RecordFault::too_large()),
            RecordError::Utf8 { .. } => body.fail(RecordFault::not_utf8()),
            RecordError::Decode { line, column, .. } => {
                body.fail(RecordFault::not_decoded().at(line, column))
            }
        })
    }
}

impl<K, T, P, F> Stream for Typed<K, P, F>
where
    K: Framing,
    F: FnMut(K::Record) -> Result<Step<T>, RecordFail<P>>,
{
    type Item = Result<T, concord_core::Error<P>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        // One budget for every record of this pull, skipped records included.
        let mut work = Work {
            bytes: 16_384,
            chunks: 64,
        };
        loop {
            let Some(lines) = this.lines.as_mut() else {
                return Poll::Ready(None);
            };
            let line = match lines.poll_line(cx, &mut work) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Ok(Some(line))) => line,
                Poll::Ready(Ok(None)) => {
                    let ended = this.lines.take().map(|lines| lines.input);
                    return match (this.end, ended) {
                        (End::LastRecord, Some(body)) => {
                            Poll::Ready(Some(Err(body.fail(RecordFault::ended_early()))))
                        }
                        _ => Poll::Ready(None),
                    };
                }
                Poll::Ready(Err(error)) => return Poll::Ready(this.fault(error).map(Err)),
            };
            let (record, ended) = this.framing.line(line);
            let record = match record {
                Ok(Some(record)) => record,
                Ok(None) => {
                    if ended {
                        lines.input.start_record();
                    }
                    continue;
                }
                Err(error) => return Poll::Ready(this.fault(error).map(Err)),
            };
            match (this.map)(record) {
                Ok(Step::Next(value)) => {
                    if ended {
                        lines.input.start_record();
                    }
                    return Poll::Ready(Some(Ok(value)));
                }
                Ok(Step::Skip) => {
                    if ended {
                        lines.input.start_record();
                    }
                }
                Ok(Step::Last(value)) => {
                    this.lines = None;
                    return Poll::Ready(Some(Ok(value)));
                }
                Ok(Step::End) => {
                    this.lines = None;
                    return Poll::Ready(None);
                }
                Err(fail) => {
                    let ended = this.lines.take().map(|lines| fail.into_error(lines.input));
                    return Poll::Ready(ended.map(Err));
                }
            }
        }
    }
}

impl<K, T, P, F> Pull<T, concord_core::Error<P>> for Typed<K, P, F>
where
    K: Framing,
    F: FnMut(K::Record) -> Result<Step<T>, RecordFail<P>>,
{
    fn cancel_wait(&mut self) {
        if let Some(lines) = &mut self.lines {
            lines.input.cancel_wait();
        }
    }
}
