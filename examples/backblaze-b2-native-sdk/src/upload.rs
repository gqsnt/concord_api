//! `b2_upload_file`: the file to send, its body, the attempt policy and what each attempt proves.

use std::error::Error as StdError;
use std::fmt;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll};

use concord::bytes::Bytes;
use concord::reqwest::StatusCode;
use concord::{Delivery, EncodedBody, ErrorKind, Prepared, reqwest};
use concord_retry::{Decision, Policy, Replay, Retry, StopReason};
use futures_core::Stream;
use http_body::{Frame, SizeHint};

use crate::Session;
use crate::error::{Error, ErrorCode, InputError, Violation};
use crate::model::{
    BucketId, ContentType, FileId, FileName, FileVersion, Sha1Hex, UploadTarget, UploadUrl,
};
use crate::wire::{FileVersionWire, provider_error};

type BoxError = Box<dyn StdError + Send + Sync>;

/// What a one-shot source handed out, observed by the body wrapper.
#[derive(Debug, Default)]
pub(crate) struct Tally {
    bytes: AtomicU64,
    failed: AtomicBool,
}

impl Tally {
    fn failed(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }

    fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::SeqCst)
    }
}

/// Why a one-shot source could not supply its declared bytes. The transfer fails; its own error
/// is withheld from `Display` and `Debug` (it may quote content).
#[derive(Debug)]
pub(crate) enum SourceFailure {
    /// The source ended before the declared length.
    Shorter,
    /// The source yielded an error.
    Source,
}

impl fmt::Display for SourceFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Shorter => "the upload source ended before its declared length",
            Self::Source => "the upload source failed",
        })
    }
}

impl StdError for SourceFailure {}

/// Adapts the caller's stream error type.
struct Boxed<S>(Pin<Box<S>>);

impl<S, E> Stream for Boxed<S>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{
    type Item = Result<Bytes, BoxError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut()
            .0
            .as_mut()
            .poll_next(cx)
            .map(|item| item.map(|chunk| chunk.map_err(Into::into)))
    }
}

struct StreamState {
    source: Pin<Box<dyn Stream<Item = Result<Bytes, BoxError>> + Send>>,
    sent: u64,
    ended: bool,
}

/// A one-shot request body of an exact declared length: `Content-Length` is that length (B2
/// does not accept chunked uploads). It is not polled once the declared length was handed out;
/// a source that ends early or fails fails the transfer before the body is complete, and the
/// failure is recorded so the attempt is classified [`Evidence::SourceFailed`].
///
/// `reqwest::Body::wrap` requires `Sync`; the state sits behind a `Mutex` reached only through
/// `get_mut` (no locking), so the source need only be `Send`.
pub(crate) struct SizedStream {
    declared: u64,
    state: Mutex<StreamState>,
    tally: Arc<Tally>,
}

impl http_body::Body for SizedStream {
    type Data = Bytes;
    type Error = SourceFailure;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, SourceFailure>>> {
        let this = self.get_mut();
        let declared = this.declared;
        let tally = Arc::clone(&this.tally);
        let state = this.state.get_mut().unwrap_or_else(PoisonError::into_inner);
        if state.ended || state.sent >= declared {
            // Complete: the source is not polled past its declared length.
            state.ended = true;
            return Poll::Ready(None);
        }
        let fail = |state: &mut StreamState, failure| {
            state.ended = true;
            tally.failed.store(true, Ordering::SeqCst);
            Poll::Ready(Some(Err(failure)))
        };
        match state.source.as_mut().poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(Ok(chunk))) => {
                // A chunk crossing the declared length is cut by hyper, which writes at most
                // `Content-Length` bytes; nothing after it is polled.
                state.sent += chunk.len() as u64;
                tally.bytes.store(state.sent, Ordering::SeqCst);
                Poll::Ready(Some(Ok(Frame::data(chunk))))
            }
            Poll::Ready(Some(Err(_))) => fail(state, SourceFailure::Source),
            Poll::Ready(None) => fail(state, SourceFailure::Shorter),
        }
    }

    fn is_end_stream(&self) -> bool {
        false
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.declared)
    }
}

enum Source {
    Buffered(Bytes),
    Stream(SizedStream),
}

struct Meta {
    name: FileName,
    content_type: ContentType,
    sha1: Sha1Hex,
    length: u64,
    last_modified_millis: Option<u64>,
}

