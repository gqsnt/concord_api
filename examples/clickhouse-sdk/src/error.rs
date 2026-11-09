//! The SDK's error type.
//!
//! `Display` follows Concord's rule: the operation, a static description, numbers (status,
//! exception code, rows, bytes) and names chosen by the SDK or the caller (a placeholder name) —
//! never a server message, a bound value, a URL or a header value. ClickHouse exception messages
//! can quote query text and parameter values, so they are reached deliberately through
//! `message()`.

use std::fmt;

use concord::reqwest::StatusCode;
use concord::stream::{Position, ReadError, ReadErrorKind};
use concord::{Document, Failure};

use crate::guard::BodyException;
use crate::meta::ResponseMeta;
use crate::query::RequestError;

/// Every failure of the SDK's operations.
#[derive(Debug, concord::derive::Error)]
#[non_exhaustive]
pub enum Error {
    /// Refused locally before anything was sent (an unbound placeholder, an invalid identifier
    /// or query ID).
    #[display("request refused before sending: {0}")]
    Request(RequestError),
    /// Concord could not build or send the request, or the transport failed before a response
    /// head arrived or while reading an insert receipt body after the head (`kind()` is `Build`
    /// or `Transport`).
    #[display("{0}")]
    Concord(concord::Error),
    /// The server answered a non-2xx status: an exception raised before the response started.
    #[display("{0}")]
    #[from(concord::Error<String>)]
    Server(Box<ServerException>),
    /// The status was 2xx, but the server reported an exception: in the response body after rows
    /// were sent (the exception block), or in the response head (`X-ClickHouse-Exception-Code` on
    /// a 2xx head). Rows already returned stay returned; the result is incomplete.
    #[display("{0}")]
    LateException(Box<LateException>),
    /// The status was 2xx, but the head was refused before the body was read: another format
    /// than the one requested (`X-ClickHouse-Format`), or a `Content-Type` the reader does not
    /// accept.
    #[display("{0}")]
    Response(Box<ResponseError>),
    /// Reading the body failed after it started: the transport broke (ClickHouse 24.11 and
    /// later break the HTTP framing after a late exception), a row is malformed, a row does not
    /// decode into the row type, or a limit was reached.
    #[display("{0}")]
    Read(Box<ReadFailure>),
}

impl Error {
    /// The response head retained by `Server`, `LateException`, `Response` and `Read`.
    /// `Request` and `Concord` return `None`, including an insert receipt-body transport failure
    /// after the response head arrived.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Server(e) => Some(&e.meta),
            Self::LateException(e) => Some(&e.meta),
            Self::Response(e) => Some(&e.meta),
            Self::Read(e) => Some(&e.meta),
            Self::Request(_) | Self::Concord(_) => None,
        }
    }

    /// The ClickHouse exception code (`X-ClickHouse-Exception-Code`, else the `Code: N` prefix of
    /// the message) of a server or late exception.
    pub fn exception_code(&self) -> Option<i32> {
        match self {
            Self::Server(e) => e.code,
            Self::LateException(e) => e.code,
            _ => None,
        }
    }

    /// Whether the outcome of the server-side work is unknown: the request may have been
    /// executed, fully or partly (a transport failure after sending, a late exception, a broken
    /// body). A `Server` exception, a `Request` refusal and a `Build` error are known failures.
    pub fn is_outcome_unknown(&self) -> bool {
        match self {
            Self::Concord(e) => e.kind() == concord::ErrorKind::Transport,
            Self::LateException(_) | Self::Read(_) => true,
            Self::Request(_) | Self::Server(_) | Self::Response(_) => false,
        }
    }
}

/// The `Code: N` prefix of a ClickHouse exception message (`Code: 62. DB::Exception: ...`, or the
/// older `Code: 59, e.displayText() = ...`).
pub(crate) fn code_of(message: &str) -> Option<i32> {
    let rest = message.trim_start().strip_prefix("Code: ")?;
    let digits = rest.split(|c: char| !c.is_ascii_digit()).next()?;
    digits.parse().ok()
}

