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
    __core: ::concord::__private::Core<<() as tasks::__Types>::__Problem>,
}
///Calls and results for [`Tasks`].
pub mod tasks {
    #[doc(hidden)]
    pub trait __Types {
        type __Input0: ::core::clone::Clone
            + ::core::marker::Send
            + ::core::marker::Sync
            + 'static;
        type __Input1: ::core::clone::Clone
            + ::core::marker::Send
            + ::core::marker::Sync
            + 'static;
        type __Input2: ::core::clone::Clone
            + ::core::marker::Send
            + ::core::marker::Sync
            + 'static;
        type __Input3: ::core::clone::Clone
            + ::core::marker::Send
            + ::core::marker::Sync
            + 'static;
        type __Node0: ::core::clone::Clone
            + ::core::marker::Send
            + ::core::marker::Sync
            + 'static;
        type __Problem: ::core::marker::Send + ::core::marker::Sync + 'static;
        fn __clone_input0(__value: &Self::__Input0) -> Self::__Input0 {
            ::core::clone::Clone::clone(__value)
        }
        fn __clone_input1(__value: &Self::__Input1) -> Self::__Input1 {
            ::core::clone::Clone::clone(__value)
        }
        fn __clone_input2(__value: &Self::__Input2) -> Self::__Input2 {
            ::core::clone::Clone::clone(__value)
        }
        fn __clone_input3(__value: &Self::__Input3) -> Self::__Input3 {
            ::core::clone::Clone::clone(__value)
        }
    }
    ///The error of each call: [`concord::Error`].
    pub type Error = ::concord::Error;
    /// The result of a call, with this client's error.
    pub type Result<T> = ::core::result::Result<T, Error>;
    ///The retry policy for this client: [`concord::Retry`].
    pub type Retry = ::concord::Retry;
    /// List tasks in their current order.
    ///The call `list`.
    #[must_use = "a call does nothing until sent or awaited"]
    pub struct List {
        pub(super) __parent: <() as __Types>::__Node0,
        pub(super) done: <() as __Types>::__Input0,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }
    /// Create a task with a title.
    ///The call `create`.
    #[must_use = "a call does nothing until sent or awaited"]
    pub struct Create {
        pub(super) __parent: <() as __Types>::__Node0,
        pub(super) task: <() as __Types>::__Input1,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }
    /// Fetch one task by its identifier.
    ///The call `get`.
    #[must_use = "a call does nothing until sent or awaited"]
    pub struct Get {
        pub(super) __parent: <() as __Types>::__Node0,
        pub(super) id: <() as __Types>::__Input2,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }
    /// Delete one task by its identifier.
    ///The call `delete`.
    #[must_use = "a call does nothing until sent or awaited"]
    pub struct Delete {
        pub(super) __parent: <() as __Types>::__Node0,
        pub(super) id: <() as __Types>::__Input3,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }
}
#[allow(
    unused_variables,
    unused_mut,
    unused_braces,
    dead_code,
    clippy::redundant_closure_call
)]
const _: () = {
    #[allow(unused_imports)]
    use ::concord::__private::AdaptValue as _;
    impl tasks::__Types for () {
        type __Node0 = Tasks;
        type __Input0 = ::core::option::Option<bool>;
        type __Input1 = NewTask;
        type __Input2 = u64;
        type __Input3 = u64;
        type __Problem = ();
    }
    struct __View0;
    impl Tasks {
        /// Create a client with the default transport and settings.
        #[allow(clippy::new_without_default)]
        pub fn new() -> Self {
            let mut __client = Self {
                __core: ::concord::__private::Core::default(),
            };
            __client
        }
        /// The retry policy of this client. Accepts a [`concord::Retry`].
        pub fn with_retry(mut self, __retry: ::concord::Retry) -> Self {
            self.__core = self.__core.with_retry(__retry);
            self
        }
        /// The longest a call may take. Accepts a [`std::time::Duration`].
        pub fn with_timeout(mut self, __timeout: ::std::time::Duration) -> Self {
            self.__core = self.__core.with_timeout(__timeout);
            self
        }
        /// The longest a transport or body read may be silent. Accepts a [`std::time::Duration`].
        pub fn with_idle_timeout(mut self, __timeout: ::std::time::Duration) -> Self {
            self.__core = self.__core.with_idle_timeout(__timeout);
            self
        }
        /// Replace the base URL. Accepts a [`concord::Base`] or text through [`concord::base::IntoBase`].
        pub fn with_base(mut self, __base: impl ::concord::base::IntoBase) -> Self {
            self.__core = self.__core.with_base(__base);
            self
        }
        /// Choose the [`concord::Transport`] beneath this client's middleware.
        pub fn with_transport(mut self, __transport: impl ::concord::Transport) -> Self {
            self.__core = self.__core.with_transport(__transport);
            self
        }
        /// Add a [`concord::Middleware`] before this client's middleware.
        pub fn with_middleware(
            mut self,
            __middleware: impl ::concord::Middleware,
        ) -> Self {
            self.__core = self.__core.with_middleware(__middleware);
            self
        }
        /// List tasks in their current order.
        ///
        /// Call: `tasks.list().await?`.
        ///
        /// Request: `GET https://tasks.example.com/api/tasks ?done`.
        ///
        /// Options:
        /// - `done`: Accepts a [`bool`], a &[`bool`], or an [`Option`]<[`bool`]>. `None` clears it.
        ///   Filter tasks by completion state.
        ///
        /// `.await` gives [`tasks::Result`]<[`Vec`]<[`Task`]>>.
        pub fn list(&self) -> tasks::List {
            tasks::List {
                __parent: ::core::clone::Clone::clone(self),
                done: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
        }
        /// Create a task with a title.
        ///
        /// Call: `tasks.create(task).await?`.
        ///
        /// Request: `POST https://tasks.example.com/api/tasks`.
        ///
        /// Inputs:
        /// - `task`: Accepts a [`NewTask`] or a &[`NewTask`].
        ///   The task to create.
        ///
        /// This endpoint may not be repeated.
        ///
        /// `.await` gives [`tasks::Result`]<[`Task`]>.
        pub fn create(&self, task: impl ::concord::arg::Arg<NewTask>) -> tasks::Create {
            tasks::Create {
                __parent: ::core::clone::Clone::clone(self),
                task: ::concord::arg::Arg::into_input(task),
                __options: ::concord::__private::Options::default(),
            }
        }
        /// Fetch one task by its identifier.
        ///
        /// Call: `tasks.get(id).await?`.
        ///
        /// Request: `GET https://tasks.example.com/api/tasks/{id}`.
        ///
        /// Inputs:
        /// - `id`: Accepts a [`u64`] or a &[`u64`].
        ///   The identifier of the task to fetch.
        ///
        /// `.await` gives [`tasks::Result`]<[`Task`]>.
        pub fn get(&self, id: impl ::concord::arg::Arg<u64>) -> tasks::Get {
            tasks::Get {
                __parent: ::core::clone::Clone::clone(self),
                id: ::concord::arg::Arg::into_input(id),
                __options: ::concord::__private::Options::default(),
            }
        }
        /// Delete one task by its identifier.
        ///
        /// Call: `tasks.delete(id).await?`.
        ///
        /// Request: `DELETE https://tasks.example.com/api/tasks/{id}`.
        ///
        /// Inputs:
        /// - `id`: Accepts a [`u64`] or a &[`u64`].
        ///   The identifier of the task to delete.
        ///
        /// `.await` gives [`tasks::Result`]<`()`>.
        pub fn delete(&self, id: impl ::concord::arg::Arg<u64>) -> tasks::Delete {
            tasks::Delete {
                __parent: ::core::clone::Clone::clone(self),
                id: ::concord::arg::Arg::into_input(id),
                __options: ::concord::__private::Options::default(),
            }
        }
    }
    impl ::core::clone::Clone for Tasks {
        fn clone(&self) -> Self {
            Self {
                __core: ::core::clone::Clone::clone(&self.__core),
            }
        }
    }
    impl ::core::fmt::Debug for Tasks {
        fn fmt(&self, __f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            __f.debug_struct("Tasks").finish_non_exhaustive()
        }
    }
    impl tasks::List {
        /// Filter tasks by completion state.
        /// Accepts a [`bool`], a &[`bool`], or an [`Option`]<[`bool`]>. `None` clears it.
        pub fn done(
            mut self,
            done: impl ::concord::arg::Arg<::core::option::Option<bool>>,
        ) -> Self {
            self.done = ::concord::arg::Arg::into_input(done);
            self
        }
    }
    impl ::core::clone::Clone for tasks::List {
        fn clone(&self) -> Self {
            Self {
                __parent: ::core::clone::Clone::clone(&self.__parent),
                done: <() as tasks::__Types>::__clone_input0(&self.done),
                __options: ::core::clone::Clone::clone(&self.__options),
            }
        }
    }
    impl ::core::fmt::Debug for tasks::List {
        fn fmt(&self, __f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            __f.debug_struct("List")
                .field("endpoint", &"tasks.list")
                .finish_non_exhaustive()
        }
    }
    impl tasks::List {
        const __ENDPOINT: &'static str = "tasks.list";
        fn __build_draft(&self) -> tasks::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(
                ::concord::http::Method::GET,
            );
            {
                let client = __View0;
                __draft
                    .base(
                        &[
                            ::concord::__private::Piece::Lit(""),
                            ::concord::__private::Piece::Lit(
                                "https://tasks.example.com/api/tasks",
                            ),
                        ],
                    );
            }
            {
                let (__EntryExpression0,) = {
                    let client = __View0;
                    let done = &self.done;
                    let __EntryExpression0 = done;
                    (__EntryExpression0,)
                };
                let __EntryValue0 = ::concord::__private::ValueRef(__EntryExpression0);
                let __EntryAdapted0 = (&__EntryValue0).adapt();
                let __Entry0 = &[
                    ::concord::__private::Piece::Val("done", &__EntryAdapted0),
                ];
                __draft.query("done", __Entry0);
            }
            ::core::result::Result::Ok(__draft)
        }
        fn __build_request(
            &self,
            __take: bool,
        ) -> tasks::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft
                    .finish_for_send(
                        &self.__parent.__core,
                        &self.__options,
                        Self::__ENDPOINT,
                    )
            } else {
                __draft.finish(&self.__parent.__core, &self.__options, Self::__ENDPOINT)
            }
        }
        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> tasks::Result<::concord::Answer<Vec<Task>>> {
            let __url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            let __value = match __status {
                _ if __status.is_success() => {
                    ::concord::__private::read_body::<
                        Vec<Task>,
                        (),
                        _,
                    >(
                            &{
                                fn __reader<__T, __R: ::concord::read::Reader<__T>>(
                                    __value: __R,
                                ) -> impl ::concord::read::Reader<__T> {
                                    ::concord::__private::reader::<__T, __R>(__value)
                                }
                                let client = __View0;
                                __reader::<Vec<Task>, _>((|| { Json })())
                            },
                            __response,
                            &__info,
                        )
                        .await?
                }
                _ => {
                    return ::core::result::Result::Err(
                        ::concord::__private::refused_plain(__response, &__info).await,
                    );
                }
            };
            ::core::result::Result::Ok(
                ::concord::Answer::new(__status, __headers, __value)
                    .with_response_url(__url),
            )
        }
        fn __prepare_inner(
            self,
        ) -> tasks::Result<
            ::concord::__private::Prepared<Vec<Task>, <() as tasks::__Types>::__Problem>,
        > {
            let __run = ::concord::__private::Run::start(
                &self.__parent.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft
                .prepare::<
                    Vec<Task>,
                    (),
                    _,
                    _,
                >(
                    __run,
                    ::core::clone::Clone::clone(&self.__parent.__core),
                    ::core::clone::Clone::clone(&self.__options),
                    __idempotent,
                    move |__response, __info| {
                        let __call = ::core::clone::Clone::clone(&self);
                        async move { __call.__read_answer(__response, __info).await }
                    },
                )
        }
    }
    const _: () = {
        ::concord::__private::call_methods!(
            tasks::List, Vec < Task >, (), tasks::Result,
            "[`tasks::Result`]<[`Vec`]<[`Task`]>>", awaitable, conditional
        );
    };
    impl ::core::clone::Clone for tasks::Create {
        fn clone(&self) -> Self {
            Self {
                __parent: ::core::clone::Clone::clone(&self.__parent),
                task: <() as tasks::__Types>::__clone_input1(&self.task),
                __options: ::core::clone::Clone::clone(&self.__options),
            }
        }
    }
    impl ::core::fmt::Debug for tasks::Create {
        fn fmt(&self, __f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            __f.debug_struct("Create")
                .field("endpoint", &"tasks.create")
                .finish_non_exhaustive()
        }
    }
    impl tasks::Create {
        const __ENDPOINT: &'static str = "tasks.create";
        fn __build_draft(&self) -> tasks::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(
                ::concord::http::Method::POST,
            );
            {
                let client = __View0;
                __draft
                    .base(
                        &[
                            ::concord::__private::Piece::Lit(""),
                            ::concord::__private::Piece::Lit(
                                "https://tasks.example.com/api/tasks",
                            ),
                        ],
                    );
            }
            {
                let client = __View0;
                let task = &self.task;
                let __body = (|| -> ::core::result::Result<
                    _,
                    ::std::boxed::Box<
                        dyn ::std::error::Error + ::core::marker::Send + ::core::marker::Sync,
                    >,
                > { ::core::result::Result::Ok({ json(task) }) })()
                    .map_err(|__error| ::concord::__private::build_error(
                        Self::__ENDPOINT,
                        __error,
                    ))?;
                __draft.body(__body);
            }
            ::core::result::Result::Ok(__draft)
        }
        fn __build_request(
            &self,
            __take: bool,
        ) -> tasks::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft
                    .finish_for_send(
                        &self.__parent.__core,
                        &self.__options,
                        Self::__ENDPOINT,
                    )
            } else {
                __draft.finish(&self.__parent.__core, &self.__options, Self::__ENDPOINT)
            }
        }
        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> tasks::Result<::concord::Answer<Task>> {
            let __url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            let __value = match __status {
                _ if __status.is_success() => {
                    ::concord::__private::read_body::<
                        Task,
                        (),
                        _,
                    >(
                            &{
                                fn __reader<__T, __R: ::concord::read::Reader<__T>>(
                                    __value: __R,
                                ) -> impl ::concord::read::Reader<__T> {
                                    ::concord::__private::reader::<__T, __R>(__value)
                                }
                                let client = __View0;
                                __reader::<Task, _>((|| { Json })())
                            },
                            __response,
                            &__info,
                        )
                        .await?
                }
                _ => {
                    return ::core::result::Result::Err(
                        ::concord::__private::refused_plain(__response, &__info).await,
                    );
                }
            };
            ::core::result::Result::Ok(
                ::concord::Answer::new(__status, __headers, __value)
                    .with_response_url(__url),
            )
        }
        fn __prepare_inner(
            self,
        ) -> tasks::Result<
            ::concord::__private::Prepared<Task, <() as tasks::__Types>::__Problem>,
        > {
            let __run = ::concord::__private::Run::start(
                &self.__parent.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft
                .prepare::<
                    Task,
                    (),
                    _,
                    _,
                >(
                    __run,
                    ::core::clone::Clone::clone(&self.__parent.__core),
                    ::core::clone::Clone::clone(&self.__options),
                    __idempotent,
                    move |__response, __info| {
                        let __call = ::core::clone::Clone::clone(&self);
                        async move { __call.__read_answer(__response, __info).await }
                    },
                )
        }
    }
    const _: () = {
        ::concord::__private::call_methods!(
            tasks::Create, Task, (), tasks::Result, "[`tasks::Result`]<[`Task`]>",
            awaitable, unconditional
        );
    };
    impl ::core::clone::Clone for tasks::Get {
        fn clone(&self) -> Self {
            Self {
                __parent: ::core::clone::Clone::clone(&self.__parent),
                id: <() as tasks::__Types>::__clone_input2(&self.id),
                __options: ::core::clone::Clone::clone(&self.__options),
            }
        }
    }
    impl ::core::fmt::Debug for tasks::Get {
        fn fmt(&self, __f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            __f.debug_struct("Get")
                .field("endpoint", &"tasks.get")
                .finish_non_exhaustive()
        }
    }
    impl tasks::Get {
        const __ENDPOINT: &'static str = "tasks.get";
        fn __build_draft(&self) -> tasks::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(
                ::concord::http::Method::GET,
            );
            {
                let client = __View0;
                __draft
                    .base(
                        &[
                            ::concord::__private::Piece::Lit(""),
                            ::concord::__private::Piece::Lit(
                                "https://tasks.example.com/api/tasks",
                            ),
                        ],
                    );
            }
            {
                let client = __View0;
                let id = &self.id;
                __draft
                    .segment(
                        &[
                            ::concord::__private::Piece::Val(
                                "id",
                                &(&::concord::__private::ValueRef(id)).adapt(),
                            ),
                        ],
                    );
            }
            ::core::result::Result::Ok(__draft)
        }
        fn __build_request(
            &self,
            __take: bool,
        ) -> tasks::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft
                    .finish_for_send(
                        &self.__parent.__core,
                        &self.__options,
                        Self::__ENDPOINT,
                    )
            } else {
                __draft.finish(&self.__parent.__core, &self.__options, Self::__ENDPOINT)
            }
        }
        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> tasks::Result<::concord::Answer<Task>> {
            let __url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            let __value = match __status {
                _ if __status.is_success() => {
                    ::concord::__private::read_body::<
                        Task,
                        (),
                        _,
                    >(
                            &{
                                fn __reader<__T, __R: ::concord::read::Reader<__T>>(
                                    __value: __R,
                                ) -> impl ::concord::read::Reader<__T> {
                                    ::concord::__private::reader::<__T, __R>(__value)
                                }
                                let client = __View0;
                                __reader::<Task, _>((|| { Json })())
                            },
                            __response,
                            &__info,
                        )
                        .await?
                }
                _ => {
                    return ::core::result::Result::Err(
                        ::concord::__private::refused_plain(__response, &__info).await,
                    );
                }
            };
            ::core::result::Result::Ok(
                ::concord::Answer::new(__status, __headers, __value)
                    .with_response_url(__url),
            )
        }
        fn __prepare_inner(
            self,
        ) -> tasks::Result<
            ::concord::__private::Prepared<Task, <() as tasks::__Types>::__Problem>,
        > {
            let __run = ::concord::__private::Run::start(
                &self.__parent.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft
                .prepare::<
                    Task,
                    (),
                    _,
                    _,
                >(
                    __run,
                    ::core::clone::Clone::clone(&self.__parent.__core),
                    ::core::clone::Clone::clone(&self.__options),
                    __idempotent,
                    move |__response, __info| {
                        let __call = ::core::clone::Clone::clone(&self);
                        async move { __call.__read_answer(__response, __info).await }
                    },
                )
        }
    }
    const _: () = {
        ::concord::__private::call_methods!(
            tasks::Get, Task, (), tasks::Result, "[`tasks::Result`]<[`Task`]>",
            awaitable, conditional
        );
    };
    impl ::core::clone::Clone for tasks::Delete {
        fn clone(&self) -> Self {
            Self {
                __parent: ::core::clone::Clone::clone(&self.__parent),
                id: <() as tasks::__Types>::__clone_input3(&self.id),
                __options: ::core::clone::Clone::clone(&self.__options),
            }
        }
    }
    impl ::core::fmt::Debug for tasks::Delete {
        fn fmt(&self, __f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            __f.debug_struct("Delete")
                .field("endpoint", &"tasks.delete")
                .finish_non_exhaustive()
        }
    }
    impl tasks::Delete {
        const __ENDPOINT: &'static str = "tasks.delete";
        fn __build_draft(&self) -> tasks::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(
                ::concord::http::Method::DELETE,
            );
            {
                let client = __View0;
                __draft
                    .base(
                        &[
                            ::concord::__private::Piece::Lit(""),
                            ::concord::__private::Piece::Lit(
                                "https://tasks.example.com/api/tasks",
                            ),
                        ],
                    );
            }
            {
                let client = __View0;
                let id = &self.id;
                __draft
                    .segment(
                        &[
                            ::concord::__private::Piece::Val(
                                "id",
                                &(&::concord::__private::ValueRef(id)).adapt(),
                            ),
                        ],
                    );
            }
            ::core::result::Result::Ok(__draft)
        }
        fn __build_request(
            &self,
            __take: bool,
        ) -> tasks::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft
                    .finish_for_send(
                        &self.__parent.__core,
                        &self.__options,
                        Self::__ENDPOINT,
                    )
            } else {
                __draft.finish(&self.__parent.__core, &self.__options, Self::__ENDPOINT)
            }
        }
        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> tasks::Result<::concord::Answer<()>> {
            let __url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            let __value = match __status {
                _ if __status.is_success() => {}
                _ => {
                    return ::core::result::Result::Err(
                        ::concord::__private::refused_plain(__response, &__info).await,
                    );
                }
            };
            ::core::result::Result::Ok(
                ::concord::Answer::new(__status, __headers, __value)
                    .with_response_url(__url),
            )
        }
        fn __prepare_inner(
            self,
        ) -> tasks::Result<
            ::concord::__private::Prepared<(), <() as tasks::__Types>::__Problem>,
        > {
            let __run = ::concord::__private::Run::start(
                &self.__parent.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft
                .prepare::<
                    (),
                    (),
                    _,
                    _,
                >(
                    __run,
                    ::core::clone::Clone::clone(&self.__parent.__core),
                    ::core::clone::Clone::clone(&self.__options),
                    __idempotent,
                    move |__response, __info| {
                        let __call = ::core::clone::Clone::clone(&self);
                        async move { __call.__read_answer(__response, __info).await }
                    },
                )
        }
    }
    const _: () = {
        ::concord::__private::call_methods!(
            tasks::Delete, (), (), tasks::Result, "[`tasks::Result`]<`()`>", awaitable,
            unconditional
        );
    };
};
