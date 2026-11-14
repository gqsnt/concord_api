//! The one error of a client.

use crate::answer::Headers;
use crate::read::Place;
use crate::transport::{RequestFailure, TransportError};
use std::borrow::Cow;
use std::fmt;
use std::time::Duration;

/// A cause retained by the call; custom sources must themselves be safe to inspect.
pub(crate) type Source = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Copy)]
/// Where a description places an input whose serialized shape is unsuitable.
pub(crate) enum ValuePlace {
    Host,
    Path,
    Query,
    Header,
    Body,
}

/// Fixed build failures; string arguments are names from the description.
#[derive(Clone)]
pub(crate) enum BuildFailure {
    Http,
    UrlLength,
    Base,
    ReplacementBase,
    NextUrl,
    Host(&'static str),
    Path(&'static str),
    Shape {
        input: &'static str,
        slot: ValuePlace,
    },
    Nonfinite(&'static str),
    Header(&'static str),
    Description(&'static str),
    TooManyHeaders,
    Body,
    BodyDepth(String),
    Entry(&'static str),
    InspectStream,
    SpentStream,
    Origin,
    ContentLength,
}

#[derive(Clone)]
/// Fixed answer failures; a decoder location contains schema names and indexes only.
pub(crate) enum MismatchFailure {
    ContentType,
    BodyLimit,
    Decode {
        place: Place,
        unexpected_label: bool,
    },
    Empty {
        unexpected_label: bool,
    },
    NoArm,
    NextPage,
    Arm,
}

#[derive(Clone, Copy)]
/// Which clock owned by Concord ended the exchange.
pub(crate) enum Clock {
    Idle,
    DescriptionIdle,
    Total,
}

#[derive(Clone, Copy)]
enum Connection {
    Refused,
    Reset,
    Dns,
    Other,
}

enum Note {
    Build(BuildFailure),
    Transport(Class, Connection),
    Request(RequestFailure),
    Clock(Clock, Duration),
    ConnectTimeout(Option<Duration>),
    Refused,
    Mismatch(MismatchFailure),
}

impl Note {
    // The single vocabulary for every line, including structured reader causes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>, status: Option<http::StatusCode>) -> fmt::Result {
        match self {
            Self::Build(failure) => match failure {
                BuildFailure::Http => f.write_str("the HTTP client could not be built"),
                BuildFailure::UrlLength => f.write_str("the URL is too long to be sent"),
                BuildFailure::NextUrl => f.write_str("the next URL is not a URL"),
                BuildFailure::Base => f.write_str("the base is not a URL that can be used"),
                BuildFailure::ReplacementBase => {
                    f.write_str("the base is not a URL that can be used (with_base)")
                }
                BuildFailure::Host(input) => write!(f, "input {input} is not one label of a host"),
                BuildFailure::Path(input) => {
                    write!(f, "input {input} gives an empty or dot path segment")
                }
                BuildFailure::Shape { input, slot } => {
                    let slot = match slot {
                        ValuePlace::Host => "a host",
                        ValuePlace::Path => "a path",
                        ValuePlace::Query => "a query",
                        ValuePlace::Header => "a header",
                        ValuePlace::Body => "a body",
                    };
                    write!(f, "input {input} cannot be written in {slot}")
                }
                BuildFailure::Nonfinite(input) => write!(f, "input {input} is not a finite number"),
                BuildFailure::Header(name) => {
                    write!(f, "the value of header {name} is not a legal header value")
                }
                BuildFailure::Description(what) => {
                    write!(f, "the description is not valid ({what})")
                }
                BuildFailure::TooManyHeaders => f.write_str("the request has too many headers"),
                BuildFailure::BodyDepth(input) => {
                    write!(f, "input {input} cannot be written in a body")
                }
                BuildFailure::Body => f.write_str("the body could not be written"),
                BuildFailure::Entry(input) => write!(f, "input {input} could not be written"),
                BuildFailure::InspectStream => {
                    f.write_str("the body is a stream; into_request takes it")
                }
                BuildFailure::SpentStream => {
                    f.write_str("the body is a stream that was already sent")
                }
                BuildFailure::Origin => f.write_str("the URL is at another origin than the call"),
                BuildFailure::ContentLength => {
                    f.write_str("the content-length is not the length of the body")
                }
            },
            Self::Transport(Class::Connect, connection) => f.write_str(match connection {
                Connection::Refused => "connection refused",
                Connection::Reset => "connection reset",
                Connection::Dns => "name not found",
                Connection::Other => "no connection",
            }),
            Self::Transport(Class::Timeout, _) => {
                f.write_str("a timeout was reported by the transport")
            }
            Self::Transport(Class::Body, _) => {
                f.write_str("the body broke")?;
                if let Some(status) = status {
                    write!(f, " after status {}", status.as_u16())?;
                }
                Ok(())
            }
            Self::Transport(Class::Blocked, _) => f.write_str("blocked before it was sent"),
            Self::Transport(Class::Other, _) => f.write_str("the transport failed"),
            Self::Request(RequestFailure::Head) => {
                f.write_str("the head of the answer could not be read")
            }
            Self::Request(RequestFailure::Body) => f.write_str("the body of the request failed"),
            Self::ConnectTimeout(duration) => {
                f.write_str("no connection")?;
                if let Some(duration) = duration {
                    f.write_str(" after ")?;
                    seconds(f, *duration)?;
                    f.write_str(" s")?;
                }
                f.write_str(" (the connect timeout of the HTTP client)")
            }
            Self::Clock(clock, duration) => {
                match clock {
                    Clock::Idle | Clock::DescriptionIdle if status.is_some() => {
                        f.write_str("the body was silent for ")?;
                    }
                    Clock::Idle | Clock::DescriptionIdle => f.write_str("no answer for ")?,
                    Clock::Total => f.write_str("the call took longer than ")?,
                }
                seconds(f, *duration)?;
                f.write_str(" s")?;
                if matches!(clock, Clock::Idle | Clock::DescriptionIdle) {
                    if let Some(status) = status {
                        write!(f, " after status {}", status.as_u16())?;
                    }
                }
                f.write_str(match clock {
                    Clock::Idle => " (with_idle_timeout changes it)",
                    Clock::DescriptionIdle => " (with_idle_timeout on the call changes it)",
                    Clock::Total => " (with_timeout)",
                })
            }
            Self::Refused => Ok(()),
            Self::Mismatch(failure) => match failure {
                MismatchFailure::ContentType => {
                    f.write_str("the content type is not the one the reader expects")
                }
                MismatchFailure::BodyLimit => {
                    f.write_str("the body is larger than the limit of the reader")
                }
                MismatchFailure::Empty { unexpected_label } => {
                    f.write_str("the body is empty")?;
                    if *unexpected_label {
                        f.write_str(" and its content type is not the reader's")?;
                    }
                    Ok(())
                }
                MismatchFailure::Decode {
                    place,
                    unexpected_label,
                } => {
                    let root = place.to_string().is_empty();
                    if root {
                        f.write_str("the body")?;
                    } else {
                        write!(f, "{place}")?;
                    }
                    f.write_str(" is not what the type says")?;
                    if *unexpected_label {
                        f.write_str(" and its content type is not the reader's")?;
                    }
                    Ok(())
                }
                MismatchFailure::NoArm => f.write_str("no arm takes this status"),
                MismatchFailure::NextPage => f.write_str("the next page cannot be asked"),
                MismatchFailure::Arm => f.write_str("the status arm refused the answer"),
            },
        }
    }
}

fn seconds(f: &mut fmt::Formatter<'_>, duration: Duration) -> fmt::Result {
    write!(f, "{}", duration.as_secs())?;
    if duration.subsec_nanos() != 0 {
        let fraction = format!("{:09}", duration.subsec_nanos());
        write!(f, ".{}", fraction.trim_end_matches('0'))?;
    }
    Ok(())
}

struct ReaderCause(Note);

impl fmt::Display for ReaderCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f, None)
    }
}
impl fmt::Debug for ReaderCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for ReaderCause {}

fn connection(source: Option<&(dyn std::error::Error + 'static)>) -> Connection {
    let mut source = source;
    let mut dns = false;
    while let Some(cause) = source {
        if let Some(error) = cause.downcast_ref::<reqwest::Error>() {
            dns |= error.is_dns();
        }
        if let Some(error) = cause.downcast_ref::<std::io::Error>() {
            use std::io::ErrorKind::*;
            match error.kind() {
                ConnectionRefused => return Connection::Refused,
                ConnectionReset | ConnectionAborted | BrokenPipe | NotConnected | UnexpectedEof => {
                    return Connection::Reset
                }
                _ => {}
            }
        }
        source = cause.source();
    }
    if dns {
        Connection::Dns
    } else {
        Connection::Other
    }
}

/// What a call gives when it does not give its value.
///
/// `E` is the API's own problem type, chosen by the client's author.
/// Printing an error (`{e}`) shows names only: its kind, the status, the endpoint, and a
/// cause in fixed words. Never a URL, a header value, a body or a secret.
/// [`details`](Error::details) prints the values, when you ask for them.
pub struct Error<E = ()> {
    inner: Box<Inner<E>>,
}

struct Inner<E> {
    kind: Kind,
    endpoint: &'static str,
    problem: Option<E>,
    status: Option<http::StatusCode>,
    headers: Option<Headers>,
    body: Option<Box<[u8]>>,
    kept: Option<Kept>,
    retry_after: Option<Duration>,
    tries: u32,
    note: Note,
    source: Option<Source>,
    reason: Option<Cow<'static, str>>,
    reader_details: Option<String>,
    from_byte_stream: bool,
}

/// What went wrong, in four cases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Kind {
    /// The request could not be built.
    Build,
    /// No answer, or an answer that broke before the end of a body that was wanted.
    Transport,
    /// The API said no: a status that the endpoint gives no meaning to, or the API's own problem.
    Refused,
    /// The answer is not what the client promised: it does not decode, it is too large, or the client's status handling refused it.
    Mismatch,
}

/// Which failure of the network.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Class {
    /// A connection failed on any hop.
    Connect,
    /// A clock ended: `idle_timeout`, `timeout`, or one that the transport reports.
    Timeout,
    /// The body broke, or ended before its announced size.
    Body,
    /// A middleware or transport refused before sending.
    Blocked,
    /// Anything else.
    Other,
}

/// The state of the body that an error keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Kept {
    /// The whole body.
    Complete,
    /// The first 64 KiB of a longer body.
    Cut,
    /// What had arrived when the body broke.
    Interrupted,
}