/// One file to upload: its name, `Content-Type`, exact length and SHA-1, and its bytes.
///
/// [`bytes`](Self::bytes) holds the content in memory (another attempt can resend it);
/// [`stream`](Self::stream) reads a one-shot source once. A one-shot file is moved into
/// [`Session::upload_file`] and cannot be sent again:
///
/// ```compile_fail,E0382
/// # async fn demo(session: concord_sdk_backblaze_b2_native::Session,
/// #     target: &mut concord_sdk_backblaze_b2_native::UploadTarget,
/// #     file: concord_sdk_backblaze_b2_native::UploadFile) {
/// use concord_sdk_backblaze_b2_native::UploadPolicy;
/// let first = session.upload_file(target, file, UploadPolicy::once()).await;
/// let again = session.upload_file(target, file, UploadPolicy::once()).await; // moved
/// # }
/// ```
pub struct UploadFile {
    meta: Meta,
    source: Source,
}

impl UploadFile {
    /// A file held in memory; its length and SHA-1 are computed here.
    pub fn bytes(name: FileName, content_type: ContentType, data: impl Into<Bytes>) -> Self {
        let data = data.into();
        let meta = Meta {
            sha1: Sha1Hex::of(&data),
            length: data.len() as u64,
            name,
            content_type,
            last_modified_millis: None,
        };
        Self {
            meta,
            source: Source::Buffered(data),
        }
    }

    /// A one-shot source of exactly `length` bytes whose SHA-1 the caller already knows (B2
    /// checks it; this SDK cannot without reading the source first). Exactly `length` bytes are
    /// sent and the source is not polled once they were handed out; a source that ends early or
    /// fails fails the transfer before the body is complete.
    ///
    /// # Errors
    ///
    /// [`InputError::EmptyStream`] for `length == 0` (use [`bytes`](Self::bytes)).
    pub fn stream<S, E>(
        name: FileName,
        content_type: ContentType,
        length: u64,
        sha1: Sha1Hex,
        source: S,
    ) -> Result<Self, InputError>
    where
        S: Stream<Item = Result<Bytes, E>> + Send + 'static,
        E: Into<BoxError> + 'static,
    {
        if length == 0 {
            return Err(InputError::EmptyStream);
        }
        let stream = SizedStream {
            declared: length,
            state: Mutex::new(StreamState {
                source: Box::pin(Boxed(Box::pin(source))),
                sent: 0,
                ended: false,
            }),
            tally: Arc::default(),
        };
        let meta = Meta {
            name,
            content_type,
            sha1,
            length,
            last_modified_millis: None,
        };
        Ok(Self {
            meta,
            source: Source::Stream(stream),
        })
    }

    /// Sends `X-Bz-Info-src_last_modified_millis` (milliseconds since the epoch).
    #[must_use]
    pub fn with_last_modified_millis(mut self, millis: u64) -> Self {
        self.meta.last_modified_millis = Some(millis);
        self
    }

    /// The file name.
    pub fn name(&self) -> &FileName {
        &self.meta.name
    }

    /// The exact length sent as `Content-Length`.
    pub fn length(&self) -> u64 {
        self.meta.length
    }

    /// The SHA-1 sent as `X-Bz-Content-Sha1`.
    pub fn sha1(&self) -> &Sha1Hex {
        &self.meta.sha1
    }

    /// Whether another attempt can resend the bytes (held in memory).
    pub fn is_replayable(&self) -> bool {
        matches!(self.source, Source::Buffered(_))
    }
}

impl fmt::Debug for UploadFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadFile")
            .field("name", &self.meta.name)
            .field("content_type", &self.meta.content_type)
            .field("length", &self.meta.length)
            .field("sha1", &self.meta.sha1)
            .field("replayable", &self.is_replayable())
            .finish_non_exhaustive()
    }
}

/// How many attempts an upload may make, and whether the caller permits sending the same file
/// again after an attempt that **may have stored it**.
///
/// A retry always goes to a new upload URL. It cannot replay a one-shot stream (such a file
/// always gets [`Replay::Never`]), and it does not prove that an earlier attempt stored nothing:
/// with [`Replay::Permitted`] the caller accepts that B2 may end up with more than one version of
/// the file, each reported in [`UploadReport`]. Retried outcomes: no answer, `503`,
/// `401 bad_auth_token`/`expired_auth_token` (the upload token), `400 auth_token_limit`,
/// `408 request_timeout`, and a failed `b2_get_upload_url` (`503` or no answer). Everything else
/// stops.
#[derive(Clone, Copy, Debug)]
pub struct UploadPolicy {
    /// Attempt count, total deadline and backoff (`concord-retry`).
    pub retry: Policy,
    /// The caller's permission to send again.
    pub replay: Replay,
}

