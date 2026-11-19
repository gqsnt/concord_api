use std::fmt;

use super::{Core, Idempotent, Options, Run, SecretUrl};
use crate::answer::Answer;
use crate::error::Error;
use crate::transport::TryInfo;
use crate::Sending;

/// A finished request paired with its private repeatable body plan.
/// Only immutable access to the ordinary request is exposed, so edits cannot
/// leave a reconstruction plan attached to a different body.
pub struct PreparedRequest {
    pub(super) request: reqwest::Request,
    pub(super) repeat: Option<(reqwest::Request, crate::Body, Option<u64>)>,
    pub(super) control: Option<crate::body::repeatable::Control>,
}
impl std::ops::Deref for PreparedRequest {
    type Target = reqwest::Request;
    fn deref(&self) -> &Self::Target {
        &self.request
    }
}
impl fmt::Debug for PreparedRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedRequest").finish_non_exhaustive()
    }
}
impl PreparedRequest {
    pub(super) fn plain(request: reqwest::Request) -> Self {
        Self {
            request,
            repeat: None,
            control: None,
        }
    }
    pub(super) fn comparison(&self) -> (Option<reqwest::Request>, Option<crate::Body>) {
        match &self.repeat {
            Some((head, body, _)) => (head.try_clone(), Some(body.clone())),
            None => (self.request.try_clone(), None),
        }
    }
}

/// What every generated call is, for the engine. The public part is [`crate::page::Call`].
pub trait Call: crate::page::Call {
    /// The request as the description builds it. The call is still whole.
    /// A wholly repeatable body stays unopened; a one-shot body is refused without
    /// taking it. No Run or description clock/mark
    /// expression is evaluated.
    fn request(&self) -> Result<reqwest::Request, Error<Self::Problem>>;

    /// Records the URL of the answer that the pager saw, on the call it returned.
    /// Stored in Options before preparation so a relative `at_url` is resolved
    /// before checking its origin and before running caller edits.
    fn answered_from(self, url: reqwest::Url) -> Self;

    /// First half of a send: start the Run, evaluate the description's
    /// `idle_timeout` (even with a call override), then `idempotent`, then build
    /// and finish the request for sending, including one-shot body claims and edits.
    /// The result retains secret marks and the code that reads each answer.
    /// Generated GET/HEAD calls also have a hidden inherent `__prepare_if_none_match`
    /// for conditional preparation, returning a `Prepared<Option<Self::Value>, Self::Problem>`.
    // Keep the generated-code interface's explicit associated value and problem types.
    // PANIC-SAFETY: The closure preserves the call, answer and problem types without boxing; no panic lint is relaxed.
    #[allow(clippy::type_complexity)]
    fn prepare(self) -> Result<Prepared<Self::Value, Self::Problem>, Error<Self::Problem>>;

    /// Prepare once, then send that preparation. Nothing runs before the first poll.
    fn send_full(self) -> Sending<Answer<Self::Value>, Self::Problem> {
        Sending::new(async move { self.prepare()?.send().await })
    }

    /// Prepare once, then send through the response head without reading its body.
    fn send_raw(self) -> Sending<reqwest::Response, Self::Problem> {
        Sending::new(async move { self.prepare()?.send_raw().await })
    }

    /// Changes each request after it is built and before it enters the middleware chain.
    /// Header values this edit marks sensitive are omitted from page request comparison.
    /// Changing the URL discards its secret marks; its URL is then compared as it is.
    /// A body containing repeatable sources is represented by an unreadable
    /// placeholder during edits. Edits must leave it alone; detected removal or
    /// replacement by a different kind of body is a Build error (see [`crate::Body::repeatable`]).
    fn edit_request(self, edit: impl Fn(&mut reqwest::Request) + Send + Sync + 'static) -> Self;
}

/// Access to the conditional settings of this exact call.
#[derive(Debug)]
pub struct ConditionalAccess<C>(std::marker::PhantomData<fn(C) -> C>);

/// The conditional preparation hook for a GET or HEAD call.
pub trait ConditionalCall: Call {
    /// Set the header on this call before building its request.
    fn set_if_none_match(&mut self, access: ConditionalAccess<Self>, etag: &str);

    /// Prepare and send the conditional call on its first poll.
    fn send_conditional(self, etag: &str) -> Sending<Answer<Option<Self::Value>>, Self::Problem> {
        let etag = etag.to_owned();
        Sending::new(async move { prepare_conditional(self, &etag)?.send().await })
    }
}

/// Prepare a conditional call, keeping its reader and response evidence.
// PANIC-SAFETY: The return type spells the call's value and problem types; no panic lint is relaxed.
#[allow(clippy::type_complexity)]
pub fn prepare_conditional<C: ConditionalCall>(
    mut call: C,
    etag: &str,
) -> Result<Prepared<Option<C::Value>, C::Problem>, Error<C::Problem>> {
    call.set_if_none_match(ConditionalAccess(std::marker::PhantomData), etag);
    call.prepare().map(Prepared::conditional)
}

/// A send between building and trying: pages inspect this exact request, then send it.
/// No request is rebuilt and no edit or description expression runs a second time.
pub struct Prepared<T, P> {
    /// The clocks started before description expressions and request construction.
    pub run: Run,
    /// The request finished for sending, before middleware.
    pub request: PreparedRequest,
    /// What secrets wrote, retained for page comparison after the origin check.
    pub secrets: RequestSecrets,
    /// The description's mark, evaluated before building the request.
    pub idempotent: Idempotent,
    pub(super) core: Core<P>,
    pub(super) options: Options<P>,
    pub(super) one_try:
        Box<dyn Fn(reqwest::Response, TryInfo) -> Sending<Answer<T>, P> + Send + Sync>,
}

impl<T, P> fmt::Debug for Prepared<T, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Neither the request nor its secret annotations belong in diagnostics.
        f.debug_struct("Prepared").finish_non_exhaustive()
    }
}