/// The error name in parentheses before `(version ...)`: `SYNTAX_ERROR` in
/// `... (SYNTAX_ERROR) (version 24.3.1)`.
pub(crate) fn name_of(message: &str) -> Option<String> {
    let before = &message[..message.rfind("(version ")?];
    let before = before.trim_end().strip_suffix(')')?;
    let name = &before[before.rfind('(')? + 1..];
    (!name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'))
    .then(|| name.to_owned())
}

/// A ClickHouse exception answered with a non-2xx status (before the response started).
pub struct ServerException {
    pub(crate) operation: &'static str,
    pub(crate) status: StatusCode,
    pub(crate) code: Option<i32>,
    pub(crate) name: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) meta: ResponseMeta,
}

/// The exception text of a non-2xx answer (bounded), its code and name.
impl From<Failure<String>> for ServerException {
    fn from(failure: Failure<String>) -> Self {
        let operation = failure.operation().map_or("", |op| op.name());
        let (meta, document, _) = failure.into_parts();
        let meta = ResponseMeta::new(meta);
        let message = match document {
            Document::Read(text) => Some(text.trim_end().to_owned()),
            _ => None,
        }
        .filter(|text| !text.is_empty());
        Self {
            operation,
            status: meta.status(),
            code: meta
                .exception_code()
                .or_else(|| message.as_deref().and_then(code_of)),
            name: message.as_deref().and_then(name_of),
            message,
            meta,
        }
    }
}

impl ServerException {
    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The exception code: `X-ClickHouse-Exception-Code`, else the message's `Code: N` prefix.
    pub fn code(&self) -> Option<i32> {
        self.code
    }

    /// The error name (`SYNTAX_ERROR`), when the message ends with `(NAME) (version ...)`.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The exception text the server sent, **unredacted** (it can quote the query and parameter
    /// values); `None` when the body was empty, not UTF-8, unreadable or over the SDK's 64 KiB
    /// bound.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// The response head.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }
}

impl fmt::Display for ServerException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: the server refused the query with HTTP status {}",
            self.operation,
            self.status.as_u16()
        )?;
        if let Some(code) = self.code {
            write!(f, ", exception code {code}")?;
        }
        if let Some(name) = &self.name {
            write!(f, " ({name})")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ServerException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerException")
            .field("operation", &self.operation)
            .field("status", &self.status.as_u16())
            .field("code", &self.code)
            .field("name", &self.name)
            .field("message", &self.message.as_ref().map(|_| "<withheld>"))
            .finish()
    }
}

/// Where a late exception was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LateSource {
    /// The tagged exception block in the body (`X-ClickHouse-Exception-Tag`).
    Block {
        /// Whether the block's trailer was found and its declared length matched the message.
        complete: bool,
    },
    /// `X-ClickHouse-Exception-Code` on a 2xx head (headers still being sent when the exception
    /// happened, e.g. with progress headers); the body text, if any, is the message.
    Head,
}

/// An exception the server reported after it had answered 2xx.
pub struct LateException {
    pub(crate) operation: &'static str,
    pub(crate) code: Option<i32>,
    pub(crate) name: Option<String>,
    pub(crate) message: String,
    pub(crate) source: LateSource,
    pub(crate) rows_before: u64,
    pub(crate) meta: ResponseMeta,
}

impl LateException {
    pub(crate) fn from_block(
        operation: &'static str,
        block: &BodyException,
        rows_before: u64,
        meta: ResponseMeta,
    ) -> Self {
        let message = block.message().to_owned();
        Self {
            operation,
            code: meta.exception_code().or_else(|| code_of(&message)),
            name: name_of(&message),
            message,
            source: LateSource::Block {
                complete: block.complete,
            },
            rows_before,
            meta,
        }
    }

    /// The exception code: `X-ClickHouse-Exception-Code` if the head carried it, else the
    /// message's `Code: N` prefix.
    pub fn code(&self) -> Option<i32> {
        self.code
    }