impl UploadPolicy {
    /// One attempt, no replay: the default.
    pub fn once() -> Self {
        Self {
            retry: Policy::new(NonZeroUsize::MIN),
            replay: Replay::Never,
        }
    }

    /// Up to `max_attempts` attempts, with replay permitted (no deadline, no backoff: set
    /// [`retry`](Self::retry)'s fields for those).
    pub fn retrying(max_attempts: NonZeroUsize) -> Self {
        Self {
            retry: Policy::new(max_attempts),
            replay: Replay::Permitted,
        }
    }
}

impl Default for UploadPolicy {
    fn default() -> Self {
        Self::once()
    }
}

/// What one attempt proves about the file being stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Evidence {
    /// `2xx` with a file version matching the name, bucket, length and SHA-1 sent.
    Stored,
    /// `2xx`, but its body could not be read or does not match what was sent: **probably
    /// stored**, unverified. Never retried.
    StoredUnverified,
    /// `4xx`: B2 refused this upload.
    Refused,
    /// `5xx` (`503 service_unavailable` included): B2 failed; whether it stored the file is not
    /// proven either way.
    ServerFailed,
    /// No answer arrived (timeout, reset, closed connection): the request may have reached B2
    /// and been stored.
    NoAnswer,
    /// The one-shot source ended early or failed, so the body never reached its declared
    /// `Content-Length`: B2 cannot have received the declared file.
    SourceFailed,
    /// Abandoned in flight at the policy's deadline: not proven either way.
    Abandoned,
    /// The upload request was not built (nothing sent).
    NotSent,
    /// No upload URL could be obtained (`b2_get_upload_url` failed): no upload request.
    NoUploadUrl,
}

impl Evidence {
    /// Whether this attempt may have left a stored version of the file.
    pub fn may_have_stored(self) -> bool {
        matches!(
            self,
            Self::Stored
                | Self::StoredUnverified
                | Self::ServerFailed
                | Self::NoAnswer
                | Self::Abandoned
        )
    }

    /// Whether this attempt sent an upload request (or may have).
    pub fn sent_upload(self) -> bool {
        !matches!(self, Self::NotSent | Self::NoUploadUrl)
    }
}

/// One attempt of an upload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttemptRecord {
    evidence: Evidence,
    status: Option<u16>,
    code: Option<ErrorCode>,
    fresh_upload_url: bool,
}

impl AttemptRecord {
    /// What the attempt proves.
    pub fn evidence(&self) -> Evidence {
        self.evidence
    }

    /// The HTTP status of its answer (of `b2_get_upload_url` for [`Evidence::NoUploadUrl`]).
    pub fn status(&self) -> Option<u16> {
        self.status
    }

    /// The B2 error code of its answer.
    pub fn code(&self) -> Option<&ErrorCode> {
        self.code.as_ref()
    }

    /// Whether it obtained a new upload URL first.
    pub fn fresh_upload_url(&self) -> bool {
        self.fresh_upload_url
    }
}

/// Every attempt of one [`Session::upload_file`] call, in order, and the retry accounting.
#[derive(Clone, Debug)]
pub struct UploadReport {
    attempts: Vec<AttemptRecord>,
    retry: concord_retry::Report,
}

impl UploadReport {
    /// The attempts, first to last.
    pub fn attempts(&self) -> &[AttemptRecord] {
        &self.attempts
    }

    /// Attempt count, waits and why repetition stopped.
    pub fn retry(&self) -> &concord_retry::Report {
        &self.retry
    }

    /// Attempts before the last one that may have stored the file. Each may be a separate
    /// stored version: a later success or a new upload URL does not prove otherwise.
    pub fn unresolved_earlier_attempts(&self) -> usize {
        let earlier = self.attempts.len().saturating_sub(1);
        self.attempts[..earlier]
            .iter()
            .filter(|a| a.evidence.may_have_stored())
            .count()
    }

    /// Attempts that sent (or may have sent) an upload request.
    pub fn upload_requests(&self) -> usize {
        self.attempts
            .iter()
            .filter(|a| a.evidence.sent_upload())
            .count()
    }
}

