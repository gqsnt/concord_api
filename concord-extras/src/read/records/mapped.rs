//! Records that go through a mapping of the author's own: a typed stream whose errors are the
//! client's own error.

#[cfg(feature = "json")]
use super::JsonLines;
use super::{RecordError, RecordStream, Sse, SseEvent};
use concord_core::read::{Incoming, ReadError, Reader};
use std::fmt;
use std::marker::PhantomData;

/// What a mapping does with one record: yield a value, skip the record, end the stream, or
/// end it with the API's own error.
///
/// A mapping is a function of your own that receives each record (an [`SseEvent`] for
/// [`Sse::map`], a value that `serde` decoded from the line for [`JsonLines::map`]) and returns
/// `Result<Step<T, P>, RecordError>`: `T` is the item of the stream, `P` the problem type of
/// the client. Its `Err` is a record that does not decode: `?` on
/// [`SseEvent::json`] returns it. The stream that the mapping makes is a
/// `RecordStream<T, concord::Error<P>>`, and each outcome reaches the caller of `try_next` as
/// follows:
///
/// | The mapping returns | `try_next` returns | And then |
/// |---|---|---|
/// | `Next(value)` | `Ok(Some(value))` | The next record is read. |
/// | `Skip` | nothing for this record | The next record is read, in the same call. |
/// | `Last(value)` | `Ok(Some(value))` | `Ok(None)`. The body is released at once: nothing after this record is read. |
/// | `End` | `Ok(None)` | `Ok(None)`. The body is released at once: nothing after this record is read. |
/// | `Problem(problem)` | `Err(error)`: Refused, with `problem()`, the status and the headers of the response | `Ok(None)`. The body is released. |
/// | `Err(record_error)` | `Err(error)`: a Mismatch, `record 3 could not be decoded` | `Ok(None)`. The body is released. |
///
/// The first error ends the stream, as for every stream of records. `Debug` prints the name of
/// the variant, never its value.
///
/// ```no_run
/// use concord::read::{RecordError, RecordStream, Sse, SseEvent, Step};
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
///     ContentBlockDelta { index: usize },
///     MessageStop,
///     #[serde(other)]
///     Unknown,
/// }
///
/// /// The event's name chooses what the event is.
/// pub fn event(event: SseEvent) -> Result<Step<StreamEvent, ApiProblem>, RecordError> {
///     Ok(match event.event.as_str() {
///         "ping" => Step::Skip,
///         "error" => Step::Problem(event.json()?),
///         "message_stop" => Step::Last(event.json()?),
///         _ => Step::Next(event.json()?),
///     })
/// }
///
/// concord::api! {
///     pub client Chat {
///         base "https://chat.example.test/v1";
///         problem ApiProblem read concord::Json;
///
///         POST stream() -> RecordStream<StreamEvent, concord::Error<ApiProblem>> {
///             path "messages";
///             read Sse::new(64 * concord::KIB).map(event).require_end();
///         }
///     }
/// }
///
/// async fn deltas(chat: &Chat) -> chat::Result<usize> {
///     let mut events = chat.stream().await?;        // the call's error
///     let mut deltas = 0;
///     while let Some(event) = events.try_next().await? {    // the same error type
///         if let StreamEvent::ContentBlockDelta { .. } = event {
///             deltas += 1;
///         }
///     }
///     Ok(deltas)                                    // `message_stop` was the last event
/// }
/// ```
#[non_exhaustive]
pub enum Step<T, P> {
    /// Yields this value; the stream goes on.
    Next(T),
    /// Yields this value, the last one: the stream ends after it, and nothing after this
    /// record is read.
    Last(T),
    /// Yields nothing for this record; the stream goes on.
    Skip,
    /// Ends the stream at this record, which yields nothing (a `[DONE]` that is not data).
    End,
    /// This record is the API's own error: the stream ends with a Refused error whose
    /// `problem()` is this value.
    Problem(P),
}

impl<T, P> fmt::Debug for Step<T, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Next(_) => "Next(..)",
            Self::Last(_) => "Last(..)",
            Self::Skip => "Skip",
            Self::End => "End",
            Self::Problem(_) => "Problem(..)",
        })
    }
}

