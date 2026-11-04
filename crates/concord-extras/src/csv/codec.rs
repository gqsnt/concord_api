//! `Encode`/`Decode` of [`Csv`] and the response head check.

use std::error::Error as StdError;
use std::sync::Arc;

use concord_core::bytes::Bytes;
use concord_core::{CodecError, Decode, Encode, ResponseHead, reqwest};
use futures_core::Stream;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::frame::{Mode, decode_all};
use super::options::{Config, MediaPolicy};
use super::record::{self, RawTable};
use super::write::{Rows, StreamRows, encode_raw, encode_rows, stream_body};
use super::{Csv, CsvProfile, DelimiterPolicy, HeaderPolicy, content_type};
use crate::media;

/// The resolved configuration of a static type (validated now).
fn config<H: HeaderPolicy, D: DelimiterPolicy, P: CsvProfile>() -> Result<Arc<Config>, CodecError> {
    Ok(Arc::new(Config::new(Csv::<H, D, P>::options())?))
}

/// The [`Decode::Context`] of [`Csv`]: its configuration, validated by
/// [`context`](Decode::context) before the body is read, for
/// [`decode_body`](Decode::decode_body). Opaque.
pub struct CsvContext(Arc<Config>);

impl CsvContext {
    /// The validated configuration of a static type, for a direct
    /// [`decode_body`](Decode::decode_body) call.
    ///
    /// # Errors
    ///
    /// A [`CodecError`] (labelled like the [`CsvConfigError`](super::CsvConfigError)) for an
    /// invalid dialect.
    pub fn of<H: HeaderPolicy, D: DelimiterPolicy, P: CsvProfile>() -> Result<Self, CodecError> {
        config::<H, D, P>().map(Self)
    }
}

impl std::fmt::Debug for CsvContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CsvContext").field(&self.0.options).finish()
    }
}

/// The CSV `Content-Type` rules (see [`MediaPolicy`]).
pub(crate) fn check_media(
    head: &ResponseHead<'_>,
    has_header: bool,
    policy: MediaPolicy,
) -> Result<(), CodecError> {
    if policy == MediaPolicy::Unchecked {
        return Ok(());
    }
    let Some(found) = media::single(head)? else {
        return match policy {
            MediaPolicy::TextCsvOrMissing => Ok(()),
            _ => Err(CodecError::new("the response has no Content-Type")),
        };
    };
    let csv = found.essence.type_() == mime::TEXT
        && found.essence.subtype() == "csv"
        && found.essence.suffix().is_none();
    if !csv {
        return Err(CodecError::new("the response Content-Type is not text/csv"));
    }
    match found.charsets.as_slice() {
        [] => {}
        [one] if media::is_utf8(one) => {}
        [_] => {
            return Err(CodecError::new(
                "the CSV response declares a charset other than utf-8",
            ));
        }
        _ => {
            return Err(CodecError::new(
                "the response Content-Type declares charset more than once",
            ));
        }
    }
    let mut header = found.params.iter().filter(|(name, _)| name == "header");
    match (header.next(), header.next()) {
        (None, _) => Ok(()),
        (Some(_), Some(_)) => Err(CodecError::new(
            "the response Content-Type declares header more than once",
        )),
        (Some((_, value)), None) => {
            let present = if value.eq_ignore_ascii_case("present") {
                true
            } else if value.eq_ignore_ascii_case("absent") {
                false
            } else {
                return Err(CodecError::new(
                    "the CSV response's header parameter is neither present nor absent",
                ));
            };
            match (present, has_header) {
                (true, false) => Err(CodecError::new(
                    "the CSV response declares header=present, but this format reads no header",
                )),
                (false, true) => Err(CodecError::new(
                    "the CSV response declares header=absent, but this format expects a header",
                )),
                _ => Ok(()),
            }
        }
    }
}

