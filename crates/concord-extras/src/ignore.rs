//! `IgnoreContentType<F>`: a format without its `Content-Type` check.

use std::marker::PhantomData;

use concord_core::bytes::Bytes;
use concord_core::{
    CodecError, Decode, Encode, EncodedBody, Incoming, Incremental, ResponseHead, reqwest,
};

/// The format `F` without its checks of the response `Content-Type` field.
///
/// Select it deliberately, on the endpoint (or scope) whose server mislabels its responses:
/// `response IgnoreContentType<Json>;`. It skips exactly `F`'s
/// [`check_content_type`](Decode::check_content_type) and nothing else:
///
/// - the status is still required to be 2xx (a non-2xx is still a
///   [`Status`](concord_core::ErrorKind::Status) error);
/// - `F`'s other head checks ([`context`](Decode::context): a version header, say) still run;
/// - the body is still decoded by `F` (malformed JSON is still a decode error);
/// - transport errors and reqwest's own decompression are unchanged;
/// - `F`'s `Accept` suggestion is kept;
/// - encoding is `F`'s, unchanged (`format IgnoreContentType<Json>;` covers both directions).
///
/// It works with any format that keeps its `Content-Type` checks in `check_content_type`, as the
/// [`Decode`] contract asks; a format of yours gets the same treatment.
pub struct IgnoreContentType<F>(PhantomData<fn() -> F>);

impl<T, F: Decode<T>> Decode<T> for IgnoreContentType<F> {
    type Context = F::Context;
    const ACCEPT: Option<&'static str> = F::ACCEPT;

    fn context(head: &ResponseHead<'_>) -> Result<F::Context, CodecError> {
        F::context(head)
    }

    fn decode_body(context: F::Context, body: Bytes) -> Result<T, CodecError> {
        F::decode_body(context, body)
    }
}

impl<T, F: Encode<T>> Encode<T> for IgnoreContentType<F> {
    const CONTENT_TYPE: Option<&'static str> = F::CONTENT_TYPE;

    fn encode(value: T) -> Result<reqwest::Body, CodecError> {
        F::encode(value)
    }

    /// `F`'s, unchanged: a runtime media type `F` binds to the body is kept.
    fn encode_body(value: T) -> Result<EncodedBody, CodecError> {
        F::encode_body(value)
    }
}

/// The streaming counterpart (`-> stream T`): skips exactly `F`'s
/// [`check_content_type`](Incremental::check_content_type); `F`'s other head checks, its reader
/// and its `Accept` suggestion are kept.
impl<T, F: Incremental<T>> Incremental<T> for IgnoreContentType<F> {
    type Records = F::Records;
    type Context = F::Context;
    const ACCEPT: Option<&'static str> = F::ACCEPT;

    fn context(head: &ResponseHead<'_>) -> Result<F::Context, CodecError> {
        F::context(head)
    }

    fn open(context: F::Context, body: Incoming) -> F::Records {
        F::open(context, body)
    }
}