/// A stored and verified upload, with every attempt it took.
#[derive(Clone, Debug)]
pub struct Uploaded {
    /// The stored version: name, bucket, length and SHA-1 matched what was sent.
    pub file: FileVersion,
    /// The attempts.
    pub report: UploadReport,
}

impl Uploaded {
    /// Earlier attempts that may also have stored the file (other versions B2 may hold). Zero
    /// does not mean exactly once was proven beyond these attempts; it means none of them is
    /// unresolved.
    pub fn possible_duplicates(&self) -> usize {
        self.report.unresolved_earlier_attempts()
    }
}

struct Failure {
    error: Error,
    report: UploadReport,
    file: Option<UploadFile>,
    stream_consumed: bool,
}

/// A failed upload: the last attempt's error, every attempt's evidence, and the file when it
/// can still be sent.
pub struct UploadError(Box<Failure>);

impl UploadError {
    /// The last attempt's failure ([`Error::Deadline`] when it was abandoned).
    pub fn error(&self) -> &Error {
        &self.0.error
    }

    /// Every attempt.
    pub fn report(&self) -> &UploadReport {
        &self.0.report
    }

    /// Whether any attempt may have stored the file. Obtaining a new upload URL does not change
    /// this; only B2's listing of the bucket can resolve it (outside this slice).
    pub fn may_have_stored(&self) -> bool {
        self.0
            .report
            .attempts
            .iter()
            .any(|a| a.evidence.may_have_stored())
    }

    /// Whether a one-shot source was handed to the transport: it cannot be sent again, whatever
    /// upload URL is used next. A new attempt needs a new source.
    pub fn stream_consumed(&self) -> bool {
        self.0.stream_consumed
    }

    /// Why repetition stopped (`NotReplayable` when another attempt was warranted but not
    /// permitted or not possible).
    pub fn stop(&self) -> StopReason {
        self.0.report.retry.stop
    }

    /// The file, when it can still be sent: bytes held in memory always, a one-shot source only
    /// if no attempt handed it to the transport.
    pub fn into_file(self) -> Option<UploadFile> {
        self.0.file
    }

    /// The last attempt's failure.
    pub fn into_error(self) -> Error {
        self.0.error
    }
}

impl fmt::Debug for UploadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadError")
            .field("error", &self.0.error)
            .field("report", &self.0.report)
            .field("stream_consumed", &self.0.stream_consumed)
            .field("file_returned", &self.0.file.is_some())
            .finish()
    }
}

impl fmt::Display for UploadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let attempts = self.0.report.attempts.len();
        write!(
            f,
            "B2 upload failed after {attempts} attempt(s): {}",
            self.0.error
        )?;
        if self.may_have_stored() {
            f.write_str("; an attempt may have stored the file")?;
        }
        if self.0.stream_consumed {
            f.write_str("; the one-shot source was consumed")?;
        }
        Ok(())
    }
}

impl StdError for UploadError {}

/// State shared by the attempts of one upload (never locked across an await).
struct Shared {
    current: Option<UploadUrl>,
    source: Option<Source>,
    records: Vec<AttemptRecord>,
    stream_consumed: bool,
}

impl Shared {
    /// The next attempt's body: a cheap copy of bytes held in memory, or the one-shot stream
    /// (taken: it is gone afterwards). Wrapping a stream does not poll it.
    fn next_body(&mut self) -> Option<(reqwest::Body, Option<Arc<Tally>>)> {
        match self.source.take()? {
            Source::Buffered(bytes) => {
                self.source = Some(Source::Buffered(bytes.clone()));
                Some((reqwest::Body::from(bytes), None))
            }
            Source::Stream(stream) => {
                self.stream_consumed = true;
                let tally = Arc::clone(&stream.tally);
                Some((reqwest::Body::wrap(stream), Some(tally)))
            }
        }
    }
}

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One attempt's outcome, kept for the classifier and the final result.
struct Attempt {
    result: Result<FileVersion, Error>,
    decision: Decision,
}

fn record(
    shared: &Mutex<Shared>,
    evidence: Evidence,
    fresh_upload_url: bool,
    result: &Result<FileVersion, Error>,
) {
    let (status, code) = match result {
        Ok(_) => (None, None),
        Err(error) => (error.status().map(|s| s.as_u16()), error.code().cloned()),
    };
    lock(shared).records.push(AttemptRecord {
        evidence,
        status,
        code,
        fresh_upload_url,
    });
}

