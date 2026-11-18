// Included in each client module. Endpoint-specific request, reader, and pager
// implementations stay with the authored description.

use concord::__private::{self, ConditionalAccess, Options, Prepared};
use concord::reqwest::{Request, Response, Url};
use concord::transport::TryInfo;
use concord::{Answer, Pages, Sending};
use std::fmt;
use std::future::{Future, IntoFuture};
use std::time::Duration;

pub(super) mod __spec {
    use super::*;

    pub trait Spec {
        type Inputs: Clone + Send + Sync + 'static;
        const DEBUG: &'static str;

        fn request(call: &Call<Self>, take: bool) -> Result<Request>
        where
            Self: Endpoint;

        fn prepare(call: Call<Self>) -> Result<Prepared<<Self as Endpoint>::Value, __Problem>>
        where
            Self: Endpoint;

        fn read(
            call: &Call<Self>,
            response: Response,
            info: TryInfo,
        ) -> impl Future<Output = Result<Answer<<Self as Endpoint>::Value>>> + Send
        where
            Self: Endpoint;
    }

    pub trait Awaitable: Endpoint {}
    pub trait Conditional: Endpoint {}
    pub trait Paged: Endpoint {
        fn pages(call: Call<Self>) -> Pages<Self::Value, __Problem>;
    }
}

/// An endpoint of this client. Each endpoint has its own type and no value.
/// Only this client can implement the trait.
pub trait Endpoint: __spec::Spec + Sized + Send + Sync + 'static {
    /// The value of a successful send. For a paged endpoint, this is one page.
    type Value: Send + 'static;

    /// The endpoint's stable name, the one `TryInfo::endpoint()` reports: the module of the
    /// client, its scopes and the endpoint, joined with `.` (`riot.matches.ids`).
    ///
    /// Use it as a key for limits and in logs.
    const NAME: &'static str;
}

/// A call to endpoint `E` of this client.
#[must_use]
pub struct Call<E: Endpoint> {
    pub(super) __inputs: E::Inputs,
    pub(super) __options: Options<__Problem>,
}

impl<E: Endpoint> Call<E> {
    pub(super) fn __new(inputs: E::Inputs) -> Self {
        Self {
            __inputs: inputs,
            __options: Options::default(),
        }
    }

    /// Send the call. For a paged endpoint, returns one page.
    ///
    /// # Panics
    ///
    /// Polling requires a Tokio runtime with its time and IO drivers enabled.
    pub fn send(self) -> __Sending<E::Value> {
        Sending::new(async move { self.send_full().await.map(Answer::into_value) })
    }

    /// Send the call, retaining the value, status and headers.
    pub fn send_full(self) -> __Sending<Answer<E::Value>> {
        __private::Call::send_full(self)
    }

    /// Send the call and return the response at the head, before reading its body.
    pub fn send_raw(self) -> __Sending<Response> {
        Sending::new(async move { E::prepare(self)?.send_raw().await })
    }

    /// Set the retry policy for this call.
    pub fn with_retry(mut self, retry: Retry) -> Self {
        self.__options.with_retry(retry);
        self
    }

    /// Set the timeout for the whole call, including retries and reading.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.__options.with_timeout(timeout);
        self
    }

    /// Set the maximum silence between transport or body events.
    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.__options.with_idle_timeout(timeout);
        self
    }

    /// Inspect the request before middleware, without taking a one-shot body.
    pub fn request(&self) -> Result<Request> {
        E::request(self, false)
    }

    /// Build the request before middleware, taking any one-shot body.
    pub fn into_request(self) -> Result<Request> {
        E::request(&self, true)
    }

    /// Edit each built request before middleware, in the order edits were added.
    pub fn edit_request(mut self, edit: impl Fn(&mut Request) + Send + Sync + 'static) -> Self {
        self.__options.edit_request(edit);
        self
    }

    /// Read a supplied response using the endpoint's reader and status rules.
    #[allow(clippy::manual_async_fn)] // The public signature explicitly promises Send.
    pub fn read_response(
        &self,
        response: Response,
    ) -> impl Future<Output = Result<E::Value>> + Send + '_ {
        async move {
            E::read(self, response, TryInfo::new(E::NAME, 0))
                .await
                .map(Answer::into_value)
        }
    }
}

impl<E: __spec::Conditional> Call<E> {
    /// Send a conditional GET or HEAD with the supplied ETag.
    pub fn send_if_none_match(self, etag: &str) -> __Sending<Answer<Option<E::Value>>> {
        let etag = etag.to_owned();
        Sending::new(async move { self.__prepare_if_none_match(&etag)?.send().await })
    }

    #[doc(hidden)]
    pub fn __prepare_if_none_match(
        self,
        etag: &str,
    ) -> Result<Prepared<Option<E::Value>, __Problem>> {
        __private::prepare_conditional(self, etag)
    }
}

impl<E: __spec::Paged> Call<E> {
    /// Iterate over all pages. No request is sent before the iteration starts.
    pub fn pages(self) -> __Pages<E::Value> {
        E::pages(self)
    }
}

/// Awaiting an unpaged call gives [`Result<E::Value>`](Result).
impl<E: __spec::Awaitable> IntoFuture for Call<E> {
    type Output = Result<E::Value>;
    type IntoFuture = __Sending<E::Value>;

    fn into_future(self) -> Self::IntoFuture {
        self.send()
    }
}

// A call is moved, never polled in place: no code takes a pin to a call or to one of
// its fields. So it is `Unpin` whatever its inputs hold, and an added input that is not
// `Unpin` never takes `Unpin` from a call.
impl<E: Endpoint> Unpin for Call<E> {}

// Every call of this client is `Unpin`, for any endpoint and so for any inputs: this
// compiles only because the implementation above holds for every `E`. Never called.
#[allow(dead_code)]
const _: () = {
    fn unpin<T: Unpin>() {}
    fn every_call<E: Endpoint>() {
        unpin::<Call<E>>();
    }
};

impl<E: Endpoint> Clone for Call<E> {
    fn clone(&self) -> Self {
        Self {
            __inputs: self.__inputs.clone(),
            __options: self.__options.clone(),
        }
    }
}

impl<E: Endpoint> fmt::Debug for Call<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(E::DEBUG)
            .field("endpoint", &E::NAME)
            .finish_non_exhaustive()
    }
}

#[doc(hidden)]
impl<E: Endpoint> concord::page::Call for Call<E> {
    type Value = E::Value;
    type Problem = __Problem;

    fn at_url(mut self, url: &str) -> Self {
        self.__options.at_url(__private::without_fragment(url));
        self
    }
}

#[doc(hidden)]
impl<E: Endpoint> __private::Call for Call<E> {
    fn request(&self) -> Result<Request> {
        self.request()
    }

    fn prepare(self) -> Result<Prepared<E::Value, __Problem>> {
        E::prepare(self)
    }

    fn answered_from(mut self, url: Url) -> Self {
        self.__options.answered_from(url);
        self
    }

    fn edit_request(self, edit: impl Fn(&mut Request) + Send + Sync + 'static) -> Self {
        self.edit_request(edit)
    }
}

#[doc(hidden)]
impl<E: __spec::Conditional> __private::ConditionalCall for Call<E> {
    fn set_if_none_match(&mut self, _: ConditionalAccess<Self>, etag: &str) {
        self.__options.if_none_match(etag);
    }
}