impl<T: Send + 'static, P: Send + Sync + 'static> Prepared<T, P> {
    /// Second half of a send: the tries, with the retained Run, request, mark and reader.
    /// Gives the same answer as [`Call::send_full`], without building again.
    pub fn send(self) -> Sending<Answer<T>, P> {
        Sending::new(async move {
            super::run::exchange_prepared(
                self.run,
                &self.core,
                &self.options,
                self.idempotent,
                self.request,
                self.one_try,
            )
            .await
        })
    }

    /// Use the generated reader's arms first, giving None only at its unclaimed 304 fallback.
    fn conditional(self) -> Prepared<Option<T>, P> {
        let read = self.one_try;
        Prepared {
            run: self.run,
            request: self.request,
            secrets: self.secrets,
            idempotent: self.idempotent,
            core: self.core,
            options: self.options,
            one_try: Box::new(move |response, info| {
                let status = response.status();
                let headers = crate::Headers::from(response.headers().clone());
                let url = response.url().clone();
                let fallback = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let reading = read(response, info.with_conditional(fallback.clone()));
                Sending::new(async move {
                    let result = reading.await;
                    if fallback.load(std::sync::atomic::Ordering::Relaxed) {
                        Ok(Answer::new(status, headers, None).with_response_url(url))
                    } else {
                        result.map(|answer| {
                            let url = answer.response_url.clone();
                            let (status, headers, value) = answer.into_parts();
                            let mut answer = Answer::new(status, headers, Some(value));
                            answer.response_url = url;
                            answer
                        })
                    }
                })
            }),
        }
    }

    /// Second half of a raw send: the tries end at the response head and give
    /// the `reqwest::Response`. Uses the retained Run, request and idempotent mark;
    /// retry still applies, but no reader, status arm or problem decoder runs.
    /// Nothing is rebuilt and no caller edit runs again.
    pub fn send_raw(self) -> Sending<reqwest::Response, P> {
        Sending::new(async move {
            super::run::exchange_raw_prepared(
                self.run,
                &self.core,
                &self.options,
                self.idempotent,
                self.request,
            )
            .await
        })
    }
}

/// Secret placements on one finished request. Header values also carry `sensitive`.
#[derive(Clone, Debug, Default)]
pub struct RequestSecrets {
    /// Whole secret query pairs, by key.
    pub query: Vec<&'static str>,
    /// Headers containing only secret values.
    pub headers: Vec<&'static str>,
    /// Host/path marks. `None` means an edit changed the URL: all URL marks,
    /// including query marks, are no longer known and the URL is compared as it is.
    pub url: Option<SecretUrl>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{Kind, MismatchFailure};
    use crate::Headers;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[tokio::test(start_paused = true)]
    /// Also proves C10.10.
    async fn c5_12_conditional_fallback_does_not_read_and_respects_claimed_304_arms() {
        for arm in 0..4 {
            let polled = Arc::new(AtomicUsize::new(0));
            let count = polled.clone();
            let core = Core::<String>::default().with_transport(crate::fake(move |_| {
                let count = count.clone();
                let body = futures_util::stream::poll_fn(move |_| {
                    count.fetch_add(1, Ordering::SeqCst);
                    std::task::Poll::Pending::<Option<Result<bytes::Bytes, std::io::Error>>>
                });
                Ok(crate::reply_with(
                    304,
                    &[("etag", "kept")],
                    reqwest::Body::wrap_stream(body),
                ))
            }));
            let options = Options::default();
            let run = Run::start(&core, &options, "contract.conditional");
            let mut draft = super::super::Draft::new(http::Method::GET);
            draft.base(&[super::super::Piece::Lit("https://example.invalid/")]);
            let prepared = draft
                .prepare(
                    run,
                    core,
                    options,
                    Idempotent::ByMethod,
                    move |response, info| async move {
                        let status = response.status();
                        let headers = Headers::from(response.headers().clone());
                        match arm {
                            0 => Err(super::super::refused_plain(response, &info).await),
                            1 => Ok(Answer::new(status, headers, 7u32)),
                            2 => Err(Error::refused(
                                info.endpoint(),
                                info.number(),
                                status,
                                headers,
                                Some("problem".to_owned()),
                            )),
                            _ => Err(Error::mismatch(
                                info.endpoint(),
                                info.number(),
                                status,
                                headers,
                                MismatchFailure::Arm,
                            )),
                        }
                    },
                )
                .unwrap()
                .conditional();
            let result = prepared.send().await;
            match arm {
                0 => {
                    let answer = result.unwrap();
                    assert_eq!(answer.value(), &None);
                    assert_eq!(answer.headers().get("etag"), Some("kept"));
                }
                1 => assert_eq!(result.unwrap().into_value(), Some(7)),
                2 => {
                    let error = result.unwrap_err();
                    assert_eq!(error.kind(), Kind::Refused);
                    assert_eq!(error.problem().map(String::as_str), Some("problem"));
                }
                _ => assert_eq!(result.unwrap_err().kind(), Kind::Mismatch),
            }
            assert_eq!(polled.load(Ordering::SeqCst), 0);
        }
    }
}

// Tests for the preparation and paging helpers used by generated calls.
#[cfg(test)]
#[path = "../../tests/support/call_helpers.rs"]
mod preparation_and_paging;