/// The documented outcomes worth another attempt at a new upload URL.
fn decide(evidence: Evidence, result: &Result<FileVersion, Error>) -> Decision {
    let retry = Decision::RetryAfter(None);
    let Err(error) = result else {
        return Decision::Done;
    };
    match evidence {
        Evidence::NoAnswer => retry,
        Evidence::ServerFailed if error.status() == Some(StatusCode::SERVICE_UNAVAILABLE) => retry,
        Evidence::Refused => match error.code() {
            Some(
                ErrorCode::BadAuthToken
                | ErrorCode::ExpiredAuthToken
                | ErrorCode::AuthTokenLimit
                | ErrorCode::RequestTimeout,
            ) => retry,
            _ => Decision::Stop,
        },
        Evidence::NoUploadUrl => match error {
            Error::Provider(e) if e.status() == StatusCode::SERVICE_UNAVAILABLE => retry,
            Error::Concord(e) if e.kind() == ErrorKind::Transport && e.head().is_none() => retry,
            _ => Decision::Stop,
        },
        _ => Decision::Stop,
    }
}

impl Session {
    /// Uploads one file to `target`'s bucket with `b2_upload_file`: its exact length as
    /// `Content-Length`, its SHA-1 as `X-Bz-Content-Sha1`, its percent-encoded name as
    /// `X-Bz-File-Name`.
    /// The prepared headers are `authorization` (the upload token, verbatim),
    /// `x-bz-file-name` (the encoded name), `x-bz-content-sha1` (the SHA-1 hex text), and,
    /// when supplied, `x-bz-info-src_last_modified_millis` (decimal milliseconds).
    /// The declared `Encoded` request format preserves the file's runtime `content-type`;
    /// the declared `Json` output adds `accept: application/json` even for this binary upload.
    ///
    /// The first attempt uses the target's upload URL, or obtains one if the target is retired.
    /// An attempt that is not stored retires the URL (B2: valid "until the endpoint rejects an
    /// upload"); with [`UploadPolicy::retrying`] a warranted retry obtains a new one. On return
    /// `target` holds the URL still usable, if any.
    ///
    /// Each attempt's [`Evidence`] is reported. A success says how many earlier attempts may
    /// also have stored the file; a failure says whether any attempt may have, and whether a
    /// one-shot source is spent. Nothing here claims exactly-once storage.
    ///
    /// # Errors
    ///
    /// [`UploadError`], carrying the last attempt's [`Error`].
    pub async fn upload_file(
        &self,
        target: &mut UploadTarget,
        file: UploadFile,
        policy: UploadPolicy,
    ) -> Result<Uploaded, UploadError> {
        let replay = if file.is_replayable() {
            policy.replay
        } else {
            Replay::Never
        };
        let UploadFile { meta, source } = file;
        let shared = Mutex::new(Shared {
            current: target.current.take(),
            source: Some(source),
            records: Vec::new(),
            stream_consumed: false,
        });
        let run = {
            let (shared, meta, bucket) = (&shared, &meta, &target.bucket);
            Retry::new(policy.retry, replay)
                .run_tokio(
                    move |_| self.attempt(bucket, meta, shared),
                    |attempt: &Attempt| attempt.decision,
                    |_, ceiling| ceiling,
                )
                .await
        };
        let Shared {
            current,
            source,
            mut records,
            stream_consumed,
        } = shared.into_inner().unwrap_or_else(PoisonError::into_inner);
        target.current = current;
        let abandoned = run.report.attempts > records.len();
        if abandoned {
            records.push(AttemptRecord {
                evidence: Evidence::Abandoned,
                status: None,
                code: None,
                fresh_upload_url: false,
            });
        }
        let report = UploadReport {
            attempts: records,
            retry: run.report,
        };
        let error = match run.outcome {
            Some(Attempt {
                result: Ok(file), ..
            }) if !abandoned => {
                return Ok(Uploaded { file, report });
            }
            Some(Attempt {
                result: Err(error), ..
            }) if !abandoned => error,
            _ => Error::Deadline,
        };
        let file = source.map(|source| UploadFile { meta, source });
        Err(UploadError(Box::new(Failure {
            error,
            report,
            file,
            stream_consumed,
        })))
    }

