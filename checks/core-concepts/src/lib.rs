//! Shared declarations for the SDK50 P1 comparisons (`tests/`): a small rates service written
//! the way an SDK author would, a provider error document, and a format whose `Context` is not
//! `()` — the counterexample to any response helper that assumes JSON's stateless context.

use std::sync::atomic::{AtomicUsize, Ordering};

use concord::bytes::Bytes;
use concord::{CodecError, ContentTypeField, Decode, Json, ResponseHead};
use serde::Deserialize;

/// One exchange rate.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Rate {
    /// Base currency.
    pub base: String,
    /// Quote currency.
    pub quote: String,
    /// Units of quote per base.
    pub rate: f64,
}

/// A provider error document.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Problem {
    /// Human text from the provider (never printed by Concord).
    pub message: String,
    /// A machine code, when sent.
    pub code: Option<String>,
}

/// How many times [`Versioned`]'s `context` ran (tests reset it).
pub static VERSIONED_CONTEXTS: AtomicUsize = AtomicUsize::new(0);

/// A text error document whose meaning depends on the head: `x-doc-version` selects how the body
/// is read, and only version 2 is understood. The version check is a head refusal (before any
/// body byte), and the version travels to `decode_body` in the `Context`.
pub struct Versioned;

impl Decode<Problem> for Versioned {
    type Context = u8;

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        match head.content_type() {
            ContentTypeField::One(value) if value.as_bytes().starts_with(b"text/plain") => Ok(()),
            _ => Err(CodecError::new("not a text/plain error document")),
        }
    }

    fn context(head: &ResponseHead<'_>) -> Result<u8, CodecError> {
        VERSIONED_CONTEXTS.fetch_add(1, Ordering::SeqCst);
        let version = head
            .headers()
            .get("x-doc-version")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u8>().ok())
            .ok_or(CodecError::new("missing or unreadable x-doc-version"))?;
        if version == 2 {
            Ok(version)
        } else {
            Err(CodecError::new("unsupported error document version").at("version", version.into()))
        }
    }

    fn decode_body(version: u8, body: Bytes) -> Result<Problem, CodecError> {
        let text = std::str::from_utf8(&body).map_err(|_| CodecError::new("not UTF-8"))?;
        let (code, message) = text
            .split_once(':')
            .ok_or(CodecError::new("no `code:` prefix"))?;
        Ok(Problem {
            message: message.trim().to_owned(),
            code: Some(format!("v{version}/{code}")),
        })
    }
}

concord::api! {
    /// A rates service.
    pub client Rates() {
        base "http://rates.invalid";
        format Json;

        /// One rate (a finite JSON output).
        GET rate(base: String, quote: String) -> Rate { path "rate/{base}/{quote}"; }
        /// Purges a cache entry (no output: the body is drained).
        POST purge(key: String) { path "cache/{key}/purge"; }
    }
}

/// The JSON format, named for the tests.
pub type ProblemJson = Json;