// Used by the following engine packages; names must be program names, never values.
impl<E> Error<E> {
    fn new(kind: Kind, endpoint: &'static str, tries: u32, note: Note) -> Self {
        Self {
            inner: Box::new(Inner {
                kind,
                endpoint,
                tries,
                note,
                problem: None,
                status: None,
                headers: None,
                body: None,
                kept: None,
                retry_after: None,
                source: None,
                reason: None,
                reader_details: None,
                from_byte_stream: false,
            }),
        }
    }

    /// A build failure with names from the description only; no attempt was entered.
    pub(crate) fn build(endpoint: &'static str, failure: BuildFailure) -> Self {
        Self::new(Kind::Build, endpoint, 0, Note::Build(failure))
    }

    /// The shared default HTTP client could not be constructed.
    pub(crate) fn http(endpoint: &'static str, source: Source) -> Self {
        let mut error = Self::build(endpoint, BuildFailure::Http);
        error.inner.source = Some(source);
        error
    }

    /// A body expression failed. Its free text is confined to the source.
    pub(crate) fn build_source(endpoint: &'static str, mut source: Source) -> Self {
        let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&*source);
        let mut failure = BuildFailure::Body;
        for _ in 0..128 {
            let Some(current) = cause else {
                break;
            };
            if let Some(input) = current
                .downcast_ref::<crate::body::WriteError>()
                .and_then(crate::body::WriteError::depth_input)
            {
                failure = BuildFailure::BodyDepth(input.to_owned());
                break;
            }
            cause = current.source();
        }
        let source = loop {
            match source.downcast::<crate::body::WriteError>() {
                Ok(writer) => source = writer.into_source(),
                Err(format) => break format,
            }
        };
        let mut error = Self::build(endpoint, failure);
        error.inner.source = Some(source);
        error
    }

    /// An entry expression failed; its program name is printed and its cause retained.
    pub(crate) fn entry_source(
        endpoint: &'static str,
        input: &'static str,
        source: Source,
    ) -> Self {
        let mut error = Self::build(endpoint, BuildFailure::Entry(input));
        error.inner.source = Some(source);
        error
    }

    /// A failure reported by a middleware or transport, with its original safe source.
    pub(crate) fn transport(endpoint: &'static str, tries: u32, failure: TransportError) -> Self {
        if let Some(timeout) = failure.clock_timeout() {
            let (clock, duration) = match timeout {
                crate::answer::BodyTimeout::Idle(duration) => (Clock::Idle, duration),
                crate::answer::BodyTimeout::IdleFromDescription(duration) => {
                    (Clock::DescriptionIdle, duration)
                }
                crate::answer::BodyTimeout::Total(duration) => (Clock::Total, duration),
            };
            return Self::clock(endpoint, tries, clock, duration);
        }
        let connect_timeout = failure.connect_timeout();
        let connection_kind = failure.connection_kind();
        let request_failure = failure.request_failure();
        let (class, source) = failure.into_parts();
        let connection = match connection_kind {
            Some(std::io::ErrorKind::ConnectionRefused) => Connection::Refused,
            Some(_) => Connection::Reset,
            None => connection(source.as_deref().map(|source| source as _)),
        };
        let mut error = Self::new(
            Kind::Transport,
            endpoint,
            tries,
            match connect_timeout {
                Some(duration) => Note::ConnectTimeout(duration),
                None => match request_failure {
                    Some(failure) => Note::Request(failure),
                    None => Note::Transport(class, connection),
                },
            },
        );
        error.inner.source = source;
        error
    }

    /// A failure while reading a wanted body, retaining the answer's head.
    /// the known head makes this Body regardless of the source's I/O kind.
    /// Concord's own body clocks use `clock` instead.
    pub(crate) fn transport_after_head(
        endpoint: &'static str,
        tries: u32,
        status: http::StatusCode,
        headers: Headers,
        failure: TransportError,
    ) -> Self {
        let (_, source) = failure.into_parts();
        let mut error = Self::new(
            Kind::Transport,
            endpoint,
            tries,
            Note::Transport(Class::Body, Connection::Other),
        )
        .with_response(status, headers);
        error.inner.source = source;
        error
    }

    /// One of Concord's own clocks expired, rather than a transport-reported timeout.
    pub(crate) fn clock(
        endpoint: &'static str,
        tries: u32,
        clock: Clock,
        duration: Duration,
    ) -> Self {
        Self::new(
            Kind::Transport,
            endpoint,
            tries,
            Note::Clock(clock, duration),
        )
    }

    /// A refused answer, optionally decoded as the API's own problem.
    pub(crate) fn refused(
        endpoint: &'static str,
        tries: u32,
        status: http::StatusCode,
        headers: Headers,
        problem: Option<E>,
    ) -> Self {
        let mut error =
            Self::new(Kind::Refused, endpoint, tries, Note::Refused).with_response(status, headers);
        error.inner.problem = problem;
        error
    }

    /// A mismatch, described only by structured program names and fixed failure kinds.
    pub(crate) fn mismatch(
        endpoint: &'static str,
        tries: u32,
        status: http::StatusCode,
        headers: Headers,
        failure: MismatchFailure,
    ) -> Self {
        let source = match &failure {
            MismatchFailure::ContentType
            | MismatchFailure::BodyLimit
            | MismatchFailure::Empty { .. }
            | MismatchFailure::Decode { .. } => {
                Some(Box::new(ReaderCause(Note::Mismatch(failure.clone()))) as Source)
            }
            _ => None,
        };
        let mut error = Self::new(Kind::Mismatch, endpoint, tries, Note::Mismatch(failure))
            .with_response(status, headers);
        error.inner.source = source;
        error
    }

    /// Retain a head, including for a broken body or a clock that expired after the head.
    pub(crate) fn with_response(mut self, status: http::StatusCode, headers: Headers) -> Self {
        self.inner.status = Some(status);
        self.inner.headers = Some(headers);
        self
    }

    /// Copy bounded evidence into its own allocation. An interrupted body stays interrupted.
    pub(crate) fn with_body(mut self, body: &[u8], kept: Kept) -> Self {
        let prefix = body
            // PANIC-SAFETY: These constant byte limits fit usize on supported 32/64-bit targets.
            .get(..body.len().min(64 * crate::KIB))
            .unwrap_or_default();
        self.inner.body = Some(prefix.into());
        // PANIC-SAFETY: These constant byte limits fit usize on supported 32/64-bit targets.
        self.inner.kept = Some(if body.len() > 64 * crate::KIB && kept == Kept::Complete {
            Kept::Cut
        } else {
            kept
        });
        self
    }

    /// Record the already-evaluated waits. No header or retry rule is asked here.
    pub(crate) fn with_retry_after(
        mut self,
        header: Option<Duration>,
        rule: Option<Duration>,
    ) -> Self {
        self.inner.retry_after = header.max(rule);
        self
    }

    /// Reader free text is copied into a bounded allocation and is only printed by details.
    pub(crate) fn with_reader_details(mut self, details: String) -> Self {
        self.inner.reader_details = Some(crate::__private::cut_details(details));
        self
    }
}