    async fn attempt(&self, bucket: &BucketId, meta: &Meta, shared: &Mutex<Shared>) -> Attempt {
        let held = lock(shared).current.take();
        let (url, fresh) = match held {
            Some(url) => (url, false),
            None => match self.issue_upload_url(bucket).await {
                Ok(url) => (url, true),
                Err(error) => {
                    let result = Err(error);
                    record(shared, Evidence::NoUploadUrl, true, &result);
                    let decision = decide(Evidence::NoUploadUrl, &result);
                    return Attempt { result, decision };
                }
            },
        };
        let next = lock(shared).next_body();
        let (evidence, result) = match next {
            Some((body, tally)) => self.send_upload(bucket, &url, meta, body, tally).await,
            // Unreachable under the policy (a one-shot file never gets a second attempt).
            None => (
                Evidence::NotSent,
                Err(Error::Input(InputError::SourceConsumed)),
            ),
        };
        if evidence == Evidence::Stored {
            lock(shared).current = Some(url);
        }
        record(shared, evidence, fresh, &result);
        let decision = decide(evidence, &result);
        Attempt { result, decision }
    }

    fn prepare_upload(
        &self,
        url: &UploadUrl,
        meta: &Meta,
        body: reqwest::Body,
    ) -> Result<Prepared<FileVersionWire>, Error> {
        let api = self.wire.clone().with_base_url(url.origin())?;
        let prepared = api
            .upload(url.token.expose())
            .upload_file(
                meta.name.header_value(),
                meta.sha1.as_str(),
                EncodedBody::with_content_type(body, meta.content_type.header()),
            )
            .maybe_last_modified_millis(meta.last_modified_millis)
            .prepare()?;
        // The issued URL, path and query as B2 sent them; same origin as the bound base.
        Ok(prepared.follow(url.url.as_str())?)
    }

    async fn send_upload(
        &self,
        bucket: &BucketId,
        url: &UploadUrl,
        meta: &Meta,
        body: reqwest::Body,
        tally: Option<Arc<Tally>>,
    ) -> (Evidence, Result<FileVersion, Error>) {
        let prepared = match self.prepare_upload(url, meta, body) {
            Ok(prepared) => prepared,
            Err(error) => return (Evidence::NotSent, Err(error)),
        };
        let received = match prepared.receive().await {
            Ok(received) => received,
            Err(error) => {
                let evidence = match error.delivery() {
                    Delivery::NotSent => Evidence::NotSent,
                    _ if tally
                        .as_ref()
                        .is_some_and(|t| t.failed() && t.bytes() < meta.length) =>
                    {
                        Evidence::SourceFailed
                    }
                    _ => Evidence::NoAnswer,
                };
                return (evidence, Err(error.into()));
            }
        };
        let status = received.status();
        if !status.is_success() {
            let evidence = if status.is_server_error() {
                Evidence::ServerFailed
            } else {
                Evidence::Refused
            };
            return (evidence, Err(provider_error(received).await));
        }
        match received.decode_with_meta().await {
            Ok((wire, head)) => match stored(wire, bucket, meta) {
                Ok(file) => (Evidence::Stored, Ok(file)),
                Err(violation) => (
                    Evidence::StoredUnverified,
                    Err(Error::contract(violation, Some(head))),
                ),
            },
            Err(error) => (Evidence::StoredUnverified, Err(error.into())),
        }
    }
}

/// The 2xx document, checked against what was sent.
fn stored(wire: FileVersionWire, bucket: &BucketId, meta: &Meta) -> Result<FileVersion, Violation> {
    let mismatch = Violation::UploadedFileVersion;
    let length = wire.content_length.value().ok_or(mismatch)?;
    let sha1 = wire
        .content_sha1
        .as_deref()
        .and_then(|s| Sha1Hex::new(s).ok())
        .ok_or(mismatch)?;
    let file_id = wire
        .file_id
        .and_then(|id| FileId::new(id).ok())
        .ok_or(mismatch)?;
    if wire.action != "upload"
        || wire.file_name != meta.name.as_str()
        || wire.bucket_id != bucket.as_str()
        || length != meta.length
        || sha1 != meta.sha1
    {
        return Err(mismatch);
    }
    Ok(FileVersion {
        file_id,
        file_name: wire.file_name,
        bucket_id: bucket.clone(),
        account_id: wire.account_id,
        action: wire.action,
        content_length: length,
        content_sha1: sha1,
        content_md5: wire.content_md5,
        content_type: wire.content_type,
        file_info: wire.file_info,
        upload_timestamp: wire.upload_timestamp,
    })
}
