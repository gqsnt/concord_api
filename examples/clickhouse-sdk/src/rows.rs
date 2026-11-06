//! Rows read incrementally from a CSVWithNames or JSONEachRow response.

use std::fmt;

use concord::csv::CsvRecords;
use concord::jsonl::JsonLinesRecords;
use concord::reqwest;
use concord::stream::{Completion, Position, ReadError, ReadErrorKind};
use serde::de::DeserializeOwned;

use crate::error::{Error, LateException, ReadFailure};
use crate::format::OutputFormat;
use crate::guard::{BodyException, Guarded};
use crate::meta::ResponseMeta;

pub(crate) type Body = Guarded<reqwest::Response>;

/// Both readers are boxed (several hundred bytes each; one allocation per query).
pub(crate) enum Reader<T> {
    Csv(Box<CsvRecords<T, Body>>),
    Json(Box<JsonLinesRecords<T, Body>>),
}

/// How far a row read got. Only [`Complete`](Self::Complete) is a complete result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Outcome {
    /// Rows may follow (also after the caller stopped early).
    Reading,
    /// The body ended cleanly, the response announced its exception tag
    /// (`X-ClickHouse-Exception-Tag`) and no exception block was found: every row was read and
    /// the server marked no late exception.
    Complete,
    /// The body ended cleanly, but the response carried no exception tag. A server that writes an
    /// unmarked plain-text exception at the end of the body (before ClickHouse 24.11 the body
    /// then even ended cleanly) is only detected when the text does not parse as rows: a clean
    /// end here does **not** prove that the query succeeded.
    EndedWithoutMarker,
    /// A terminal error ended the read: a late exception, a broken body, a malformed or
    /// undecodable row, a limit. Not a complete result.
    Failed,
}

/// The rows of one query, read as the response arrives.
///
/// Each [`next`](Self::next) reads only what the next row needs; nothing is read ahead and no
/// task is spawned. Dropping it drops the response and closes its connection: that stops the
/// client's read, **not** the query on the server (the HTTP interface documents that running
/// requests do not stop when the connection is lost); [`ClickHouse::cancel`](crate::ClickHouse::cancel)
/// asks the server to stop it.
///
/// A dropped `next()` future loses no byte: the reader's sources (the response body and the
/// SDK's exception guard) keep every chunk they took.
pub struct Rows<T> {
    pub(crate) reader: Reader<T>,
    pub(crate) operation: &'static str,
    pub(crate) format: OutputFormat,
    pub(crate) marked: bool,
    pub(crate) meta: ResponseMeta,
}

impl<T: DeserializeOwned> Rows<T> {
    /// The next row; `None` at the end of the body and after an error.
    ///
    /// An `Err` is terminal: [`Error::LateException`] (rows returned before it stay returned),
    /// or [`Error::Read`] (transport, malformed row, row type mismatch, limit).
    pub async fn next(&mut self) -> Option<Result<T, Error>> {
        let item = match &mut self.reader {
            Reader::Csv(rows) => rows.next().await,
            Reader::Json(rows) => rows.next().await,
        }?;
        Some(item.map_err(|error| self.failure(error)))
    }

    /// Reads every remaining row.
    ///
    /// # Errors
    ///
    /// The first terminal error; the rows read before it are dropped (use [`next`](Self::next)
    /// to keep them).
    pub async fn collect(mut self) -> Result<Collected<T>, Error> {
        let mut rows = Vec::new();
        while let Some(row) = self.next().await {
            rows.push(row?);
        }
        Ok(Collected {
            outcome: self.outcome(),
            rows,
            meta: self.meta,
        })
    }

    fn failure(&self, error: ReadError) -> Error {
        if error.kind() == ReadErrorKind::Transport
            && let Some(block) = error
                .source_unredacted()
                .and_then(|source| source.downcast_ref::<BodyException>())
        {
            return Error::LateException(Box::new(LateException::from_block(
                self.operation,
                block,
                error.position().records,
                self.meta.clone(),
            )));
        }
        Error::Read(Box::new(ReadFailure {
            operation: self.operation,
            error,
            meta: self.meta.clone(),
        }))
    }
}

// `JsonLinesRecords::completion`/`position` require `T: DeserializeOwned` (the CSV reader's do
// not), so these accessors carry the bound too.
impl<T: DeserializeOwned> Rows<T> {
    fn completion(&self) -> Completion {
        match &self.reader {
            Reader::Csv(rows) => rows.completion(),
            Reader::Json(rows) => rows.completion(),
        }
    }

    /// Where the read stands; see [`Outcome`].
    pub fn outcome(&self) -> Outcome {
        match self.completion() {
            Completion::EndOfInput if self.marked => Outcome::Complete,
            Completion::EndOfInput => Outcome::EndedWithoutMarker,
            Completion::Failed => Outcome::Failed,
            Completion::Reading => Outcome::Reading,
        }
    }

    /// Rows returned and body bytes consumed so far.
    pub fn position(&self) -> Position {
        match &self.reader {
            Reader::Csv(rows) => rows.position(),
            Reader::Json(rows) => rows.position(),
        }
    }

    /// The response head (status, `X-ClickHouse-*` metadata).
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// The format the rows are read in.
    pub fn format(&self) -> OutputFormat {
        self.format
    }

    /// The CSV header names, once the header record was read (CSVWithNames only).
    pub fn columns(&self) -> Option<Vec<String>> {
        match &self.reader {
            Reader::Csv(rows) => rows.headers().map(|names| {
                names
                    .iter()
                    .map(|name| String::from_utf8_lossy(name).into_owned())
                    .collect()
            }),
            Reader::Json(_) => None,
        }
    }
}

impl<T: DeserializeOwned> fmt::Debug for Rows<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rows")
            .field("operation", &self.operation)
            .field("format", &self.format)
            .field("outcome", &self.outcome())
            .field("position", &self.position())
            .field("meta", &self.meta)
            .finish()
    }
}

/// Every row of a query that ended without error, with the outcome and the head.
#[derive(Debug)]
pub struct Collected<T> {
    /// The rows.
    pub rows: Vec<T>,
    /// [`Outcome::Complete`] or [`Outcome::EndedWithoutMarker`].
    pub outcome: Outcome,
    /// The response head.
    pub meta: ResponseMeta,
}