impl<E> Error<E> {
    /// The kind of the error.
    pub fn kind(&self) -> Kind {
        self.inner.kind
    }

    /// The name of the endpoint: `riot.matches.get`.
    pub fn endpoint(&self) -> &'static str {
        self.inner.endpoint
    }

    /// The status of the answer, when one arrived.
    pub fn status(&self) -> Option<http::StatusCode> {
        self.inner.status
    }

    /// The answer had this status: `e.is_status(404)`.
    pub fn is_status(&self, status: u16) -> bool {
        self.status().is_some_and(|s| s == status)
    }

    /// The headers of the answer, when one arrived.
    pub fn headers(&self) -> Option<&Headers> {
        self.inner.headers.as_ref()
    }

    /// The longer of the answer’s `Retry-After` and the wait requested by the first
    /// `when` rule that gave one for the try that did not happen. Neither rule is asked again.
    pub fn retry_after(&self) -> Option<std::time::Duration> {
        self.inner.retry_after
    }

    /// The start of the body, when one was kept: at most 64 KiB, in a buffer of its own.
    pub fn body(&self) -> Option<&[u8]> {
        self.inner.body.as_deref()
    }

    /// The state of [`body`](Error::body), when one was kept.
    pub fn kept(&self) -> Option<Kept> {
        self.inner.kept
    }

    /// The API's own problem, typed, when the API refused and its body was one.
    pub fn problem(&self) -> Option<&E> {
        self.inner.problem.as_ref()
    }

    /// The API's own problem, taken; or the error back, when it holds none.
    pub fn into_problem(mut self) -> Result<E, Error<E>> {
        match self.inner.problem.take() {
            Some(problem) => Ok(problem),
            None => Err(self),
        }
    }

    /// The same error, with its problem changed by a function.
    pub fn map_problem<F>(self, map: impl FnOnce(E) -> F) -> Error<F> {
        self.map_optional_problem(|problem| problem.map(map))
    }

    /// Change a problem-free stream error's type without changing its note or cause.
    pub(crate) fn without_problem<F>(self) -> Error<F> {
        self.map_optional_problem(|_| None)
    }

    fn map_optional_problem<F>(self, map: impl FnOnce(Option<E>) -> Option<F>) -> Error<F> {
        let Inner {
            kind,
            endpoint,
            problem,
            status,
            headers,
            body,
            kept,
            retry_after,
            tries,
            note,
            source,
            reason,
            reader_details,
            from_byte_stream,
        } = *self.inner;
        Error {
            inner: Box::new(Inner {
                kind,
                endpoint,
                problem: map(problem),
                status,
                headers,
                body,
                kept,
                retry_after,
                tries,
                note,
                source,
                reason,
                reader_details,
                from_byte_stream,
            }),
        }
    }

    pub(crate) fn mark_byte_stream(mut self) -> Self {
        self.inner.from_byte_stream = true;
        self
    }

    pub(crate) fn is_from_byte_stream(&self) -> bool {
        self.inner.from_byte_stream
    }

    /// How many attempts entered the middleware chain. A middleware may send more
    /// than once within one attempt. A response read separately reports zero.
    pub fn tries(&self) -> u32 {
        self.inner.tries
    }

    /// Which failure of the network, for a Transport error.
    pub fn class(&self) -> Option<Class> {
        match self.inner.note {
            Note::Transport(class, _) => Some(class),
            Note::Request(_) => Some(Class::Other),
            Note::Clock(_, _) => Some(Class::Timeout),
            Note::ConnectTimeout(_) => Some(Class::Timeout),
            _ => None,
        }
    }

    /// A clock ended.
    pub fn is_timeout(&self) -> bool {
        self.class() == Some(Class::Timeout)
    }

    /// A reason supplied by the author, for example through `mismatch`.
    /// A failed body read gives `None`; decoder messages are in `details()`.
    pub fn reason(&self) -> Option<&str> {
        self.inner.reason.as_deref()
    }

    /// Everything the error holds, values included, as one text to print by your own choice:
    /// the line of `{e}`, the reason, the headers, the problem or the start of the body,
    /// and at most 1 KiB of a reader’s decoder message, followed by a cut marker if needed.
    pub fn details(&self) -> Details<'_, E>
    where
        E: fmt::Debug,
    {
        Details { error: self }
    }
}

