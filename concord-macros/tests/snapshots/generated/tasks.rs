/// A client for creating, finding, and deleting tasks.
///
/// # Example
///
/// ```no_run
/// use tour::Tasks;
///
/// # async fn example() -> tour::tasks::Result<()> {
/// let tasks = Tasks::new();
/// let open = tasks.list().done(false).await?;
/// # Ok(())
/// # }
/// ```
///
/// A client for `tasks`.
pub struct Tasks {
    __core: ::concord::__private::Core<
        <tasks::Call<
            ::concord::__private::ClientKey,
        > as ::concord::__private::ClientProblem>::Problem,
    >,
}
///Calls and results for [`Tasks`].
pub mod tasks {
    type __Problem = <Call<
        ::concord::__private::ClientKey,
    > as ::concord::__private::ClientProblem>::Problem;
    ///The error of each call: [`concord::Error`].
    pub type Error = ::concord::Error;
    /// The result of a call, with this client's error.
    pub type Result<T> = ::core::result::Result<T, Error>;
    ///The retry policy for this client: [`concord::Retry`].
    pub type Retry = ::concord::Retry;
    mod __sealed {
        pub trait Sealed {}
    }
    #[allow(deprecated)]
    impl __sealed::Sealed for List {}
    #[allow(deprecated)]
    impl __sealed::Sealed for Create {}
    #[allow(deprecated)]
    impl __sealed::Sealed for Get {}
    #[allow(deprecated)]
    impl __sealed::Sealed for Delete {}
    impl __sealed::Sealed for ::concord::__private::ClientKey {}
    #[doc(hidden)]
    impl Endpoint for ::concord::__private::ClientKey {
        type Value = ::core::convert::Infallible;
        type Data = ::core::convert::Infallible;
        type Paging = ::concord::call::NoPaginate;
        type Conditional = ::concord::call::NotGetOrHead;
        const NAME: &'static ::core::primitive::str = "";
        const __DEBUG: &'static ::core::primitive::str = "";
        fn __core(value: &Self::Data) -> &::concord::__private::Core<__Problem> {
            match *value {}
        }
        fn __build_draft(value: &Self::Data) -> Result<::concord::__private::Draft> {
            match *value {}
        }
        async fn __read_answer(
            value: &Self::Data,
            _: ::concord::reqwest::Response,
            _: ::concord::transport::TryInfo,
        ) -> Result<::concord::Answer<Self::Value>> {
            match *value {}
        }
        fn __pages(
            _: Call<Self>,
            proof: ::core::convert::Infallible,
        ) -> ::concord::Pages<Self::Value, __Problem> {
            match proof {}
        }
    }
    /// The endpoint types of this client. The client supplies all implementations.
    ///
    /// Use `E: Endpoint` with `Call<E>` to write code that accepts any call of this client.
    pub trait Endpoint: __sealed::Sealed + ::core::marker::Sized + ::core::marker::Send + ::core::marker::Sync + 'static {
        /// The value of a successful send. For a paged endpoint, this is one page.
        type Value: ::core::marker::Send + 'static;
        #[doc(hidden)]
        type Data: ::core::clone::Clone
            + ::core::marker::Send
            + ::core::marker::Sync
            + 'static;
        #[doc(hidden)]
        type Paging: ::concord::call::PagingMode;
        #[doc(hidden)]
        type Conditional: ::concord::call::ConditionalMode;
        /// The endpoint's stable name, the one `TryInfo::endpoint()` reports: the module of the
        /// client, its scopes and the endpoint, joined with `.` (`riot.matches.ids`).
        ///
        /// Use it as a key for limits and in logs.
        const NAME: &'static ::core::primitive::str;
        #[doc(hidden)]
        const __DEBUG: &'static ::core::primitive::str;
        #[doc(hidden)]
        fn __core(__value: &Self::Data) -> &::concord::__private::Core<__Problem>;
        #[doc(hidden)]
        fn __settings(
            __value: &Self::Data,
            __draft: &mut ::concord::__private::Options<__Problem>,
        ) -> ::concord::__private::Idempotent {
            ::concord::__private::Idempotent::ByMethod
        }
        #[doc(hidden)]
        fn __build_draft(__value: &Self::Data) -> Result<::concord::__private::Draft>;
        #[doc(hidden)]
        fn __read_answer(
            __value: &Self::Data,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> impl ::core::future::Future<
            Output = Result<::concord::Answer<Self::Value>>,
        > + ::core::marker::Send;
        #[doc(hidden)]
        fn __pages(
            __call: Call<Self>,
            __proof: <Self::Paging as ::concord::call::PagingMode>::Proof,
        ) -> ::concord::Pages<Self::Value, __Problem>;
    }
    /// A call to the endpoint named by its type argument.
    ///
    /// The endpoint's page lists its request, inputs, options, and result.
    /// Use the endpoint name to find its options.
    #[must_use = "a call does nothing until sent or awaited"]
    pub struct Call<E: Endpoint> {
        __data: E::Data,
        __options: ::concord::__private::Options<__Problem>,
    }
    impl<E: Endpoint> ::core::marker::Unpin for Call<E> {}
    impl<E: Endpoint> ::core::clone::Clone for Call<E> {
        fn clone(&self) -> Self {
            Self {
                __data: ::core::clone::Clone::clone(&self.__data),
                __options: ::core::clone::Clone::clone(&self.__options),
            }
        }
    }
    impl<E: Endpoint> ::core::fmt::Debug for Call<E> {
        fn fmt(
            &self,
            formatter: &mut ::core::fmt::Formatter<'_>,
        ) -> ::core::fmt::Result {
            let mut debug = ::core::fmt::Formatter::debug_struct(
                formatter,
                <E as Endpoint>::__DEBUG,
            );
            ::core::fmt::DebugStruct::field(
                &mut debug,
                "endpoint",
                &<E as Endpoint>::NAME,
            );
            ::core::fmt::DebugStruct::finish_non_exhaustive(&mut debug)
        }
    }
    impl<E: Endpoint> Call<E> {
        #[doc(hidden)]
        pub(super) fn __new(data: E::Data) -> Self {
            Self {
                __data: data,
                __options: ::concord::__private::Options::default(),
            }
        }
        #[doc(hidden)]
        pub(super) fn __data(&self) -> &E::Data {
            &self.__data
        }
        #[doc(hidden)]
        pub(super) fn __data_mut(&mut self) -> &mut E::Data {
            &mut self.__data
        }
        fn __build_request(&self, take: bool) -> Result<::concord::reqwest::Request> {
            ::concord::__private::Draft::finish_request(
                <E as Endpoint>::__build_draft(&self.__data)?,
                <E as Endpoint>::__core(&self.__data),
                &self.__options,
                <E as Endpoint>::NAME,
                take,
            )
        }
        fn __prepare_inner(
            mut self,
        ) -> Result<::concord::__private::Prepared<E::Value, __Problem>> {
            let run = ::concord::__private::Run::start(
                <E as Endpoint>::__core(&self.__data),
                &self.__options,
                <E as Endpoint>::NAME,
            );
            let idempotent = <E as Endpoint>::__settings(
                &self.__data,
                &mut self.__options,
            );
            let draft = <E as Endpoint>::__build_draft(&self.__data)?;
            ::concord::__private::prepare_call(
                draft,
                run,
                ::core::clone::Clone::clone(<E as Endpoint>::__core(&self.__data)),
                ::core::clone::Clone::clone(&self.__options),
                idempotent,
                self,
                |call, response, info| async move {
                    <E as Endpoint>::__read_answer(&call.__data, response, info).await
                },
            )
        }
        /// The retry policy for this call. Accepts a [`concord::Retry`].
        pub fn with_retry(mut self, retry: self::Retry) -> Self {
            ::concord::__private::Options::with_retry(&mut self.__options, retry);
            self
        }
        /// The longest the whole call may take, retries and reading included. No limit by default. Accepts a [`std::time::Duration`].
        pub fn with_timeout(mut self, timeout: ::std::time::Duration) -> Self {
            ::concord::__private::Options::with_timeout(&mut self.__options, timeout);
            self
        }
        /// The longest the transport or a body read may be silent. Accepts a [`std::time::Duration`].
        /// Precedence: this call, the endpoint's `idle_timeout`, the client setting, then 30 seconds.
        /// The endpoint's `idle_timeout` still runs once for every send, before
        /// `idempotent` and request construction, even when this call sets the length.
        pub fn with_idle_timeout(mut self, timeout: ::std::time::Duration) -> Self {
            ::concord::__private::Options::with_idle_timeout(
                &mut self.__options,
                timeout,
            );
            self
        }
        /// Read a response that you obtained yourself using this endpoint's reader and status rules. No retry, call timeout, or idle timeout is started; pages are not advanced, and errors report zero attempts.
        pub fn read_response(
            &self,
            response: ::concord::reqwest::Response,
        ) -> impl ::core::future::Future<
            Output = Result<E::Value>,
        > + ::core::marker::Send + '_ {
            async move {
                ::core::result::Result::map(
                    <E as Endpoint>::__read_answer(
                            &self.__data,
                            response,
                            ::concord::transport::TryInfo::new(<E as Endpoint>::NAME, 0),
                        )
                        .await,
                    ::concord::Answer::into_value,
                )
            }
        }
        /// The [`reqwest::Request`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Request.html) that this call would send.
        /// The call can still be sent. This is the request before any middleware (such as a signer).
        /// Refuses a one-shot body without taking it; a wholly repeatable body stays unopened. The call timeout does not start, and neither the endpoint's `idle_timeout` nor its `idempotent` runs.
        pub fn request(&self) -> Result<::concord::reqwest::Request> {
            Self::__build_request(self, false)
        }
        /// The [`reqwest::Request`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Request.html) that this call would send, and the call is used up.
        ///
        /// Needed when the body is a stream that can be read only once.
        /// Takes that one-shot body. This is the request before any middleware.
        /// A repeatable body is exported as one lazy stream, without its Concord retry plan.
        /// The call timeout does not start, and neither `idle_timeout` nor `idempotent`
        /// from the endpoint runs.
        pub fn into_request(self) -> Result<::concord::reqwest::Request> {
            Self::__build_request(&self, true)
        }
        /// Change the built request before it enters the middleware chain.
        ///
        /// The changed request is used on every attempt. Several changes run in the order added.
        /// When the body contains a `Body::repeatable` source, an edit sees an unreadable placeholder;
        /// Reading it, removing it or a detected replacement is a Build error.
        /// Wrappers and look-alikes cannot always be detected; edits must leave the body alone.
        /// If building succeeds, each attempt receives the declared repeatable body as a fresh stream.
        /// A header value marked sensitive is left out of the same-request comparison
        /// for pages, including when this edit sets that flag.
        /// If an edit changes the URL, its secret marks are no longer known;
        /// page comparison uses that URL as it is, without its fragment.
        pub fn edit_request(
            mut self,
            edit: impl ::core::ops::Fn(
                &mut ::concord::reqwest::Request,
            ) + ::core::marker::Send + ::core::marker::Sync + 'static,
        ) -> Self {
            ::concord::__private::Options::edit_request(&mut self.__options, edit);
            self
        }
    }
    #[doc(hidden)]
    impl<E: Endpoint> ::concord::__private::ConditionalCall for Call<E>
    where
        E::Conditional: ::concord::call::IsGetOrHead,
    {
        fn set_if_none_match(
            &mut self,
            _: ::concord::__private::ConditionalAccess<Self>,
            etag: &::core::primitive::str,
        ) {
            ::concord::__private::Options::if_none_match(&mut self.__options, etag);
        }
    }
    #[doc(hidden)]
    impl<E: Endpoint> ::concord::page::Call for Call<E> {
        type Value = E::Value;
        type Problem = __Problem;
        fn at_url(mut self, url: &::core::primitive::str) -> Self {
            ::concord::__private::Options::at_url(
                &mut self.__options,
                ::concord::__private::without_fragment(url),
            );
            self
        }
    }
    #[doc(hidden)]
    impl<E: Endpoint> ::concord::page::NamedCall for Call<E> {
        const ENDPOINT: &'static ::core::primitive::str = E::NAME;
    }
    #[doc(hidden)]
    impl<E: Endpoint> ::concord::__private::Call for Call<E> {
        fn request(&self) -> Result<::concord::reqwest::Request> {
            Self::__build_request(self, false)
        }
        fn prepare(self) -> Result<::concord::__private::Prepared<E::Value, __Problem>> {
            Self::__prepare_inner(self)
        }
        fn answered_from(mut self, url: ::concord::reqwest::Url) -> Self {
            ::concord::__private::Options::answered_from(&mut self.__options, url);
            self
        }
        fn edit_request(
            self,
            edit: impl ::core::ops::Fn(
                &mut ::concord::reqwest::Request,
            ) + ::core::marker::Send + ::core::marker::Sync + 'static,
        ) -> Self {
            Self::edit_request(self, edit)
        }
    }
    /// List tasks in their current order.
    ///
    /// Its name in errors and logs: [`Endpoint::NAME`](super::tasks::Endpoint::NAME).
    ///
    /// Request: `GET https://tasks.example.com/api/tasks ?done&tags`.
    ///
    /// Call type: `tasks::Call<tasks::List>`, returned by `Tasks::list`.
    ///
    /// `List` names the endpoint and has no value.
    ///
    /// Options:
    ///
    /// Each option is a method of the call (`call.done(v)`) and a function of this type (`List::done(call, v)`).
    ///
    ///
    /// [`done(impl Arg<Option<bool>>)`](Self::done): Accepts a `bool`, a &`bool`, or an [`Option`]<`bool`>. `None` clears it.
    ///
    /// Filter tasks by completion state.
    ///
    /// [`tags(impl Arg<Option<Vec<String>>>)`](Self::tags): Accepts a `Vec`<`String`>, a &`Vec`<`String`>, or an [`Option`]<`Vec`<`String`>>. `None` clears it. Text and list conversions: [`concord::arg::Arg`](https://docs.rs/concord-api/5.0.0-rc.1/concord/arg/index.html).
    ///
    /// Keep only tasks that carry every one of these tags.
    ///
    /// `.await` returns [`tasks::Result`](super::tasks::Result)<`Vec`<[`Task`](super::Task)>>.
    ///
    /// Methods shared by every call: see [`Call`](super::tasks::Call).
    #[allow(missing_debug_implementations)]
    pub enum List {}
    /// Create a task with a title.
    ///
    /// Its name in errors and logs: [`Endpoint::NAME`](super::tasks::Endpoint::NAME).
    ///
    /// Request: `POST https://tasks.example.com/api/tasks`.
    ///
    /// Call type: `tasks::Call<tasks::Create>`, returned by `Tasks::create`.
    ///
    /// `Create` names the endpoint and has no value.
    ///
    /// Inputs:
    ///
    ///
    /// `task`: Accepts a [`NewTask`](super::NewTask) or a &[`NewTask`](super::NewTask).
    ///
    /// The task to create.
    ///
    /// A retry does not repeat this endpoint unless `Retry::not_applied_on` lists the status.
    ///
    /// `.await` returns [`tasks::Result`](super::tasks::Result)<[`Task`](super::Task)>.
    ///
    /// Methods shared by every call: see [`Call`](super::tasks::Call).
    #[allow(missing_debug_implementations)]
    pub enum Create {}
    /// Fetch one task by its identifier.
    ///
    /// Its name in errors and logs: [`Endpoint::NAME`](super::tasks::Endpoint::NAME).
    ///
    /// Request: `GET https://tasks.example.com/api/tasks/{id}`.
    ///
    /// Call type: `tasks::Call<tasks::Get>`, returned by `Tasks::get`.
    ///
    /// `Get` names the endpoint and has no value.
    ///
    /// Inputs:
    ///
    ///
    /// `id`: Accepts a `u64` or a &`u64`.
    ///
    /// The identifier of the task to fetch.
    ///
    /// `.await` returns [`tasks::Result`](super::tasks::Result)<[`Task`](super::Task)>.
    ///
    /// Methods shared by every call: see [`Call`](super::tasks::Call).
    #[allow(missing_debug_implementations)]
    pub enum Get {}
    /// Delete one task by its identifier.
    ///
    /// Its name in errors and logs: [`Endpoint::NAME`](super::tasks::Endpoint::NAME).
    ///
    /// Request: `DELETE https://tasks.example.com/api/tasks/{id}`.
    ///
    /// Call type: `tasks::Call<tasks::Delete>`, returned by `Tasks::delete`.
    ///
    /// `Delete` names the endpoint and has no value.
    ///
    /// Inputs:
    ///
    ///
    /// `id`: Accepts a `u64` or a &`u64`.
    ///
    /// The identifier of the task to delete.
    ///
    /// `.await` returns [`tasks::Result`](super::tasks::Result)<`()`>.
    ///
    /// Methods shared by every call: see [`Call`](super::tasks::Call).
    #[allow(missing_debug_implementations)]
    pub enum Delete {}
}
#[allow(
    unused_variables,
    unused_mut,
    unused_braces,
    dead_code,
    clippy::redundant_closure_call
)]
const _: () = {
    #[doc(hidden)]
    impl ::concord::__private::StoredType<0usize>
    for tasks::Call<::concord::__private::ClientKey> {
        type Value = Tasks;
    }
    const _: () = {
        #[doc(hidden)]
        pub struct __View {}
        #[allow(unused_imports)]
        use ::core::mem::drop as __fields;
        impl __View {
            #[doc(hidden)]
            pub fn new(__fields: ()) -> Self {
                Self {}
            }
        }
        #[doc(hidden)]
        impl ::concord::__private::OwnerView<0usize>
        for tasks::Call<::concord::__private::ClientKey> {
            type View<'__a> = __View;
        }
    };
    #[doc(hidden)]
    impl ::concord::__private::ClientProblem
    for tasks::Call<::concord::__private::ClientKey> {
        type Problem = ();
    }
    const _: () = {
        #[allow(unused_imports)]
        use ::core::mem::drop as etag;
        impl<E: tasks::Endpoint> tasks::Call<E>
        where
            E::Conditional: ::concord::call::IsGetOrHead,
        {
            #[doc(hidden)]
            pub fn __prepare_if_none_match(
                self,
                etag: &::core::primitive::str,
            ) -> tasks::Result<
                ::concord::__private::Prepared<
                    ::core::option::Option<E::Value>,
                    <tasks::Call<
                        ::concord::__private::ClientKey,
                    > as ::concord::__private::ClientProblem>::Problem,
                >,
            > {
                ::concord::__private::prepare_conditional(self, etag)
            }
            /// Send a conditional GET or HEAD with `If-None-Match`.
            ///
            /// The ETag is written as given: pass it as it arrived, quotes included
            /// (`"\"v1\""`, `W/"v1"`).
            ///
            /// Replaces any `If-None-Match` set for this endpoint before the
            /// `edit_request` edits run, so those edits can replace or remove it.
            /// On success: `None` for a 304 that no status arm matches, `Some(value)` otherwise.
            /// The status and headers are retained in both cases.
            ///
            /// # Panics
            ///
            /// Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
            /// Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.
            pub fn send_if_none_match(
                self,
                etag: &::core::primitive::str,
            ) -> ::concord::Sending<
                ::concord::Answer<::core::option::Option<E::Value>>,
            > {
                <Self as ::concord::__private::ConditionalCall>::send_conditional(
                    self,
                    etag,
                )
            }
        }
        impl<E: tasks::Endpoint> tasks::Call<E> {
            /// Send the call. On a paged endpoint this returns one page.
            /// Preparation starts on the first poll; only the value is kept.
            ///
            /// # Panics
            ///
            /// Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
            /// Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.
            pub fn send(self) -> ::concord::Sending<E::Value> {
                ::concord::Sending::new(async move {
                    ::core::result::Result::map(
                        <Self as ::concord::__private::Call>::send_full(self).await,
                        ::concord::Answer::into_value,
                    )
                })
            }
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
            pub fn send_full(self) -> ::concord::Sending<::concord::Answer<E::Value>> {
                <Self as ::concord::__private::Call>::send_full(self)
            }
            /// Send the call and get the [`reqwest::Response`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Response.html) as it came.
            ///
            /// Concord passes a response with a status of 400 or above to the retry rules. It does not
            /// apply the endpoint's status rules or read the body. On the first poll, prepares exactly as for
            /// `send_full()`, then sends that preparation only as far as the head.
            ///
            /// # Panics
            ///
            /// Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
            /// Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.
            pub fn send_raw(self) -> ::concord::Sending<::concord::reqwest::Response> {
                <Self as ::concord::__private::Call>::send_raw(self)
            }
        }
        impl<E: tasks::Endpoint> tasks::Call<E>
        where
            E::Paging: ::concord::call::DeclaresPaginate,
        {
            /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
            pub fn pages(self) -> ::concord::Pages<E::Value> {
                <E as tasks::Endpoint>::__pages(self, ())
            }
        }
    };
    ///Awaiting an unpaged call gives [`tasks::Result<E::Value>`](tasks::Result).
    impl<E: tasks::Endpoint> ::core::future::IntoFuture for tasks::Call<E>
    where
        E::Paging: ::concord::call::AwaitWithoutPaginate,
    {
        type Output = ::core::result::Result<E::Value, ::concord::Error<()>>;
        type IntoFuture = ::concord::Sending<E::Value>;
        fn into_future(self) -> Self::IntoFuture {
            tasks::Call::<E>::send(self)
        }
    }
    impl Tasks {
        /// Create a client with the default transport and settings.
        #[allow(clippy::new_without_default)]
        pub fn new() -> Self {
            let mut __client = Self {
                __core: <::concord::__private::Core<
                    _,
                > as ::core::default::Default>::default(),
            };
            __client
        }
        /// The retry policy of this client. Accepts a [`concord::Retry`].
        pub fn with_retry(mut self, __retry: ::concord::Retry) -> Self {
            self.__core = ::concord::__private::Core::with_retry(self.__core, __retry);
            self
        }
        /// The longest a call may take. Accepts a [`std::time::Duration`].
        pub fn with_timeout(mut self, __timeout: ::std::time::Duration) -> Self {
            self.__core = ::concord::__private::Core::with_timeout(
                self.__core,
                __timeout,
            );
            self
        }
        /// The longest a transport or body read may be silent. Accepts a [`std::time::Duration`].
        pub fn with_idle_timeout(mut self, __timeout: ::std::time::Duration) -> Self {
            self.__core = ::concord::__private::Core::with_idle_timeout(
                self.__core,
                __timeout,
            );
            self
        }
        /// Replace the base URL. Accepts a [`concord::Base`] or text through [`concord::base::IntoBase`].
        pub fn with_base(mut self, __base: impl ::concord::base::IntoBase) -> Self {
            self.__core = ::concord::__private::Core::with_base(self.__core, __base);
            self
        }
        /// Choose the [`concord::Transport`] beneath this client's middleware.
        pub fn with_transport(mut self, __transport: impl ::concord::Transport) -> Self {
            self.__core = ::concord::__private::Core::with_transport(
                self.__core,
                __transport,
            );
            self
        }
        /// Add a [`concord::Middleware`] before this client's middleware.
        pub fn with_middleware(
            mut self,
            __middleware: impl ::concord::Middleware,
        ) -> Self {
            self.__core = ::concord::__private::Core::with_middleware(
                self.__core,
                __middleware,
            );
            self
        }
        /// List tasks in their current order.
        ///
        /// Call: `tasks.list().await?`.
        ///
        /// Request: `GET https://tasks.example.com/api/tasks ?done&tags`.
        ///
        /// Options:
        ///
        ///
        /// `done`: Accepts a [`bool`], a &[`bool`], or an [`Option`]<[`bool`]>. `None` clears it.
        ///
        /// Filter tasks by completion state.
        ///
        /// `tags`: Accepts a [`Vec`]<[`String`]>, a &[`Vec`]<[`String`]>, or an [`Option`]<[`Vec`]<[`String`]>>. `None` clears it. Text and list conversions: [`concord::arg::Arg`](https://docs.rs/concord-api/5.0.0-rc.1/concord/arg/index.html).
        ///
        /// Keep only tasks that carry every one of these tags.
        ///
        /// `.await` returns [`tasks::Result`]<[`Vec`]<[`Task`]>>.
        ///
        /// See [`List`](tasks::List) for the endpoint documentation.
        #[must_use = "a call does nothing until sent or awaited"]
        pub fn list(&self) -> tasks::Call<tasks::List> {
            tasks::Call::__new(
                ::concord::__private::CallData::new(
                    ::core::clone::Clone::clone(self),
                    (::core::option::Option::None, ::core::option::Option::None),
                ),
            )
        }
        /// Create a task with a title.
        ///
        /// Call: `tasks.create(task).await?`.
        ///
        /// Request: `POST https://tasks.example.com/api/tasks`.
        ///
        /// Inputs:
        ///
        ///
        /// `task`: Accepts a [`NewTask`] or a &[`NewTask`].
        ///
        /// The task to create.
        ///
        /// A retry does not repeat this endpoint unless `Retry::not_applied_on` lists the status.
        ///
        /// `.await` returns [`tasks::Result`]<[`Task`]>.
        ///
        /// See [`Create`](tasks::Create) for the endpoint documentation.
        #[must_use = "a call does nothing until sent or awaited"]
        pub fn create(
            &self,
            task: impl ::concord::arg::Arg<NewTask>,
        ) -> tasks::Call<tasks::Create> {
            tasks::Call::__new(
                ::concord::__private::CallData::new(
                    ::core::clone::Clone::clone(self),
                    (::concord::arg::Arg::into_input(task),),
                ),
            )
        }
        /// Fetch one task by its identifier.
        ///
        /// Call: `tasks.get(id).await?`.
        ///
        /// Request: `GET https://tasks.example.com/api/tasks/{id}`.
        ///
        /// Inputs:
        ///
        ///
        /// `id`: Accepts a [`u64`] or a &[`u64`].
        ///
        /// The identifier of the task to fetch.
        ///
        /// `.await` returns [`tasks::Result`]<[`Task`]>.
        ///
        /// See [`Get`](tasks::Get) for the endpoint documentation.
        #[must_use = "a call does nothing until sent or awaited"]
        pub fn get(&self, id: impl ::concord::arg::Arg<u64>) -> tasks::Call<tasks::Get> {
            tasks::Call::__new(
                ::concord::__private::CallData::new(
                    ::core::clone::Clone::clone(self),
                    (::concord::arg::Arg::into_input(id),),
                ),
            )
        }
        /// Delete one task by its identifier.
        ///
        /// Call: `tasks.delete(id).await?`.
        ///
        /// Request: `DELETE https://tasks.example.com/api/tasks/{id}`.
        ///
        /// Inputs:
        ///
        ///
        /// `id`: Accepts a [`u64`] or a &[`u64`].
        ///
        /// The identifier of the task to delete.
        ///
        /// `.await` returns [`tasks::Result`]<`()`>.
        ///
        /// See [`Delete`](tasks::Delete) for the endpoint documentation.
        #[must_use = "a call does nothing until sent or awaited"]
        pub fn delete(
            &self,
            id: impl ::concord::arg::Arg<u64>,
        ) -> tasks::Call<tasks::Delete> {
            tasks::Call::__new(
                ::concord::__private::CallData::new(
                    ::core::clone::Clone::clone(self),
                    (::concord::arg::Arg::into_input(id),),
                ),
            )
        }
    }
    impl ::core::marker::Unpin for Tasks {}
    impl ::core::clone::Clone for Tasks {
        fn clone(&self) -> Self {
            Self {
                __core: ::core::clone::Clone::clone(&self.__core),
            }
        }
    }
    impl ::core::fmt::Debug for Tasks {
        fn fmt(&self, __f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            ::core::fmt::DebugStruct::finish_non_exhaustive(
                &mut ::core::fmt::Formatter::debug_struct(__f, "Tasks"),
            )
        }
    }
    impl tasks::List {
        /// Option `done` of `list`.
        ///
        /// The first argument is the call.
        ///
        /// Filter tasks by completion state.
        ///
        /// Accepts a `bool`, a &`bool`, or an [`Option`]<`bool`>. `None` clears it.
        #[must_use = "a call does nothing until sent or awaited"]
        #[doc(alias = "list done")]
        pub fn done(
            mut __call: tasks::Call<tasks::List>,
            done: impl ::concord::arg::Arg<::core::option::Option<bool>>,
        ) -> tasks::Call<tasks::List> {
            (*tasks::Call::__data_mut(&mut __call)).__inputs.0 = ::concord::arg::Arg::into_input(
                done,
            );
            __call
        }
        /// Option `tags` of `list`.
        ///
        /// The first argument is the call.
        ///
        /// Keep only tasks that carry every one of these tags.
        ///
        /// Accepts a `Vec`<`String`>, a &`Vec`<`String`>, or an [`Option`]<`Vec`<`String`>>. `None` clears it. Text and list conversions: [`concord::arg::Arg`](https://docs.rs/concord-api/5.0.0-rc.1/concord/arg/index.html).
        #[must_use = "a call does nothing until sent or awaited"]
        #[doc(alias = "list tags")]
        pub fn tags(
            mut __call: tasks::Call<tasks::List>,
            tags: impl ::concord::arg::Arg<::core::option::Option<Vec<String>>>,
        ) -> tasks::Call<tasks::List> {
            (*tasks::Call::__data_mut(&mut __call)).__inputs.1 = ::concord::arg::Arg::into_input(
                tags,
            );
            __call
        }
    }
    #[doc(hidden)]
    impl tasks::Call<tasks::List> {
        /// Option `done` of `list`.
        ///
        /// Filter tasks by completion state.
        ///
        /// Accepts a [`bool`], a &[`bool`], or an [`Option`]<[`bool`]>. `None` clears it.
        #[must_use = "a call does nothing until sent or awaited"]
        #[allow(deprecated)]
        pub fn done(
            self,
            done: impl ::concord::arg::Arg<::core::option::Option<bool>>,
        ) -> Self {
            tasks::List::done(self, done)
        }
        /// Option `tags` of `list`.
        ///
        /// Keep only tasks that carry every one of these tags.
        ///
        /// Accepts a [`Vec`]<[`String`]>, a &[`Vec`]<[`String`]>, or an [`Option`]<[`Vec`]<[`String`]>>. `None` clears it. Text and list conversions: [`concord::arg::Arg`](https://docs.rs/concord-api/5.0.0-rc.1/concord/arg/index.html).
        #[must_use = "a call does nothing until sent or awaited"]
        #[allow(deprecated)]
        pub fn tags(
            self,
            tags: impl ::concord::arg::Arg<::core::option::Option<Vec<String>>>,
        ) -> Self {
            tasks::List::tags(self, tags)
        }
    }
    const _: () = {
        impl __check_problem_1::Declared for __check_problem_1::Witness {
            type Value = <tasks::List as tasks::Endpoint>::Value;
            type Problem = <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::ClientProblem>::Problem;
        }
        #[allow(dead_code, clippy::needless_borrow)]
        mod __check_problem_1 {
            pub(super) struct Witness;
            pub(super) trait Declared {
                type Value: ?::core::marker::Sized;
                type Problem;
            }
            fn check() {
                use ::concord::__private::{TypedStream as _, OrdinaryOutput as _};
                (&::concord::__private::Output::<<Witness as Declared>::Value>::new())
                    .must_name_the_clients_problem_type::<
                        <Witness as Declared>::Problem,
                    >();
            }
        }
    };
    #[doc(hidden)]
    impl ::concord::__private::EndpointInputs2<1usize>
    for tasks::Call<::concord::__private::ClientKey> {
        type Input0 = bool;
        type Input1 = Vec<String>;
    }
    impl tasks::Endpoint for tasks::List {
        type Data = ::concord::__private::CallData<
            <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::StoredType<0usize>>::Value,
            (
                ::core::option::Option<
                    <tasks::Call<
                        ::concord::__private::ClientKey,
                    > as ::concord::__private::EndpointInputs2<1usize>>::Input0,
                >,
                ::core::option::Option<
                    <tasks::Call<
                        ::concord::__private::ClientKey,
                    > as ::concord::__private::EndpointInputs2<1usize>>::Input1,
                >,
            ),
            Self,
        >;
        type Value = Vec<Task>;
        type Paging = ::concord::call::NoPaginate;
        type Conditional = ::concord::call::GetOrHead;
        const NAME: &'static ::core::primitive::str = "tasks.list";
        const __DEBUG: &'static ::core::primitive::str = "List";
        fn __core(
            __value: &Self::Data,
        ) -> &::concord::__private::Core<
            <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::ClientProblem>::Problem,
        > {
            &__value.__parent.__core
        }
        fn __build_draft(
            __value: &Self::Data,
        ) -> tasks::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::from_literal(
                ::concord::http::Method::GET,
                "https://tasks.example.com/api/tasks",
            );
            {
                ::concord::__private::Draft::query_value(
                    &mut __draft,
                    "done",
                    "done",
                    ::concord::__private::AdaptPlacement::adapt_placement(
                        &&__value.__inputs.0,
                    ),
                );
                ::concord::__private::Draft::query_value(
                    &mut __draft,
                    "tags",
                    "tags",
                    ::concord::__private::AdaptPlacement::adapt_placement(
                        &&__value.__inputs.1,
                    ),
                );
            }
            ::core::result::Result::Ok(__draft)
        }
        fn __read_answer(
            __value: &Self::Data,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> impl ::core::future::Future<
            Output = tasks::Result<::concord::Answer<Self::Value>>,
        > + ::core::marker::Send {
            ::concord::__private::answer_read(
                __response,
                __info,
                move || {
                    ::concord::__private::reader_for_response::<
                        Self::Value,
                        _,
                    >((|| { Json })())
                },
                ::core::convert::identity,
                ::concord::__private::PlainRefusal,
            )
        }
        fn __pages(
            _: tasks::Call<Self>,
            __proof: ::core::convert::Infallible,
        ) -> ::concord::Pages<
            Self::Value,
            <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::ClientProblem>::Problem,
        > {
            match __proof {}
        }
    }
    const _: () = {
        impl __check_problem_2::Declared for __check_problem_2::Witness {
            type Value = <tasks::Create as tasks::Endpoint>::Value;
            type Problem = <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::ClientProblem>::Problem;
        }
        #[allow(dead_code, clippy::needless_borrow)]
        mod __check_problem_2 {
            pub(super) struct Witness;
            pub(super) trait Declared {
                type Value: ?::core::marker::Sized;
                type Problem;
            }
            fn check() {
                use ::concord::__private::{TypedStream as _, OrdinaryOutput as _};
                (&::concord::__private::Output::<<Witness as Declared>::Value>::new())
                    .must_name_the_clients_problem_type::<
                        <Witness as Declared>::Problem,
                    >();
            }
        }
    };
    #[doc(hidden)]
    impl ::concord::__private::EndpointInputs1<2usize>
    for tasks::Call<::concord::__private::ClientKey> {
        type Input0 = NewTask;
    }
    impl tasks::Endpoint for tasks::Create {
        type Data = ::concord::__private::CallData<
            <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::StoredType<0usize>>::Value,
            (
                <tasks::Call<
                    ::concord::__private::ClientKey,
                > as ::concord::__private::EndpointInputs1<2usize>>::Input0,
            ),
            Self,
        >;
        type Value = Task;
        type Paging = ::concord::call::NoPaginate;
        type Conditional = ::concord::call::Post;
        const NAME: &'static ::core::primitive::str = "tasks.create";
        const __DEBUG: &'static ::core::primitive::str = "Create";
        fn __core(
            __value: &Self::Data,
        ) -> &::concord::__private::Core<
            <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::ClientProblem>::Problem,
        > {
            &__value.__parent.__core
        }
        fn __build_draft(
            __value: &Self::Data,
        ) -> tasks::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::from_literal(
                ::concord::http::Method::POST,
                "https://tasks.example.com/api/tasks",
            );
            {
                let task = &__value.__inputs.0;
                let __body = ::concord::__private::evaluate_body(
                    <Self as tasks::Endpoint>::NAME,
                    || ::core::result::Result::Ok({ json(task) }),
                )?;
                ::concord::__private::Draft::whole_body(&mut __draft, __body);
            }
            ::core::result::Result::Ok(__draft)
        }
        fn __read_answer(
            __value: &Self::Data,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> impl ::core::future::Future<
            Output = tasks::Result<::concord::Answer<Self::Value>>,
        > + ::core::marker::Send {
            ::concord::__private::answer_read(
                __response,
                __info,
                move || {
                    ::concord::__private::reader_for_response::<
                        Self::Value,
                        _,
                    >((|| { Json })())
                },
                ::core::convert::identity,
                ::concord::__private::PlainRefusal,
            )
        }
        fn __pages(
            _: tasks::Call<Self>,
            __proof: ::core::convert::Infallible,
        ) -> ::concord::Pages<
            Self::Value,
            <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::ClientProblem>::Problem,
        > {
            match __proof {}
        }
    }
    const _: () = {
        impl __check_problem_3::Declared for __check_problem_3::Witness {
            type Value = <tasks::Get as tasks::Endpoint>::Value;
            type Problem = <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::ClientProblem>::Problem;
        }
        #[allow(dead_code, clippy::needless_borrow)]
        mod __check_problem_3 {
            pub(super) struct Witness;
            pub(super) trait Declared {
                type Value: ?::core::marker::Sized;
                type Problem;
            }
            fn check() {
                use ::concord::__private::{TypedStream as _, OrdinaryOutput as _};
                (&::concord::__private::Output::<<Witness as Declared>::Value>::new())
                    .must_name_the_clients_problem_type::<
                        <Witness as Declared>::Problem,
                    >();
            }
        }
    };
    #[doc(hidden)]
    impl ::concord::__private::EndpointInputs1<3usize>
    for tasks::Call<::concord::__private::ClientKey> {
        type Input0 = u64;
    }
    impl tasks::Endpoint for tasks::Get {
        type Data = ::concord::__private::CallData<
            <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::StoredType<0usize>>::Value,
            (
                <tasks::Call<
                    ::concord::__private::ClientKey,
                > as ::concord::__private::EndpointInputs1<3usize>>::Input0,
            ),
            Self,
        >;
        type Value = Task;
        type Paging = ::concord::call::NoPaginate;
        type Conditional = ::concord::call::GetOrHead;
        const NAME: &'static ::core::primitive::str = "tasks.get";
        const __DEBUG: &'static ::core::primitive::str = "Get";
        fn __core(
            __value: &Self::Data,
        ) -> &::concord::__private::Core<
            <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::ClientProblem>::Problem,
        > {
            &__value.__parent.__core
        }
        fn __build_draft(
            __value: &Self::Data,
        ) -> tasks::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::from_literal(
                ::concord::http::Method::GET,
                "https://tasks.example.com/api/tasks",
            );
            ::concord::__private::Draft::segment_value(
                &mut __draft,
                "id",
                ::concord::__private::AdaptPlacement::adapt_placement(
                    &&__value.__inputs.0,
                ),
                false,
            );
            ::core::result::Result::Ok(__draft)
        }
        fn __read_answer(
            __value: &Self::Data,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> impl ::core::future::Future<
            Output = tasks::Result<::concord::Answer<Self::Value>>,
        > + ::core::marker::Send {
            ::concord::__private::answer_read(
                __response,
                __info,
                move || {
                    ::concord::__private::reader_for_response::<
                        Self::Value,
                        _,
                    >((|| { Json })())
                },
                ::core::convert::identity,
                ::concord::__private::PlainRefusal,
            )
        }
        fn __pages(
            _: tasks::Call<Self>,
            __proof: ::core::convert::Infallible,
        ) -> ::concord::Pages<
            Self::Value,
            <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::ClientProblem>::Problem,
        > {
            match __proof {}
        }
    }
    const _: () = {
        impl __check_problem_4::Declared for __check_problem_4::Witness {
            type Value = <tasks::Delete as tasks::Endpoint>::Value;
            type Problem = <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::ClientProblem>::Problem;
        }
        #[allow(dead_code, clippy::needless_borrow)]
        mod __check_problem_4 {
            pub(super) struct Witness;
            pub(super) trait Declared {
                type Value: ?::core::marker::Sized;
                type Problem;
            }
            fn check() {
                use ::concord::__private::{TypedStream as _, OrdinaryOutput as _};
                (&::concord::__private::Output::<<Witness as Declared>::Value>::new())
                    .must_name_the_clients_problem_type::<
                        <Witness as Declared>::Problem,
                    >();
            }
        }
    };
    #[doc(hidden)]
    impl ::concord::__private::EndpointInputs1<4usize>
    for tasks::Call<::concord::__private::ClientKey> {
        type Input0 = u64;
    }
    impl tasks::Endpoint for tasks::Delete {
        type Data = ::concord::__private::CallData<
            <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::StoredType<0usize>>::Value,
            (
                <tasks::Call<
                    ::concord::__private::ClientKey,
                > as ::concord::__private::EndpointInputs1<4usize>>::Input0,
            ),
            Self,
        >;
        type Value = ();
        type Paging = ::concord::call::NoPaginate;
        type Conditional = ::concord::call::NotGetOrHead;
        const NAME: &'static ::core::primitive::str = "tasks.delete";
        const __DEBUG: &'static ::core::primitive::str = "Delete";
        fn __core(
            __value: &Self::Data,
        ) -> &::concord::__private::Core<
            <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::ClientProblem>::Problem,
        > {
            &__value.__parent.__core
        }
        fn __build_draft(
            __value: &Self::Data,
        ) -> tasks::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::from_literal(
                ::concord::http::Method::DELETE,
                "https://tasks.example.com/api/tasks",
            );
            ::concord::__private::Draft::segment_value(
                &mut __draft,
                "id",
                ::concord::__private::AdaptPlacement::adapt_placement(
                    &&__value.__inputs.0,
                ),
                false,
            );
            ::core::result::Result::Ok(__draft)
        }
        fn __read_answer(
            __value: &Self::Data,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> impl ::core::future::Future<
            Output = tasks::Result<::concord::Answer<Self::Value>>,
        > + ::core::marker::Send {
            ::concord::__private::answer_empty(
                __response,
                __info,
                ::concord::__private::PlainRefusal,
            )
        }
        fn __pages(
            _: tasks::Call<Self>,
            __proof: ::core::convert::Infallible,
        ) -> ::concord::Pages<
            Self::Value,
            <tasks::Call<
                ::concord::__private::ClientKey,
            > as ::concord::__private::ClientProblem>::Problem,
        > {
            match __proof {}
        }
    }
};