/// Typed rows of a collected body.
impl<T, H, D, P> Decode<Vec<T>> for Csv<H, D, P>
where
    T: DeserializeOwned,
    H: HeaderPolicy,
    D: DelimiterPolicy,
    P: CsvProfile,
{
    type Context = CsvContext;
    const ACCEPT: Option<&'static str> = Some("text/csv");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        check_media(head, H::HAS_HEADER, P::options().media_policy())
    }

    fn context(_: &ResponseHead<'_>) -> Result<CsvContext, CodecError> {
        CsvContext::of::<H, D, P>()
    }

    fn decode_body(context: CsvContext, body: Bytes) -> Result<Vec<T>, CodecError> {
        decode_all(context.0, Mode::Typed, &body, record::typed::<T>).map(|(_, rows)| rows)
    }
}

/// Byte records of a collected body.
impl<H, D, P> Decode<RawTable> for Csv<H, D, P>
where
    H: HeaderPolicy,
    D: DelimiterPolicy,
    P: CsvProfile,
{
    type Context = CsvContext;
    const ACCEPT: Option<&'static str> = Some("text/csv");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        check_media(head, H::HAS_HEADER, P::options().media_policy())
    }

    fn context(_: &ResponseHead<'_>) -> Result<CsvContext, CodecError> {
        CsvContext::of::<H, D, P>()
    }

    fn decode_body(context: CsvContext, body: Bytes) -> Result<RawTable, CodecError> {
        decode_all(context.0, Mode::Raw, &body, record::raw)
            .map(|(headers, records)| RawTable { headers, records })
    }
}

/// A finite body of rows.
impl<T, H, D, P> Encode<Vec<T>> for Csv<H, D, P>
where
    T: Serialize,
    H: HeaderPolicy,
    D: DelimiterPolicy,
    P: CsvProfile,
{
    const CONTENT_TYPE: Option<&'static str> = Some(content_type(H::HAS_HEADER));

    fn encode(value: Vec<T>) -> Result<reqwest::Body, CodecError> {
        encode_rows(config::<H, D, P>()?, Rows::new(value)).map(reqwest::Body::from)
    }
}

/// A finite body of an iterator's rows (encoded before sending).
impl<I, H, D, P> Encode<Rows<I>> for Csv<H, D, P>
where
    I: IntoIterator,
    I::Item: Serialize,
    H: HeaderPolicy,
    D: DelimiterPolicy,
    P: CsvProfile,
{
    const CONTENT_TYPE: Option<&'static str> = Some(content_type(H::HAS_HEADER));

    fn encode(value: Rows<I>) -> Result<reqwest::Body, CodecError> {
        encode_rows(config::<H, D, P>()?, value).map(reqwest::Body::from)
    }
}

/// A lazy body over a one-shot source: preparation polls it zero times.
impl<S, T, E, H, D, P> Encode<StreamRows<S>> for Csv<H, D, P>
where
    S: Stream<Item = Result<T, E>> + Send + 'static,
    T: Serialize + 'static,
    E: Into<Box<dyn StdError + Send + Sync>>,
    H: HeaderPolicy,
    D: DelimiterPolicy,
    P: CsvProfile,
{
    const CONTENT_TYPE: Option<&'static str> = Some(content_type(H::HAS_HEADER));

    fn encode(value: StreamRows<S>) -> Result<reqwest::Body, CodecError> {
        stream_body(config::<H, D, P>()?, value)
    }
}

/// A finite body of byte records.
impl<H, D, P> Encode<RawTable> for Csv<H, D, P>
where
    H: HeaderPolicy,
    D: DelimiterPolicy,
    P: CsvProfile,
{
    const CONTENT_TYPE: Option<&'static str> = Some(content_type(H::HAS_HEADER));

    fn encode(value: RawTable) -> Result<reqwest::Body, CodecError> {
        encode_raw(config::<H, D, P>()?, value).map(reqwest::Body::from)
    }
}