impl<E> fmt::Display for Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.kind() {
            Kind::Build => "build error: ",
            Kind::Transport => "transport error: ",
            Kind::Refused => "refused: ",
            Kind::Mismatch => "mismatch: ",
        })?;
        if matches!(self.kind(), Kind::Refused | Kind::Mismatch) {
            if let Some(status) = self.status() {
                write!(f, "status {}", status.as_u16())?;
                if self.kind() == Kind::Mismatch {
                    f.write_str(", ")?;
                }
            }
        }
        self.inner.note.fmt(f, self.status())?;
        write!(f, ", in {}", self.endpoint())?;
        if self.tries() > 1 {
            write!(f, ", after {} tries", self.tries())?;
        }
        Ok(())
    }
}

impl<E> fmt::Debug for Error<E> {
    /// The same names as `Display`, as a struct. No value, so it needs nothing of `E`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("kind", &self.inner.kind)
            .field("status", &self.status().map(|status| status.as_u16()))
            .field("class", &self.class())
            .field("message", &format_args!("{self}"))
            .finish_non_exhaustive()
    }
}

impl<E> std::error::Error for Error<E> {
    /// For a failure of the network: the error of the HTTP library, with its URL taken out,
    /// or the source that a transport of yours gave. For a body that could not be
    /// written: the writer's error. For a reader's Mismatch: a small, value-free cause
    /// constructed by the core from the structured place and failure kind, never
    /// supplied by the reader. Decoder messages that may quote the answer belong
    /// in `details()`, bounded to 1 KiB.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.source.as_deref().map(|source| source as _)
    }
}

/// What [`Error::details`] gives. It prints values.
pub struct Details<'a, E> {
    error: &'a Error<E>,
}

impl<E> fmt::Debug for Details<'_, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Details").finish_non_exhaustive()
    }
}

impl<E: fmt::Debug> fmt::Display for Details<'_, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let error = self.error;
        write!(f, "{error}")?;
        if let Some(reason) = error.reason() {
            write!(f, "\nreason: {reason}")?;
        }
        if let Some(headers) = error.headers() {
            f.write_str("\nheaders:")?;
            for (name, value) in headers.as_map() {
                write!(
                    f,
                    "\n  {name}: {}",
                    String::from_utf8_lossy(value.as_bytes())
                )?;
            }
        }
        if let Some(problem) = error.problem() {
            write!(f, "\nproblem: {problem:?}")?;
        } else if let Some(body) = error.body() {
            f.write_str("\nbody (")?;
            match error.kept() {
                Some(Kept::Complete) => write!(f, "complete, {} bytes", body.len())?,
                Some(Kept::Cut) => f.write_str("cut at 64 KiB")?,
                Some(Kept::Interrupted) => write!(f, "interrupted, {} bytes", body.len())?,
                None => write!(f, "{} bytes", body.len())?,
            }
            write!(f, "): {}", String::from_utf8_lossy(body))?;
        }
        if let Some(details) = &error.inner.reader_details {
            write!(f, "\nreader: {details}")?;
        }
        let mut source = std::error::Error::source(error);
        while let Some(cause) = source {
            write!(f, "\ncaused by: {cause}")?;
            source = cause.source();
        }
        Ok(())
    }
}

impl<E: Send + Sync + 'static> From<Error<E>> for std::io::Error {
    /// So that a [`ByteStream`](crate::ByteStream) can feed a reader that wants `io::Error`.
    fn from(error: Error<E>) -> std::io::Error {
        std::io::Error::other(error)
    }
}

/// How an arm or a pager fails.
///
/// `?` on a `Result<_, P>` makes one from the API's own problem: the call ends as Refused.
/// [`mismatch`] makes one that says "this answer is not what was promised".
///
/// `Fail::from(problem)` in a pager becomes a Refused error with that problem;
/// `concord::mismatch(…)` becomes a Mismatch. Both are returned by the `try_next()`
/// after the page that arrived.
pub struct Fail<P> {
    cause: Cause<P>,
}

enum Cause<P> {
    Problem(P),
    Mismatch(Cow<'static, str>),
}

impl<P> From<P> for Fail<P> {
    fn from(problem: P) -> Fail<P> {
        Fail {
            cause: Cause::Problem(problem),
        }
    }
}

/// "This answer is not what was promised." The text becomes the `reason()` of the error.
pub fn mismatch<P>(reason: impl Into<std::borrow::Cow<'static, str>>) -> Fail<P> {
    Fail {
        cause: Cause::Mismatch(reason.into()),
    }
}

impl<P> fmt::Debug for Fail<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.cause {
            Cause::Problem(_) => f.write_str("Fail(problem)"),
            Cause::Mismatch(_) => f.write_str("Fail(mismatch)"),
        }
    }
}