/// A reader of records whose records go through a mapping of your own:
/// `read Sse::new(n).map(event);`, `read JsonLines::new(n).map(watch_event);`.
///
/// It reads as the reader that it maps does, with the same bound, and gives a
/// `RecordStream<T, concord::Error<P>>`: the items that the mapping yields, and the client's
/// own error, so that the call and the stream fail with one type. Each record is held to the
/// bound of the reader, and the mapping receives it whole; nothing is held beyond one record
/// and the value that the mapping returns. The mapping runs inside `try_next`, to its end:
/// dropping a `try_next` never leaves a record half mapped. A skipped record costs the same
/// work budget as a record that is yielded, so a stream of skipped records still yields to
/// the runtime. A panic of the mapping is not caught: it reaches the caller of `try_next`,
/// as a panic of an arm reaches the caller of the call.
///
/// The mapping is cloned for each response, so a mapping that keeps state (a closure that
/// counts, a builder) starts each stream afresh. It must own what it holds (`'static`): a
/// closure written in `read` can use none of the inputs of the call.
///
/// A body that ends cleanly ends the stream with `Ok(None)`, unless
/// [`require_end`](Mapped::require_end) says that a record must end it.
///
/// The errors that the stream returns, each once, before `Ok(None)`:
///
/// | When | Kind | Text |
/// |---|---|---|
/// | The mapping returns [`Step::Problem`] | Refused, with `problem()` | `refused: status 200, in chat.stream` |
/// | The mapping returns `Err`, or a line does not decode (`JsonLines`) | Mismatch | `mismatch: status 200, record 3 could not be decoded, in chat.stream` |
/// | A record exceeds the bound | Mismatch | `mismatch: status 200, record 3 exceeds the reader's size limit, in chat.stream` |
/// | A record is not UTF-8 | Mismatch | `mismatch: status 200, record 3 is not UTF-8, in chat.stream` |
/// | The body ends before a record ended the stream, under `require_end` | Mismatch | `mismatch: status 200, the body ended before its last record, in chat.stream` |
/// | The body breaks, or a timeout expires | Transport | the text of the core, as for a stream of bytes |
///
/// No text holds a value of the response. A Mismatch keeps no byte of the record and no
/// message of the decoder.
///
/// ```no_run
/// use concord::read::{JsonLines, Mapped, RecordError, RecordStream, Step};
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize)]
/// pub struct Status {
///     pub code: Option<u16>,
/// }
/// #[derive(Debug, Deserialize)]
/// pub struct Pod {
///     pub name: String,
/// }
/// /// `{"type": "ADDED", "object": {…}}`; an `ERROR` line holds a `Status`.
/// #[derive(Debug, Deserialize)]
/// #[serde(tag = "type", content = "object", rename_all = "UPPERCASE")]
/// pub enum WatchEvent<T> {
///     Added(T),
///     Deleted(T),
///     Error(Status),
/// }
///
/// /// An `ERROR` line is the API's own error; the server ends the watch after it.
/// pub fn watch_event<T>(event: WatchEvent<T>) -> Result<Step<WatchEvent<T>, Status>, RecordError> {
///     Ok(match event {
///         WatchEvent::Error(status) => Step::Problem(status),
///         event => Step::Next(event),
///     })
/// }
///
/// concord::api! {
///     pub client Cluster {
///         base "https://cluster.example.test/api/v1";
///         read concord::Json;
///         problem Status;
///
///         GET watch() -> RecordStream<WatchEvent<Pod>, concord::Error<Status>> {
///             path "pods";
///             query { "watch" = true; }
///             read JsonLines::new(64 * concord::KIB).map(watch_event);
///         }
///     }
/// }
///
/// async fn names(cluster: &Cluster) -> cluster::Result<Vec<String>> {
///     let mut events = cluster.watch().await?;
///     let mut names = Vec::new();
///     loop {
///         match events.try_next().await {
///             Ok(Some(WatchEvent::Added(pod))) => names.push(pod.name),
///             Ok(Some(_)) => {}
///             Ok(None) => return Ok(names),           // the server ended the watch
///             Err(error) if error.problem().and_then(|s| s.code) == Some(410) => return Ok(names),
///             Err(error) => return Err(error),
///         }
///     }
/// }
/// ```
pub struct Mapped<R, F, X> {
    reader: R,
    map: F,
    end_required: bool,
    record: PhantomData<fn(X)>,
}

impl<R, F, X> Mapped<R, F, X> {
    fn new(reader: R, map: F) -> Self {
        Self {
            reader,
            map,
            end_required: false,
            record: PhantomData,
        }
    }

    /// Requires that a record end the stream, with [`Step::Last`] or [`Step::End`]: a body
    /// that ends cleanly before one did is the Mismatch
    /// `the body ended before its last record`, not the end of the stream.
    ///
    /// An API whose stream ends with a record of its own (`message_stop`, `[DONE]`) uses it,
    /// so that a reply cut short at a record boundary is never taken for a whole one. Without
    /// it, the end of the body ends the stream, as for a reader that is not mapped.
    ///
    /// ```no_run
    /// use concord::read::{RecordError, Sse, SseEvent, Step};
    ///
    /// fn text(event: SseEvent) -> Result<Step<String, ()>, RecordError> {
    ///     Ok(if event.data == "[DONE]" { Step::End } else { Step::Next(event.data) })
    /// }
    /// let reader = Sse::new(16 * concord::KIB).map(text).require_end();
    /// ```
    #[must_use]
    pub fn require_end(mut self) -> Self {
        self.end_required = true;
        self
    }
}