    /// The error name (`FUNCTION_THROW_IF_VALUE_IS_NON_ZERO`), when the message ends with
    /// `(NAME) (version ...)`.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The exception text, **unredacted** (it can quote the query and parameter values).
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Where it was found.
    pub fn source(&self) -> LateSource {
        self.source
    }

    /// Rows the reader returned before the exception.
    pub fn rows_before(&self) -> u64 {
        self.rows_before
    }

    /// The response head (2xx).
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }
}

impl fmt::Display for LateException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: the server reported an exception after answering HTTP {}",
            self.operation,
            self.meta.status().as_u16()
        )?;
        if let Some(code) = self.code {
            write!(f, " (exception code {code})")?;
        }
        write!(f, "; {} rows were returned before it", self.rows_before)
    }
}

impl fmt::Debug for LateException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LateException")
            .field("operation", &self.operation)
            .field("code", &self.code)
            .field("name", &self.name)
            .field("message", &"<withheld>")
            .field("source", &self.source)
            .field("rows_before", &self.rows_before)
            .finish()
    }
}

/// Why a 2xx head was refused before its body was read.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResponseProblem {
    /// `X-ClickHouse-Format` names another format (a `FORMAT` clause in the query won over the
    /// request header, on a server before 26.8).
    FormatMismatch {
        /// The format requested.
        requested: &'static str,
        /// The format reported.
        served: String,
    },
    /// The reader refused the `Content-Type` (a static label from Concord's reader).
    ContentType {
        /// The label.
        label: &'static str,
    },
}

/// A 2xx response whose head the SDK refused (the body was not read).
#[derive(Debug)]
pub struct ResponseError {
    pub(crate) operation: &'static str,
    pub(crate) problem: ResponseProblem,
    pub(crate) meta: ResponseMeta,
}

impl ResponseError {
    /// What was refused.
    pub fn problem(&self) -> &ResponseProblem {
        &self.problem
    }

    /// The response head.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }
}

impl fmt::Display for ResponseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.problem {
            ResponseProblem::FormatMismatch { requested, .. } => write!(
                f,
                "{}: the response is not in the requested format {requested}",
                self.operation
            ),
            ResponseProblem::ContentType { label } => {
                write!(
                    f,
                    "{}: the response head was refused: {label}",
                    self.operation
                )
            }
        }
    }
}

/// A failure while reading rows (after the body started), with the response head.
pub struct ReadFailure {
    pub(crate) operation: &'static str,
    pub(crate) error: ReadError,
    pub(crate) meta: ResponseMeta,
}

impl ReadFailure {
    /// The stage: `Transport`, `Format`, `Decode` or `Limit`.
    pub fn kind(&self) -> ReadErrorKind {
        self.error.kind()
    }

    /// Rows returned and body bytes consumed before the failure.
    pub fn position(&self) -> Position {
        self.error.position()
    }

    /// Concord's reader error (its `source_unredacted()` may quote the body).
    pub fn read_error(&self) -> &ReadError {
        &self.error
    }

    /// The response head.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }
}

impl fmt::Display for ReadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The reader names the operation itself (`read_for`): `<operation>: <reader error>`.
        fmt::Display::fmt(&self.error, f)
    }
}

impl fmt::Debug for ReadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadFailure")
            .field("operation", &self.operation)
            .field("error", &self.error)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_names_are_read_from_documented_messages() {
        assert_eq!(
            code_of("Code: 62. DB::Exception: Syntax error (Multi-statements are not allowed)"),
            Some(62)
        );
        assert_eq!(
            code_of("Code: 59, e.displayText() = DB::Exception: Syntax error"),
            Some(59)
        );
        assert_eq!(code_of("DB::Exception: no code"), None);
        assert_eq!(
            name_of(
                "Code: 457. DB::Exception: Value abc 123 cannot be parsed as String for query parameter 'arg1'. (BAD_QUERY_PARAMETER) (version 23.4.1.869 (official build))"
            )
            .as_deref(),
            Some("BAD_QUERY_PARAMETER")
        );
        assert_eq!(name_of("Code: 1. no name (version 1)"), None);
        assert_eq!(name_of("Code: 1. lower (abc) (version 1)"), None);
    }
}