// Called by the arm and pager halves of the engine.
impl<P> Fail<P> {
    /// Preserve typed problems and keep custom mismatch reasons out of printed names.
    pub(crate) fn into_error(
        self,
        endpoint: &'static str,
        tries: u32,
        status: http::StatusCode,
        headers: Headers,
        next_page: bool,
    ) -> Error<P> {
        match self.cause {
            Cause::Problem(problem) => {
                Error::refused(endpoint, tries, status, headers, Some(problem))
            }
            Cause::Mismatch(reason) => {
                let failure = if next_page {
                    MismatchFailure::NextPage
                } else {
                    MismatchFailure::Arm
                };
                let mut error = Error::mismatch(endpoint, tries, status, headers, failure);
                error.inner.reason = Some(reason);
                error
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;
    use std::error::Error as _;

    fn build(failure: BuildFailure) -> Error {
        Error::build("tasks.get", failure)
    }

    fn mismatch_at(endpoint: &'static str, failure: MismatchFailure) -> Error {
        Error::mismatch(endpoint, 1, StatusCode::OK, Headers::default(), failure)
    }

    fn header_values() -> Headers {
        let mut headers = http::HeaderMap::new();
        let mut value = http::HeaderValue::from_static("PRIVATE_SENTINEL");
        value.set_sensitive(true);
        headers.insert("x-request-id", value);
        headers.into()
    }

    #[test]
    fn c6_2_text_01_invalid_base() {
        assert_eq!(
            build(BuildFailure::Base).to_string(),
            "build error: the base is not a URL that can be used, in tasks.get"
        );
    }

    #[test]
    fn c6_2_text_02_host_input() {
        assert_eq!(
            build(BuildFailure::Host("region")).to_string(),
            "build error: input region is not one label of a host, in tasks.get"
        );
    }

    #[test]
    fn c6_2_text_03_path_input() {
        assert_eq!(
            build(BuildFailure::Path("id")).to_string(),
            "build error: input id gives an empty or dot path segment, in tasks.get"
        );
    }

    #[test]
    fn c6_2_text_04_query_shape() {
        assert_eq!(
            build(BuildFailure::Shape {
                input: "filter",
                slot: ValuePlace::Query
            })
            .to_string(),
            "build error: input filter cannot be written in a query, in tasks.get"
        );
    }

    #[test]
    fn c6_2_text_05_nonfinite() {
        assert_eq!(
            build(BuildFailure::Nonfinite("ratio")).to_string(),
            "build error: input ratio is not a finite number, in tasks.get"
        );
    }

    #[test]
    fn c6_2_text_06_header_value() {
        assert_eq!(
            build(BuildFailure::Header("x-note")).to_string(),
            "build error: the value of header x-note is not a legal header value, in tasks.get"
        );
    }

    #[test]
    fn c6_2_text_07_body_write() {
        let error: Error = Error::build("tasks.create", BuildFailure::Body);
        assert_eq!(
            error.to_string(),
            "build error: the body could not be written, in tasks.create"
        );
    }

    #[test]
    fn c6_2_text_08_inspect_stream() {
        let error: Error = Error::build("files.upload", BuildFailure::InspectStream);
        assert_eq!(
            error.to_string(),
            "build error: the body is a stream; into_request takes it, in files.upload"
        );
    }

    #[test]
    fn c6_2_text_09_spent_stream() {
        let error: Error = Error::build("files.upload", BuildFailure::SpentStream);
        assert_eq!(
            error.to_string(),
            "build error: the body is a stream that was already sent, in files.upload"
        );
    }

    #[test]
    fn c6_2_text_10_blocked() {
        let error: Error = Error::transport("vault.balance", 1, TransportError::blocked());
        assert_eq!(
            error.to_string(),
            "transport error: blocked before it was sent, in vault.balance"
        );
    }

    #[test]
    fn c6_2_text_11_refused() {
        let error: Error = Error::refused(
            "tasks.get",
            1,
            StatusCode::NOT_FOUND,
            Headers::default(),
            None,
        );
        assert_eq!(error.to_string(), "refused: status 404, in tasks.get");
    }

    #[test]
    fn c6_2_text_12_refused_after_tries() {
        let error: Error = Error::refused(
            "riot.matches.ids",
            3,
            StatusCode::TOO_MANY_REQUESTS,
            Headers::default(),
            None,
        );
        assert_eq!(
            error.to_string(),
            "refused: status 429, in riot.matches.ids, after 3 tries"
        );
    }

    #[test]
    fn c6_2_text_13_content_type() {
        assert_eq!(mismatch_at("tasks.get", MismatchFailure::ContentType).to_string(), "mismatch: status 200, the content type is not the one the reader expects, in tasks.get");
    }

    #[test]
    fn c6_2_text_14_body_limit() {
        assert_eq!(
            mismatch_at("tasks.get", MismatchFailure::BodyLimit).to_string(),
            "mismatch: status 200, the body is larger than the limit of the reader, in tasks.get"
        );
    }

    #[test]
    fn c6_2_text_15_decode_place() {
        let error = mismatch_at(
            "tasks.list",
            MismatchFailure::Decode {
                place: Place::root().field("items").index(3).field("kind"),
                unexpected_label: false,
            },
        );
        assert_eq!(
            error.to_string(),
            "mismatch: status 200, items[3].kind is not what the type says, in tasks.list"
        );
    }

    #[test]
    fn c6_2_root_decode_with_label_uses_the_decided_text() {
        let error = mismatch_at(
            "tasks.list",
            MismatchFailure::Decode {
                place: Place::root(),
                unexpected_label: true,
            },
        );
        assert_eq!(
            error.to_string(),
            "mismatch: status 200, the body is not what the type says and its content type is not the reader's, in tasks.list"
        );
    }

    #[test]
    fn c6_2_text_17_head_no_arm() {
        let error: Error = Error::mismatch(
            "tasks.exists",
            1,
            StatusCode::NO_CONTENT,
            Headers::default(),
            MismatchFailure::NoArm,
        );
        assert_eq!(
            error.to_string(),
            "mismatch: status 204, no arm takes this status, in tasks.exists"
        );
    }

    #[test]
    fn c6_2_text_18_broken_body() {
        let error: Error = Error::transport_after_head(
            "files.download",
            1,
            StatusCode::OK,
            Headers::default(),
            TransportError::new(Class::Body),
        );
        assert_eq!(
            error.to_string(),
            "transport error: the body broke after status 200, in files.download"
        );
    }

    #[test]
    fn c6_2_text_19_next_page() {
        let error: Error = super::mismatch("PRIVATE_SENTINEL").into_error(
            "tasks.list",
            1,
            StatusCode::OK,
            Headers::default(),
            true,
        );
        assert_eq!(
            error.to_string(),
            "mismatch: status 200, the next page cannot be asked, in tasks.list"
        );
    }

    #[test]
    fn c6_2_text_20_pager_problem() {
        let error = Fail::from("PRIVATE_SENTINEL").into_error(
            "tasks.list",
            1,
            StatusCode::OK,
            Headers::default(),
            true,
        );
        assert_eq!(error.to_string(), "refused: status 200, in tasks.list");
    }

    #[test]
    fn c6_2_text_21_origin() {
        let error: Error = Error::build("tasks.list", BuildFailure::Origin);
        assert_eq!(
            error.to_string(),
            "build error: the URL is at another origin than the call, in tasks.list"
        );
    }

    #[test]
    fn c6_2_text_22_connection_refused() {
        let error: Error = Error::transport(
            "tasks.get",
            1,
            TransportError::connect()
                .with_source(std::io::Error::from(std::io::ErrorKind::ConnectionRefused)),
        );
        assert_eq!(
            error.to_string(),
            "transport error: connection refused, in tasks.get"
        );
    }

    #[test]
    fn c6_2_text_23_idle_clock() {
        let error: Error = Error::clock("tasks.get", 1, Clock::Idle, Duration::from_secs(30));
        assert_eq!(
            error.to_string(),
            "transport error: no answer for 30 s (with_idle_timeout changes it), in tasks.get"
        );
    }

    #[test]
    fn c6_2_text_24_total_clock() {
        let error: Error = Error::clock("tasks.get", 1, Clock::Total, Duration::from_secs(60));
        assert_eq!(
            error.to_string(),
            "transport error: the call took longer than 60 s (with_timeout), in tasks.get"
        );
    }

    #[test]
    fn c6_2_text_25_content_length() {
        let error: Error = Error::build("files.upload", BuildFailure::ContentLength);
        assert_eq!(
            error.to_string(),
            "build error: the content-length is not the length of the body, in files.upload"
        );
    }

    #[test]
    fn c6_1_absent_accessors_and_zero_build_tries() {
        let error = build(BuildFailure::Base);
        assert_eq!(error.kind(), Kind::Build);
        assert_eq!(error.endpoint(), "tasks.get");
        assert_eq!(error.tries(), 0);
        assert_eq!(error.status(), None);
        assert!(!error.is_status(404));
        assert!(!error.is_status(0));
        assert!(error.headers().is_none());
        assert_eq!(error.body(), None);
        assert_eq!(error.kept(), None);
        assert_eq!(error.problem(), None);
        assert_eq!(error.class(), None);
        assert!(!error.is_timeout());
        assert_eq!(error.retry_after(), None);
        assert_eq!(error.reason(), None);
        assert!(error.source().is_none());
    }

    #[test]
    fn c6_1_response_accessors_and_taking_problem() {
        let error = Error::refused(
            "tasks.get",
            4,
            StatusCode::NOT_FOUND,
            header_values(),
            Some("problem"),
        )
        .with_body(b"body", Kept::Complete);
        assert_eq!(error.kind(), Kind::Refused);
        assert_eq!(error.endpoint(), "tasks.get");
        assert_eq!(error.tries(), 4);
        assert_eq!(error.status(), Some(StatusCode::NOT_FOUND));
        assert!(error.is_status(404));
        assert!(!error.is_status(200));
        assert_eq!(
            error.headers().unwrap().get("x-request-id"),
            Some("PRIVATE_SENTINEL")
        );
        assert_eq!(error.body(), Some(b"body".as_slice()));
        assert_eq!(error.kept(), Some(Kept::Complete));
        assert_eq!(error.problem(), Some(&"problem"));
        assert_eq!(error.class(), None);
        assert_eq!(error.into_problem().unwrap(), "problem");

        let error = build(BuildFailure::Body).with_body(b"retained", Kept::Interrupted);
        let pointer = &*error.inner as *const _;
        let error = error.into_problem().unwrap_err();
        assert_eq!(&*error.inner as *const _, pointer);
        assert_eq!(error.body(), Some(b"retained".as_slice()));
        assert_eq!(error.kept(), Some(Kept::Interrupted));
    }

    #[test]
    fn c6_1_problem_mapping_preserves_all_evidence() {
        let mut invoked = 0;
        let error = Error::refused(
            "tasks.list",
            3,
            StatusCode::BAD_REQUEST,
            header_values(),
            Some(String::from("problem")),
        )
        .with_body(b"body", Kept::Cut)
        .with_retry_after(Some(Duration::from_secs(12)), None)
        .with_reader_details("decoder".into());
        let before = error.to_string();
        let error = error.map_problem(|p| {
            invoked += 1;
            p.len()
        });
        assert_eq!(invoked, 1);
        assert_eq!(error.problem(), Some(&7));
        assert_eq!(error.to_string(), before);
        assert_eq!(error.status(), Some(StatusCode::BAD_REQUEST));
        assert_eq!(
            error.headers().unwrap().get("x-request-id"),
            Some("PRIVATE_SENTINEL")
        );
        assert_eq!(error.body(), Some(b"body".as_slice()));
        assert_eq!(error.kept(), Some(Kept::Cut));
        assert_eq!(error.retry_after(), Some(Duration::from_secs(12)));
        assert!(error.details().to_string().contains("decoder"));

        let error = build(BuildFailure::Base).map_problem(|_| {
            invoked += 1;
            2
        });
        assert_eq!(invoked, 1);
        assert_eq!(error.problem(), None);
        assert_eq!(error.kind(), Kind::Build);

        let error: Error = Error::transport(
            "tasks.get",
            2,
            TransportError::connect().with_source(std::io::Error::other("safe cause")),
        );
        let source = error.source().unwrap() as *const _;
        let error = error.map_problem(|_| 9);
        assert_eq!(error.class(), Some(Class::Connect));
        assert!(std::ptr::eq(error.source().unwrap(), source));
        assert_eq!(error.tries(), 2);

        let error: Error = super::mismatch("reason").into_error(
            "tasks.get",
            1,
            StatusCode::OK,
            Headers::default(),
            false,
        );
        assert_eq!(error.map_problem(|_| 0).reason(), Some("reason"));
    }

    #[test]
    fn c6_1_retry_after_is_the_longer_recorded_wait() {
        let duration = Duration::from_secs;
        for (header, rule, expected) in [
            (None, None, None),
            (Some(duration(0)), None, Some(duration(0))),
            (Some(duration(9)), None, Some(duration(9))),
            (None, Some(duration(10)), Some(duration(10))),
            (Some(duration(9)), Some(duration(10)), Some(duration(10))),
            (Some(duration(10)), Some(duration(9)), Some(duration(10))),
            (Some(duration(9)), Some(duration(9)), Some(duration(9))),
        ] {
            let error = build(BuildFailure::Body).with_retry_after(header, rule);
            assert_eq!(error.retry_after(), expected);
        }
    }

    #[test]
    fn c6_1_classes_and_timeout_accessors() {
        for class in [
            Class::Connect,
            Class::Timeout,
            Class::Body,
            Class::Blocked,
            Class::Other,
        ] {
            let error: Error = Error::transport("tasks.get", 1, TransportError::new(class));
            assert_eq!(error.kind(), Kind::Transport);
            assert_eq!(error.class(), Some(class));
            assert_eq!(error.is_timeout(), class == Class::Timeout);
            assert!(error.source().is_none());
        }
        for clock in [Clock::Idle, Clock::Total] {
            let error: Error = Error::clock("tasks.get", 0, clock, Duration::from_millis(125));
            assert_eq!(error.class(), Some(Class::Timeout));
            assert!(error.is_timeout());
            assert!(error.source().is_none());
            assert!(error.to_string().contains("0.125 s"));
        }
    }

    #[test]
    fn c6_1_reason_is_explicit_and_reader_causes_are_structured() {
        for failure in [
            MismatchFailure::ContentType,
            MismatchFailure::BodyLimit,
            MismatchFailure::Decode {
                place: Place::root().field("data").key().variant().index(0),
                unexpected_label: true,
            },
        ] {
            let error = mismatch_at("tasks.list", failure)
                .with_body(b"PRIVATE_SENTINEL", Kept::Complete)
                .with_reader_details("PRIVATE_SENTINEL".into());
            assert_eq!(error.kind(), Kind::Mismatch);
            assert_eq!(error.reason(), None);
            let source = error.source().unwrap();
            assert!(!format!("{source} {source:?} {source:#?}").contains("PRIVATE_SENTINEL"));
            assert!(source.source().is_none());
        }
        for failure in [
            MismatchFailure::NoArm,
            MismatchFailure::NextPage,
            MismatchFailure::Arm,
        ] {
            assert!(mismatch_at("tasks.list", failure).source().is_none());
        }
        for next_page in [false, true] {
            let error: Error = super::mismatch(String::from("PRIVATE_SENTINEL")).into_error(
                "tasks.list",
                2,
                StatusCode::OK,
                Headers::default(),
                next_page,
            );
            assert_eq!(error.reason(), Some("PRIVATE_SENTINEL"));
            assert!(!error.to_string().contains("PRIVATE_SENTINEL"));
            assert!(error.details().to_string().contains("PRIVATE_SENTINEL"));
            assert!(error.source().is_none());
        }
    }

    #[test]
    fn c10_7_each_kind_redacts_values_in_display_debug_and_sources() {
        // Deliberately lacks Debug: neither Error formatting implementation requires it.
        struct NoDebug;
        let build = Error::<NoDebug>::build_source(
            "tasks.get",
            Box::new(std::io::Error::other("safe writer cause")),
        );
        let http = crate::reply(404, "PRIVATE_SENTINEL").error_for_status().unwrap_err()
            .with_url("https://user:PRIVATE_SENTINEL@example.invalid/PRIVATE_SENTINEL?token=PRIVATE_SENTINEL".parse().unwrap());
        let transport = Error::transport("tasks.get", 2, TransportError::from(http));
        let refused = Error::refused(
            "tasks.get",
            2,
            StatusCode::NOT_FOUND,
            header_values(),
            Some(NoDebug),
        );
        let mismatch = Error::mismatch(
            "tasks.list",
            2,
            StatusCode::OK,
            header_values(),
            MismatchFailure::Decode {
                place: Place::root()
                    .field("items")
                    .key()
                    .variant()
                    .index(3)
                    .field("kind"),
                unexpected_label: true,
            },
        );
        for error in [build, transport, refused, mismatch] {
            let error = error
                .with_body(b"PRIVATE_SENTINEL", Kept::Complete)
                .with_reader_details("PRIVATE_SENTINEL".into());
            let display = error.to_string();
            let debug = format!("{error:?}");
            assert!(debug.contains(&display));
            assert!(!format!("{display} {debug} {error:#?}").contains("PRIVATE_SENTINEL"));
            let mut source = error.source();
            while let Some(cause) = source {
                assert!(!format!("{cause} {cause:?} {cause:#?}").contains("PRIVATE_SENTINEL"));
                source = cause.source();
            }
        }
        let error = Error::refused(
            "tasks.get",
            1,
            StatusCode::BAD_REQUEST,
            Headers::default(),
            Some("PRIVATE_SENTINEL"),
        );
        assert!(!format!("{error} {error:?} {error:#?}").contains("PRIVATE_SENTINEL"));
        assert!(error.source().is_none());
    }

    #[test]
    fn c6_1_sources_of_author_code_are_preserved_without_printing_them() {
        let error: Error = Error::build_source(
            "tasks.create",
            Box::new(std::io::Error::other("PRIVATE_SENTINEL")),
        );
        assert_eq!(error.source().unwrap().to_string(), "PRIVATE_SENTINEL");
        assert!(!format!("{error} {error:?}").contains("PRIVATE_SENTINEL"));
        assert!(error.details().to_string().contains("PRIVATE_SENTINEL"));
        let error: Error = Error::transport(
            "tasks.get",
            1,
            TransportError::connect().with_source(std::io::Error::other("safe transport cause")),
        );
        assert_eq!(error.source().unwrap().to_string(), "safe transport cause");
        let io = std::io::Error::from(error);
        let original = io.get_ref().unwrap().downcast_ref::<Error>().unwrap();
        assert_eq!(original.endpoint(), "tasks.get");
        assert_eq!(original.class(), Some(Class::Connect));
    }

    #[test]
    fn c6_2_connection_words_use_structured_kinds_only() {
        for (source, expected) in [
            (std::io::ErrorKind::ConnectionRefused, "connection refused"),
            (std::io::ErrorKind::ConnectionReset, "connection reset"),
            (std::io::ErrorKind::Other, "no connection"),
        ] {
            let error: Error = Error::transport(
                "tasks.get",
                1,
                TransportError::connect().with_source(std::io::Error::new(
                    source,
                    "PRIVATE_SENTINEL: connection refused, name not found",
                )),
            );
            assert_eq!(
                error.to_string(),
                format!("transport error: {expected}, in tasks.get")
            );
        }
        let error: Error = Error::transport("tasks.get", 1, TransportError::connect());
        assert_eq!(
            error.to_string(),
            "transport error: no connection, in tasks.get"
        );
        let error: Error = Error::transport("tasks.get", 1, TransportError::timeout());
        assert_eq!(
            error.to_string(),
            "transport error: a timeout was reported by the transport, in tasks.get"
        );
        let error: Error = Error::transport("tasks.get", 1, TransportError::new(Class::Other));
        assert_eq!(
            error.to_string(),
            "transport error: the transport failed, in tasks.get"
        );
    }

    #[test]
    fn c5_6_kept_body_has_a_bounded_independent_allocation() {
        for (length, state, expected) in [
            (0, Kept::Complete, Kept::Complete),
            (64 * crate::KIB, Kept::Complete, Kept::Complete),
            (64 * crate::KIB + 1, Kept::Complete, Kept::Cut),
            (64 * crate::KIB + 1, Kept::Interrupted, Kept::Interrupted),
            (8, Kept::Cut, Kept::Cut),
        ] {
            let mut body = Vec::with_capacity(1_000_000);
            body.resize(length, b'x');
            let error = build(BuildFailure::Body).with_body(&body, state);
            assert_eq!(error.body().unwrap().len(), length.min(64 * crate::KIB));
            assert_eq!(error.kept(), Some(expected));
            if length != 0 {
                assert_ne!(error.body().unwrap().as_ptr(), body.as_ptr());
                body[0] = b'y';
                assert_eq!(error.body().unwrap()[0], b'x');
            }
        }
    }

    #[test]
    fn c6_3_details_are_opt_in_and_include_values_and_source_chain() {
        let error: Error = super::mismatch("PRIVATE_REASON")
            .into_error("tasks.list", 1, StatusCode::OK, header_values(), true)
            .with_body(b"PRIVATE_BODY\xff", Kept::Interrupted)
            .with_reader_details("PRIVATE_DECODER".into());
        assert_eq!(format!("{:?}", error.details()), "Details { .. }");
        let details = error.details().to_string();
        assert!(details.starts_with(&error.to_string()));
        for value in [
            "PRIVATE_REASON",
            "PRIVATE_SENTINEL",
            "PRIVATE_BODY�",
            "body (interrupted, 13 bytes)",
            "PRIVATE_DECODER",
        ] {
            assert!(details.contains(value), "missing {value}: {details}");
        }
        let error = Error::refused(
            "tasks.get",
            1,
            StatusCode::NOT_FOUND,
            Headers::default(),
            Some("PRIVATE_PROBLEM"),
        )
        .with_body(b"PRIVATE_BODY", Kept::Complete);
        let details = error.details().to_string();
        assert!(details.contains("PRIVATE_PROBLEM"));
        assert!(!details.contains("PRIVATE_BODY"));
        let error: Error = Error::build_source(
            "tasks.create",
            Box::new(std::io::Error::other(std::io::Error::other(
                "PRIVATE_INNER",
            ))),
        );
        assert!(error.details().to_string().contains("PRIVATE_INNER"));
    }

    #[test]
    fn c5_6_reader_details_are_bounded_even_for_small_text_in_large_storage() {
        let mut message = String::with_capacity(7_000_000);
        message.push_str("small");
        let error = build(BuildFailure::Body).with_reader_details(message);
        assert_eq!(error.inner.reader_details.as_deref(), Some("small"));
        assert!(error.inner.reader_details.unwrap().capacity() < 2048);
        let error =
            build(BuildFailure::Body).with_reader_details("a".repeat(1023) + "🦀" + "large");
        let kept = error.inner.reader_details.unwrap();
        assert!(kept.len() <= 1024);
        assert!(kept.capacity() < 2048);
        let error = build(BuildFailure::Body).with_reader_details("a".repeat(1024));
        assert_eq!(error.inner.reader_details.unwrap(), "a".repeat(1024));
    }

    #[test]
    fn c6_5_error_is_one_pointer_wide() {
        assert_eq!(
            std::mem::size_of::<Error<[u8; 4096]>>(),
            std::mem::size_of::<usize>()
        );
    }
    #[test]
    /// Also proves C6.2.
    fn c2_5_shape_messages_for_path_host_and_header_are_whole_lines() {
        for (slot, expected) in [
            (
                ValuePlace::Path,
                "build error: input filter cannot be written in a path, in tasks.get",
            ),
            (
                ValuePlace::Host,
                "build error: input filter cannot be written in a host, in tasks.get",
            ),
            (
                ValuePlace::Header,
                "build error: input filter cannot be written in a header, in tasks.get",
            ),
        ] {
            assert_eq!(
                build(BuildFailure::Shape {
                    input: "filter",
                    slot
                })
                .to_string(),
                expected
            );
        }
    }
    #[test]
    fn c2_13_entry_expression_failure_has_its_name_and_original_source() {
        let error: Error = Error::entry_source(
            "tasks.get",
            "limit",
            Box::new(std::io::Error::other("PRIVATE_SENTINEL")),
        );
        assert_eq!(
            error.to_string(),
            "build error: input limit could not be written, in tasks.get"
        );
        assert_eq!(error.kind(), Kind::Build);
        assert_eq!(error.tries(), 0);
        assert_eq!(error.source().unwrap().to_string(), "PRIVATE_SENTINEL");
        assert!(!format!("{error:?}").contains("PRIVATE_SENTINEL"));
    }
    #[test]
    fn c6_2_arm_and_pager_mismatch_use_distinct_whole_lines() {
        let arm: Error = super::mismatch("PRIVATE_SENTINEL").into_error(
            "tasks.list",
            1,
            StatusCode::OK,
            Headers::default(),
            false,
        );
        let page: Error = super::mismatch("PRIVATE_SENTINEL").into_error(
            "tasks.list",
            1,
            StatusCode::OK,
            Headers::default(),
            true,
        );
        assert_eq!(
            arm.to_string(),
            "mismatch: status 200, the status arm refused the answer, in tasks.list"
        );
        assert_eq!(
            page.to_string(),
            "mismatch: status 200, the next page cannot be asked, in tasks.list"
        );
    }
    #[test]
    fn c6_2_decode_place_and_label_and_root_without_label_use_whole_lines() {
        let error = mismatch_at(
            "tasks.list",
            MismatchFailure::Decode {
                place: Place::root().field("items").index(3).field("kind"),
                unexpected_label: true,
            },
        );
        assert_eq!(error.to_string(),"mismatch: status 200, items[3].kind is not what the type says and its content type is not the reader's, in tasks.list");
        let error = mismatch_at(
            "tasks.list",
            MismatchFailure::Decode {
                place: Place::root(),
                unexpected_label: false,
            },
        );
        assert_eq!(
            error.to_string(),
            "mismatch: status 200, the body is not what the type says, in tasks.list"
        );
    }
    #[test]
    fn c6_2_broken_body_without_a_head_uses_a_whole_line() {
        let error: Error = Error::transport("files.download", 1, TransportError::new(Class::Body));
        assert_eq!(
            error.to_string(),
            "transport error: the body broke, in files.download"
        );
    }
    #[derive(Debug)]
    struct Chain(&'static str, Option<Source>);
    impl fmt::Display for Chain {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.0)
        }
    }
    impl std::error::Error for Chain {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.1.as_deref().map(|e| e as _)
        }
    }
    #[test]
    fn c6_3_details_walk_the_chain_beyond_a_source_whose_display_omits_its_cause() {
        let error: Error = Error::build_source(
            "tasks.create",
            Box::new(Chain(
                "OUTER_ONLY",
                Some(Box::new(Chain("INNER_ONLY", None))),
            )),
        );
        assert_eq!(error.details().to_string(),"build error: the body could not be written, in tasks.create\ncaused by: OUTER_ONLY\ncaused by: INNER_ONLY");
    }
    #[test]
    fn c6_2_connection_words_walk_four_wrappers_and_cover_all_io_kinds() {
        use std::io::ErrorKind::*;
        for (kind, word) in [
            (ConnectionRefused, "connection refused"),
            (ConnectionReset, "connection reset"),
            (ConnectionAborted, "connection reset"),
            (BrokenPipe, "connection reset"),
            (NotConnected, "connection reset"),
            (UnexpectedEof, "connection reset"),
            (Other, "no connection"),
        ] {
            let mut cause: Source = Box::new(std::io::Error::from(kind));
            for _ in 0..4 {
                cause = Box::new(Chain("wrapper", Some(cause)));
            }
            let error: Error =
                Error::transport("tasks.get", 1, TransportError::connect().with_source(cause));
            assert_eq!(
                error.to_string(),
                format!("transport error: {word}, in tasks.get")
            );
        }
    }
    #[test]
    fn c6_1_is_status_compares_the_argument_for_every_retained_status() {
        for status in [200, 204, 404, 503, 599, 999] {
            let error: Error = Error::refused(
                "tasks.get",
                1,
                StatusCode::from_u16(status).unwrap(),
                Headers::default(),
                None,
            );
            for queried in [0, 200, 204, 404, 503, 599, 999, u16::MAX] {
                assert_eq!(error.is_status(queried), status == queried);
            }
        }
    }

    #[test]
    fn c6_2_clock_lengths_print_exact_seconds_without_trailing_zeros() {
        for (duration, seconds) in [
            (Duration::ZERO, "0"),
            (Duration::from_secs(30), "30"),
            (Duration::from_millis(1500), "1.5"),
            (Duration::from_millis(125), "0.125"),
            (Duration::from_nanos(1), "0.000000001"),
            (Duration::from_nanos(999_999_999), "0.999999999"),
            (
                Duration::new(u64::MAX, 999_999_999),
                "18446744073709551615.999999999",
            ),
        ] {
            for (clock, lead, tail) in [
                (Clock::Idle, "no answer for", "with_idle_timeout changes it"),
                (Clock::Total, "the call took longer than", "with_timeout"),
            ] {
                let error: Error = Error::clock("tasks.get", 1, clock, duration);
                assert_eq!(
                    error.to_string(),
                    format!("transport error: {lead} {seconds} s ({tail}), in tasks.get")
                );
            }
        }
    }

    #[test]
    fn c6_2_the_tries_suffix_is_present_only_after_more_than_one_try() {
        for tries in [0, 1, 2, 3, u32::MAX] {
            let suffix = if tries > 1 {
                format!(", after {tries} tries")
            } else {
                String::new()
            };
            for (error, lead) in [
                (
                    Error::<()>::transport("tasks.get", tries, TransportError::new(Class::Other)),
                    "transport error: the transport failed",
                ),
                (
                    Error::refused(
                        "tasks.get",
                        tries,
                        StatusCode::BAD_REQUEST,
                        Headers::default(),
                        None,
                    ),
                    "refused: status 400",
                ),
                (
                    Error::mismatch(
                        "tasks.get",
                        tries,
                        StatusCode::OK,
                        Headers::default(),
                        MismatchFailure::Arm,
                    ),
                    "mismatch: status 200, the status arm refused the answer",
                ),
            ] {
                assert_eq!(error.to_string(), format!("{lead}, in tasks.get{suffix}"));
            }
        }
    }
}