impl<R: Clone, F: Clone, X> Clone for Mapped<R, F, X> {
    fn clone(&self) -> Self {
        Self {
            reader: self.reader.clone(),
            map: self.map.clone(),
            end_required: self.end_required,
            record: PhantomData,
        }
    }
}

impl<R: fmt::Debug, F, X> fmt::Debug for Mapped<R, F, X> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mapped")
            .field("reader", &self.reader)
            .finish_non_exhaustive()
    }
}

impl Sse {
    /// Gives each event to `map`, which says what the event is: a value of the stream, an
    /// event to skip, the last one, or the API's own error ([`Step`]).
    ///
    /// The stream is a `RecordStream<T, concord::Error<P>>`. The bound, the framing and the
    /// timeouts are the ones of this reader; [`Mapped`] says what the stream holds and returns.
    ///
    /// ```no_run
    /// use concord::read::{RecordError, RecordStream, Sse, SseEvent, Step};
    ///
    /// concord::api! {
    ///     client Feed {
    ///         base "https://feed.example.test";
    ///         GET lines() -> RecordStream<String, concord::Error> {
    ///             path "lines";
    ///             read Sse::new(16 * concord::KIB).map(|event: SseEvent| {
    ///                 Ok::<_, RecordError>(match event.event.as_str() {
    ///                     "keepalive" => Step::Skip,
    ///                     _ => Step::Next(event.data),
    ///                 })
    ///             });
    ///         }
    ///     }
    /// }
    /// ```
    pub fn map<T, P, F>(self, map: F) -> Mapped<Sse, F, SseEvent>
    where
        F: FnMut(SseEvent) -> Result<Step<T, P>, RecordError> + Clone + Send + Sync + 'static,
    {
        Mapped::new(self, map)
    }
}

#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
impl JsonLines {
    /// Decodes each line as `X` with `serde`, and gives the value to `map`, which says what
    /// the record is: a value of the stream, a record to skip, the last one, or the API's own
    /// error ([`Step`]).
    ///
    /// The stream is a `RecordStream<T, concord::Error<P>>`. A line that does not decode as
    /// `X` is the Mismatch `record 3 could not be decoded`, as one that `map` rejects; an empty
    /// line is one too. The bound and the framing are the ones of this reader; [`Mapped`] says
    /// what the stream holds and returns.
    ///
    /// ```no_run
    /// use concord::read::{JsonLines, RecordError, Step};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Line { id: u64, done: bool }
    ///
    /// fn line(line: Line) -> Result<Step<u64, ()>, RecordError> {
    ///     Ok(if line.done { Step::Last(line.id) } else { Step::Next(line.id) })
    /// }
    /// let reader = JsonLines::new(4 * concord::KIB).map(line).require_end();
    /// ```
    pub fn map<X, T, P, F>(self, map: F) -> Mapped<JsonLines, F, X>
    where
        X: serde::de::DeserializeOwned,
        F: FnMut(X) -> Result<Step<T, P>, RecordError> + Clone + Send + Sync + 'static,
    {
        Mapped::new(self, map)
    }
}

impl<T, P, F> Reader<RecordStream<T, concord_core::Error<P>>> for Mapped<Sse, F, SseEvent>
where
    T: Send + 'static,
    P: Send + 'static,
    F: FnMut(SseEvent) -> Result<Step<T, P>, RecordError> + Clone + Send + Sync + 'static,
{
    // PANIC-SAFETY: The surface is proposed; its body is written once the surface is accepted.
    #[allow(clippy::unimplemented)]
    async fn read(
        &self,
        body: Incoming,
    ) -> Result<RecordStream<T, concord_core::Error<P>>, ReadError> {
        let _ = (body, &self.reader, &self.map, self.end_required);
        unimplemented!()
    }
}

#[cfg(feature = "json")]
impl<X, T, P, F> Reader<RecordStream<T, concord_core::Error<P>>> for Mapped<JsonLines, F, X>
where
    X: serde::de::DeserializeOwned + 'static,
    T: Send + 'static,
    P: Send + 'static,
    F: FnMut(X) -> Result<Step<T, P>, RecordError> + Clone + Send + Sync + 'static,
{
    // PANIC-SAFETY: The surface is proposed; its body is written once the surface is accepted.
    #[allow(clippy::unimplemented)]
    async fn read(
        &self,
        body: Incoming,
    ) -> Result<RecordStream<T, concord_core::Error<P>>, ReadError> {
        let _ = (body, &self.reader, &self.map, self.end_required);
        unimplemented!()
    }
}
