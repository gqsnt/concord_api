use std::fmt;

use super::{Core, Idempotent, Options, Run, SecretUrl};
use crate::answer::Answer;
use crate::error::Error;
use crate::transport::TryInfo;
use crate::Sending;

/// What every generated call is, for the engine. The public part is [`crate::page::Call`].
pub trait Call: crate::page::Call {
    /// The request as the description builds it. The call is still whole.
    /// Refuses a one-shot body without taking it. No Run or description clock/mark
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

    /// Changes each request after it is built and before it enters the middleware chain.
    /// Header values this edit marks sensitive are omitted from page request comparison.
    /// Changing the URL discards its secret marks; its URL is then compared as it is.
    fn edit_request(self, edit: impl Fn(&mut reqwest::Request) + Send + Sync + 'static) -> Self;
}

/// A send between building and trying: pages inspect this exact request, then send it.
/// No request is rebuilt and no edit or description expression runs a second time.
pub struct Prepared<T, P> {
    /// The clocks started before description expressions and request construction.
    pub run: Run,
    /// The request finished for sending, before middleware.
    pub request: reqwest::Request,
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
            super::exchange(
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
    #[doc(hidden)]
    pub fn conditional(self) -> Prepared<Option<T>, P> {
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
            super::exchange_raw(
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

/// Common call methods. Expanded beside the API description, using Concord's crate re-exports.
/// Each send (including each page) starts its clock, then evaluates `idle_timeout`,
/// then `idempotent`, then builds the base, paths, query, headers and body in that order.
/// `read` runs for each answer, the pager once at `.pages()`, and middleware expressions
/// once in the client's `new`.
/// All sends prepare once before trying; raw sends use [`Prepared::send_raw`].
/// Only non-paged calls implement `IntoFuture`; paged calls use `.send().await`
/// for one page or `.pages()` for iteration.
///
/// The result link and the paging mode are supplied by the caller. Before the
/// optional `pages` block, `docs { method { #[doc = "..."] ... } ... }` replaces
/// the complete documentation of `send`, `read_response`, `with_idle_timeout`,
/// `request`, `into_request`, or `send_if_none_match`. The methods may appear in
/// any order; omitted methods keep the defaults used by the hand-written tour.
/// Overrides carry the endpoint's result links and repetition rules without
/// making this helper interpret the author's types or endpoint settings.
///
/// Reserved on calls: `send`, `send_full`, `send_raw`, `send_if_none_match`,
/// `request`, `into_request`, `edit_request`, `read_response`, `pages`, `at_url`,
/// `into_future`, `clone`. On clients and handles: `call`, `clone`; on clients also
/// `new`. In a client's module: `Error`, `Result`. The prefix `with_` is reserved;
/// there is no reserved `send_` prefix. No new receiver method without `with_`
/// will be added in a minor release.
#[doc(hidden)]
#[macro_export]
macro_rules! call_methods {
    ($call:ty, $value:ty, (), $($result:ident)::+, $await_type:literal, $future:ident, $conditional:ident
        $(, docs { $($doc_method:ident { $($doc_attrs:tt)* })* })?
        $(, pages { $($pages:tt)* })? $(,)?) => {
        $crate::call_methods!(@impl $call, $value, (), [], $($result)::+, $await_type, $future, $conditional
            , docs { $($($doc_method { $($doc_attrs)* })*)? }
            $(, pages { $($pages)* })?);
    };
    ($call:ty, $value:ty, $problem:ty, $($result:ident)::+, $await_type:literal, $future:ident, $conditional:ident
        $(, docs { $($doc_method:ident { $($doc_attrs:tt)* })* })?
        $(, pages { $($pages:tt)* })? $(,)?) => {
        $crate::call_methods!(@impl $call, $value, $problem, [$problem], $($result)::+, $await_type, $future, $conditional
            , docs { $($($doc_method { $($doc_attrs)* })*)? }
            $(, pages { $($pages)* })?);
    };
    (@impl $call:ty, $value:ty, $problem:ty, [$($shown_problem:ty)?], $($result:ident)::+, $await_type:literal, $future:ident, $conditional:ident
        , docs { $($doc_method:ident { $($doc_attrs:tt)* })* }
        $(, pages { $($pages:tt)* })? $(,)?) => {
        $($crate::call_methods!(@doc_method $doc_method);)*
        // Identifier patterns otherwise resolve same-named caller constants,
        // even with macro_rules mixed-site hygiene. Private function items
        // shield the helper's bindings; authored page methods stay outside.
        // PANIC-SAFETY: Private placeholder items shield binding names; no panic lint is relaxed.
        #[allow(dead_code)]
        const _: () = {
        fn __response() {}
        fn __retry() {}
        fn __timeout() {}
        fn __edit() {}
        fn __url() {}
        fn __etag() {}
        /// Send.
        impl $call {
            $crate::call_methods!(@doc send, [$($doc_method { $($doc_attrs)* })*], {
            /// Send the call. On a paged endpoint this gives one page.
            /// Preparation starts on the first poll; only the value is kept.
            #[doc = ::core::concat!("`.send().await` gives ", $await_type, ".")]
            ///
            /// # Panics
            ///
            /// Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
            /// Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.
            pub fn send(self) -> $crate::Sending<$value $(, $shown_problem)?> {
                $crate::Sending::new(async move {
                    <$call as $crate::__private::Call>::send_full(self).await.map($crate::Answer::into_value)
                })
            }
            });

            /// Send the call, and keep the status and the headers beside the value.
            ///
            /// The value is in a [`concord::Answer`]. Use `value()` to borrow it,
            /// or `into_parts()` to take the status, headers, and value.
            /// Prepares once on the first poll, then sends that preparation.
            ///
            /// # Panics
            ///
            /// Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
            /// Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.
            pub fn send_full(self) -> $crate::Sending<$crate::Answer<$value> $(, $shown_problem)?> {
                <$call as $crate::__private::Call>::send_full(self)
            }

            /// Send the call and get the [`reqwest::Response`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Response.html) as it came.
            ///
            /// Concord passes heads at or above 400 to the retry rules. It does not
            /// apply the endpoint's status rules or read the body. On the first poll, prepares exactly as for
            /// `send_full()`, then sends that preparation only as far as the head.
            ///
            /// # Panics
            ///
            /// Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
            /// Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.
            pub fn send_raw(self) -> $crate::Sending<$crate::reqwest::Response $(, $shown_problem)?> {
                $crate::Sending::new(async move {
                    <$call as $crate::__private::Call>::prepare(self)?.send_raw().await
                })
            }
        }

        /// Settings for this call only. Each replaces the client's.
        impl $call {
            /// The retry policy for this call. Accepts a [`concord::Retry`].
            pub fn with_retry(mut self, __retry: $crate::Retry $(<$shown_problem>)?) -> Self {
                self.__options.with_retry(__retry);
                self
            }

            /// The longest the whole call may take, retries and reading included. No limit by default. Accepts a [`std::time::Duration`].
            pub fn with_timeout(mut self, __timeout: ::std::time::Duration) -> Self {
                self.__options.with_timeout(__timeout);
                self
            }

            $crate::call_methods!(@doc with_idle_timeout, [$($doc_method { $($doc_attrs)* })*], {
            /// The longest the transport or a body read may be silent. Accepts a [`std::time::Duration`].
            /// Precedence: this call, the endpoint's `idle_timeout`, the client setting, then 30 seconds.
            /// The endpoint's `idle_timeout` still runs once for every send, before
            /// `idempotent` and request construction, even when this call sets the length.
            pub fn with_idle_timeout(mut self, __timeout: ::std::time::Duration) -> Self {
                self.__options.with_idle_timeout(__timeout);
                self
            }
            });
        }

        /// Look at the request, or change it.
        impl $call {
            $crate::call_methods!(@doc read_response, [$($doc_method { $($doc_attrs)* })*], {
            /// Read an answer obtained yourself using this endpoint's reader and status rules.
            /// No retry or clock is applied, and pages are not advanced. Errors report zero tries.
            pub fn read_response(
                &self,
                __response: $crate::reqwest::Response,
            ) -> impl ::std::future::Future<Output = $($result)::+<$value>> + ::std::marker::Send + '_ {
                async move {
                    self.__read_answer(__response, $crate::transport::TryInfo::new(Self::__ENDPOINT, 0))
                        .await
                        .map($crate::Answer::into_value)
                }
            }
            });

            $crate::call_methods!(@doc request, [$($doc_method { $($doc_attrs)* })*], {
            /// The [`reqwest::Request`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Request.html) that this call would send.
            /// The call can still be sent. This is the request before any middleware (such as a signer).
            /// Refuses a one-shot body without taking it. No call clock starts and
            /// neither the endpoint's `idle_timeout` nor its `idempotent` runs.
            pub fn request(&self) -> $($result)::+<$crate::reqwest::Request> {
                self.__build_request(false)
            }
            });

            $crate::call_methods!(@doc into_request, [$($doc_method { $($doc_attrs)* })*], {
            /// The [`reqwest::Request`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Request.html) that this call would send, and the call is used up.
            ///
            /// Needed when the body is a stream that can be read only once.
            /// Takes that one-shot body. This is the request before any middleware.
            /// No call clock starts and neither `idle_timeout` nor `idempotent`
            /// from the endpoint runs.
            pub fn into_request(self) -> $($result)::+<$crate::reqwest::Request> {
                self.__build_request(true)
            }
            });

            /// Change the built request before it enters the middleware chain.
            ///
            /// The changed request is used on every try. Several changes run in the order added.
            /// A header value marked sensitive is left out of the same-request comparison
            /// for pages, including when this edit sets that flag.
            /// If an edit changes the URL, its secret marks are no longer known;
            /// page comparison uses that URL as it is, without its fragment.
            pub fn edit_request(
                mut self,
                __edit: impl ::std::ops::Fn(&mut $crate::reqwest::Request)
                    + ::std::marker::Send
                    + ::std::marker::Sync
                    + 'static,
            ) -> Self {
                self.__options.edit_request(__edit);
                self
            }
        }

        // Used by pagers. A caller does not need it, so it is not on the page of the call.
        #[doc(hidden)]
        impl $crate::page::Call for $call {
            type Value = $value;
            type Problem = $problem;

            fn at_url(mut self, __url: &::core::primitive::str) -> Self {
                self.__options.at_url($crate::__private::without_fragment(__url));
                self
            }
        }

        #[doc(hidden)]
        impl $crate::__private::Call for $call {
            fn request(&self) -> $($result)::+<$crate::reqwest::Request> {
                <$call>::request(self)
            }

            fn prepare(self) -> $($result)::+<$crate::__private::Prepared<$value, $problem>> {
                self.__prepare_inner()
            }

            fn answered_from(mut self, __url: $crate::reqwest::Url) -> Self {
                self.__options.answered_from(__url);
                self
            }

            fn edit_request(
                self,
                __edit: impl ::std::ops::Fn(&mut $crate::reqwest::Request)
                    + ::std::marker::Send
                    + ::std::marker::Sync
                    + 'static,
            ) -> Self {
                <$call>::edit_request(self, __edit)
            }
        }

        $crate::call_methods!(@future $future, $call, $value, $problem, [$($shown_problem)?]);
        $crate::call_methods!(@conditional $conditional, $call, $value, $problem, [$($shown_problem)?], $($result)::+
            , docs { $($doc_method { $($doc_attrs)* })* });
        };

        $(
            /// Every page. Authored methods retain only their caller's scope.
            impl $call {
                $($pages)*
            }
        )?
    };
    (@future paged, $call:ty, $value:ty, $problem:ty, [$($shown_problem:ty)?]) => {};
    (@future awaitable, $call:ty, $value:ty, $problem:ty, [$($shown_problem:ty)?]) => {
        impl ::std::future::IntoFuture for $call {
            type Output = ::core::result::Result<$value, $crate::Error<$problem>>;
            type IntoFuture = $crate::Sending<$value $(, $shown_problem)?>;

            fn into_future(self) -> Self::IntoFuture {
                self.send()
            }
        }
    };
    (@conditional unconditional, $call:ty, $value:ty, $problem:ty, [$($shown_problem:ty)?], $($result:ident)::+
        , docs { $($docs:tt)* }) => {};
    (@conditional conditional, $call:ty, $value:ty, $problem:ty, [$($shown_problem:ty)?], $($result:ident)::+
        , docs { $($docs:tt)* }) => {
        impl $call {
            /// First half of a conditional GET or HEAD, otherwise the same as `prepare`.
            ///
            /// Writes `If-None-Match` after the endpoint's headers, replacing any
            /// value they wrote, and before `finish_for_send` runs the `edit_request`
            /// edits. An edit that removes the header therefore removes this one too.
            /// The retained reader gives `None` for a 304 that no status rule takes,
            /// and `Some` of what `prepare` would read for every other answer.
            #[doc(hidden)]
            pub fn __prepare_if_none_match(mut self, __etag: &::core::primitive::str)
                -> $($result)::+<$crate::__private::Prepared<::core::option::Option<$value>, $problem>>
            {
                self.__options.if_none_match(__etag);
                self.__prepare_inner().map($crate::__private::Prepared::conditional)
            }

            $crate::call_methods!(@doc send_if_none_match, [$($docs)*], {
            /// Send a conditional GET or HEAD with `If-None-Match`.
            ///
            /// The ETag is written as given: pass it as it arrived, quotes included
            /// (`"\"v1\""`, `W/"v1"`).
            ///
            /// Replaces any `If-None-Match` set for this endpoint before the
            /// `edit_request` edits run, so those edits can replace or remove it.
            /// An unclaimed 304 gives `None`; every other answer gives `Some(value)`.
            /// The status and headers are retained in both cases.
            ///
            /// # Panics
            ///
            /// Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
            /// Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.
            pub fn send_if_none_match(self, __etag: &::core::primitive::str)
                -> $crate::Sending<$crate::Answer<::core::option::Option<$value>> $(, $shown_problem)?>
            {
                let __etag = __etag.to_owned();
                $crate::Sending::new(async move {
                    self.__prepare_if_none_match(&__etag)?.send().await
                })
            }
            });
        }
    };
    (@doc_method send) => {};
    (@doc_method read_response) => {};
    (@doc_method with_idle_timeout) => {};
    (@doc_method request) => {};
    (@doc_method into_request) => {};
    (@doc_method send_if_none_match) => {};
    (@doc send, [send { $($docs:tt)* } $($rest:tt)*], { $($item:tt)* }) => {
        $crate::call_methods!(@replace_docs [$($docs)*] $($item)*);
    };
    (@doc read_response, [read_response { $($docs:tt)* } $($rest:tt)*], { $($item:tt)* }) => {
        $crate::call_methods!(@replace_docs [$($docs)*] $($item)*);
    };
    (@doc with_idle_timeout, [with_idle_timeout { $($docs:tt)* } $($rest:tt)*], { $($item:tt)* }) => {
        $crate::call_methods!(@replace_docs [$($docs)*] $($item)*);
    };
    (@doc request, [request { $($docs:tt)* } $($rest:tt)*], { $($item:tt)* }) => {
        $crate::call_methods!(@replace_docs [$($docs)*] $($item)*);
    };
    (@doc into_request, [into_request { $($docs:tt)* } $($rest:tt)*], { $($item:tt)* }) => {
        $crate::call_methods!(@replace_docs [$($docs)*] $($item)*);
    };
    (@doc send_if_none_match, [send_if_none_match { $($docs:tt)* } $($rest:tt)*], { $($item:tt)* }) => {
        $crate::call_methods!(@replace_docs [$($docs)*] $($item)*);
    };
    (@doc $method:ident, [$other:ident { $($docs:tt)* } $($rest:tt)*], { $($item:tt)* }) => {
        $crate::call_methods!(@doc $method, [$($rest)*], { $($item)* });
    };
    (@doc $method:ident, [], { $($item:tt)* }) => { $($item)* };
    // Strip the default attributes before attaching the complete override.
    (@replace_docs [$($docs:tt)*] $hash:tt [$($attribute:tt)*] $($rest:tt)*) => {
        $crate::call_methods!(@replace_docs [$($docs)*] $($rest)*);
    };
    (@replace_docs [$($docs:tt)*] $item:item) => { $($docs)* $item };
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

#[cfg(test)]
#[allow(dead_code, non_upper_case_globals)]
mod author_names {
    use super::{Core, Options, Prepared, Run};
    use crate::__private::{Draft, Idempotent, Piece};
    use crate::transport::TryInfo;

    // Every prelude name that generated code must qualify is occupied by the author.
    struct Option<T>(T);
    struct Result;
    struct Ok;
    struct Err;
    struct Some;
    struct None;
    struct String;
    struct Vec<T>(T);
    struct Box<T>(T);
    mod core {}
    mod std {}
    mod concord {}
    fn drop<T>(_: T) {
        panic!("a helper resolved the author's drop")
    }
    const __status: u8 = 1;
    const __response: u8 = 2;
    const __info: u8 = 3;
    const __retry: u8 = 4;
    const __timeout: u8 = 5;
    const __edit: u8 = 6;
    const __url: u8 = 7;
    const __etag: u8 = 8;
    // This must not intercept the helper's documentation macro either.
    #[allow(unused_macros)]
    macro_rules! concat {
        ($($tokens:tt)*) => {
            compile_error!("unqualified concat")
        };
    }
    type CallResult<T> = ::core::result::Result<T, crate::Error>;

    #[derive(Clone)]
    struct Get {
        core: Core<()>,
        __options: Options<()>,
    }

    impl Get {
        const __ENDPOINT: &'static str = "author.get";

        fn draft() -> Draft {
            let mut draft = Draft::new(http::Method::GET);
            draft.base(&[Piece::Lit("https://example.test/")]);
            draft
        }

        fn __build_request(&self, _: bool) -> CallResult<reqwest::Request> {
            Self::draft().finish(&self.core, &self.__options, Self::__ENDPOINT)
        }

        fn __prepare_inner(self) -> CallResult<Prepared<u32, ()>> {
            let run = Run::start(&self.core, &self.__options, Self::__ENDPOINT);
            Self::draft().prepare(
                run,
                self.core,
                self.__options,
                Idempotent::ByMethod,
                Self::read,
            )
        }

        async fn read(
            response: reqwest::Response,
            info: TryInfo,
        ) -> CallResult<crate::Answer<u32>> {
            if response.status() == 304 {
                return ::core::result::Result::Err(
                    crate::__private::refused_plain(response, &info).await,
                );
            }
            ::core::result::Result::Ok(crate::Answer::new(
                response.status(),
                response.headers().clone().into(),
                7,
            ))
        }

        async fn __read_answer(
            &self,
            response: reqwest::Response,
            info: TryInfo,
        ) -> CallResult<crate::Answer<u32>> {
            Self::read(response, info).await
        }
    }

    // All six overrides, deliberately out of declaration order, retain the
    // helper's hygiene and signatures, including the conditional and page paths.
    crate::call_methods!(Get, u32, (), CallResult, "[`u32`]", awaitable, conditional,
        docs {
            send_if_none_match { #[doc = "Send with a cache validator."] }
            request { #[doc = "Borrow the call to build its request."] }
            with_idle_timeout { #[doc = "Set the quiet-time limit."] }
            send {
                #[doc = "Send this GET."]
                #[doc = ::core::concat!("Gives ", "[`u32`].")]
            }
            into_request { #[doc = "Take the call to build its request."] }
            read_response { #[doc = "Read a supplied answer."] }
        },
        pages {
            fn author_constants() -> (u8, u8) { (__response, __etag) }
        }
    );

    #[tokio::test(start_paused = true)]
    async fn a51_call_methods_resolve_core_types_despite_author_prelude_names() {
        let _author_names = (Option(()), Result, Ok, Err, Some, None);
        for status in [200, 304] {
            let call = Get {
                core: Core::default().with_transport(crate::fake(move |request| {
                    assert_eq!(request.headers().get("if-none-match").unwrap(), "tag");
                    ::core::result::Result::Ok(crate::reply(status, ""))
                })),
                __options: Options::default(),
            };
            assert_eq!(
                call.request().unwrap().url().as_str(),
                "https://example.test/"
            );
            let answer = call.send_if_none_match("tag").await.unwrap();
            assert_eq!(answer.status(), status);
            assert_eq!(*answer.value(), (status == 200).then_some(7));
        }
        let call = Get {
            core: Core::default().with_transport(crate::fake(|_| {
                ::core::result::Result::Ok(crate::reply(200, ""))
            })),
            __options: Options::default(),
        };
        assert_eq!(call.read_response(crate::reply(200, "")).await.unwrap(), 7);
        assert!(call.clone().into_request().is_ok());
        assert_eq!(
            call.clone()
                .with_retry(crate::Retry::none())
                .with_timeout(::std::time::Duration::from_secs(5))
                .with_idle_timeout(::std::time::Duration::from_secs(1))
                .edit_request(|_| {})
                .send_full()
                .await
                .unwrap()
                .into_value(),
            7
        );
        assert_eq!(call.clone().send_raw().await.unwrap().status(), 200);
        assert_eq!(call.await.unwrap(), 7);
    }

    macro_rules! endpoint {
        ($call:ident, $method:ident, $value:ty, $decode:expr, $future:ident, $conditional:ident) => {
            #[derive(Clone)]
            struct $call {
                core: Core<()>,
                __options: Options<()>,
            }
            impl $call {
                const __ENDPOINT: &'static str = stringify!($call);
                fn draft() -> Draft {
                    let mut draft = Draft::new(crate::http::Method::$method);
                    draft.base(&[Piece::Lit("https://example.test/")]);
                    draft
                }
                fn new() -> Self {
                    Self {
                        core: Core::default().with_transport(crate::fake(|_| {
                            ::core::result::Result::Ok(crate::reply(200, "body"))
                        })),
                        __options: Options::default(),
                    }
                }
                fn __build_request(&self, _: bool) -> CallResult<crate::reqwest::Request> {
                    Self::draft().finish(&self.core, &self.__options, Self::__ENDPOINT)
                }
                fn __prepare_inner(self) -> CallResult<Prepared<$value, ()>> {
                    let run = Run::start(&self.core, &self.__options, Self::__ENDPOINT);
                    Self::draft().prepare(
                        run,
                        self.core,
                        self.__options,
                        Idempotent::ByMethod,
                        Self::read,
                    )
                }
                async fn read(
                    response: crate::reqwest::Response,
                    info: TryInfo,
                ) -> CallResult<crate::Answer<$value>> {
                    if !response.status().is_success() {
                        return ::core::result::Result::Err(
                            crate::__private::refused_plain(response, &info).await,
                        );
                    }
                    let status = response.status();
                    let headers = response.headers().clone().into();
                    let value: $value = ($decode)(crate::read::Incoming::new(response));
                    ::core::result::Result::Ok(crate::Answer::new(status, headers, value))
                }
                async fn __read_answer(
                    &self,
                    response: crate::reqwest::Response,
                    info: TryInfo,
                ) -> CallResult<crate::Answer<$value>> {
                    Self::read(response, info).await
                }
            }
            // An alias to () exercises the helper's non-unit-problem matcher too.
            crate::call_methods!(
                $call,
                $value,
                Problem,
                CallResult,
                "the call result",
                $future,
                $conditional
            );
        };
    }
    type Problem = ();
    endpoint!(Head, HEAD, (), |_| (), awaitable, conditional);
    endpoint!(Post, POST, u32, |_| 9, awaitable, unconditional);
    endpoint!(
        Paged,
        GET,
        ::std::vec::Vec<u32>,
        |_| ::std::vec![1, 2],
        paged,
        conditional
    );
    endpoint!(
        Download,
        GET,
        crate::ByteStream,
        |body: crate::read::Incoming| body.stream(),
        awaitable,
        conditional
    );

    #[tokio::test(start_paused = true)]
    async fn a51_a52_every_helper_is_hygienic_for_all_call_shapes() {
        assert_eq!(Get::author_constants(), (2, 8));
        assert_eq!(
            Head::new().request().unwrap().method(),
            crate::http::Method::HEAD
        );
        Head::new().send_if_none_match("tag").await.unwrap();
        Head::new().await.unwrap();
        assert_eq!(Post::new().await.unwrap(), 9);
        assert_eq!(Post::new().clone().send().await.unwrap(), 9);
        assert_eq!(Paged::new().send().await.unwrap(), ::std::vec![1, 2]);
        let mut pages =
            crate::__private::pages(Paged::new(), |_, _| ::core::option::Option::None::<Paged>);
        assert_eq!(
            pages.try_next().await.unwrap(),
            ::core::option::Option::Some(::std::vec![1, 2])
        );
        assert!(pages.try_next().await.unwrap().is_none());
        let mut body = Download::new().await.unwrap();
        assert_eq!(body.try_next().await.unwrap().unwrap(), "body");
        assert!(body.try_next().await.unwrap().is_none());
        let conditional = Download::new().send_if_none_match("tag").await.unwrap();
        let mut body = conditional.into_value().unwrap();
        assert_eq!(body.try_next().await.unwrap().unwrap(), "body");
    }
}
