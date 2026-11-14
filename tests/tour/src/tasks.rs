const _: () = {
    use ::concord::__private::AdaptValue as _;

    // This client's owner view has no inputs.
    struct __Client;
    use concord::{json, Json};

    // /// A client for creating, finding, and deleting tasks.
    // ///
    // /// # Example
    // ///
    // /// ```no_run
    // /// use tour::Tasks;
    // ///
    // /// # async fn example() -> tour::tasks::Result<()> {
    // /// let tasks = Tasks::new();
    // /// let open = tasks.list().done(false).await?;
    // /// # Ok(())
    // /// # }
    // /// ```
    // ///
    // pub client Tasks {
    //     base "https://tasks.example.com/api/tasks";
    //     read Json;
    //     /// List tasks in their current order.
    //     GET list(
    //         /// Filter tasks by completion state.
    //         done?: bool,
    //     ) -> Vec<Task> {
    //         query { done; }
    //     }
    //     /// Create a task with a title.
    //     POST create(
    //         /// The task to create.
    //         task: NewTask,
    //     ) -> Task {
    //         body json(task);
    //     }
    //     /// Fetch one task by its identifier.
    //     GET get(
    //         /// The identifier of the task to fetch.
    //         id: u64,
    //     ) -> Task {
    //         path "{id}";
    //     }
    //     /// Delete one task by its identifier.
    //     DELETE delete(
    //         /// The identifier of the task to delete.
    //         id: u64,
    //     ) {
    //         path "{id}";
    //     }
    // }
    impl tasks::__Types for () {
        type __Input0 = ::core::option::Option<bool>;
        type __Input1 = NewTask;
        type __Input2 = u64;
        type __Input3 = u64;
        type __Problem = ();
    }

    impl Tasks {
        /// Create a client with the default transport and settings.
        // Preserve the hand-written generated client API without adding a Default implementation.
        #[allow(clippy::new_without_default)]
        pub fn new() -> Self {
            Self {
                __core: ::concord::__private::Core::default(),
            }
        }

        /// The retry policy for calls made by this client. Accepts a [`concord::Retry`]. Retries are off by default.
        pub fn with_retry(mut self, __retry: ::concord::Retry) -> Self {
            self.__core = self.__core.with_retry(__retry);
            self
        }

        /// The longest a whole call may take, retries and reading included. No limit by default. Accepts a [`std::time::Duration`].
        pub fn with_timeout(mut self, __timeout: ::std::time::Duration) -> Self {
            self.__core = self.__core.with_timeout(__timeout);
            self
        }

        /// The longest the transport or a body read may be silent. A call override wins,
        /// then the description's `idle_timeout`, then this client setting, then 30 seconds.
        /// The description's expression still runs for every send, even with a call override.
        pub fn with_idle_timeout(mut self, __timeout: ::std::time::Duration) -> Self {
            self.__core = self.__core.with_idle_timeout(__timeout);
            self
        }

        /// Use this base URL for every request. A path prefix is allowed.
        ///
        /// Accepts [`concord::Base`], &[`concord::Base`], &[`str`], [`String`], or &[`String`]. Invalid text becomes an error when building the request.
        pub fn with_base(mut self, __base: impl ::concord::base::IntoBase) -> Self {
            self.__core = self.__core.with_base(__base);
            self
        }

        /// Choose what really sends, under this client's middlewares.
        ///
        /// Accepts a [`concord::Http`], a [`reqwest::Client`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Client.html), a [`concord::fake`], or another [`concord::Transport`].
        pub fn with_transport(mut self, __transport: impl ::concord::Transport) -> Self {
            self.__core = self.__core.with_transport(__transport);
            self
        }

        /// Add a middleware before the description's, in the order added.
        ///
        /// It is made once and shared by this client's handles and calls.
        pub fn with_middleware(mut self, __middleware: impl ::concord::Middleware) -> Self {
            self.__core = self.__core.with_middleware(__middleware);
            self
        }

        /// List tasks in their current order.
        ///
        /// Call: `tasks.list().await?`.
        ///
        /// Request: `GET https://tasks.example.com/api/tasks`.
        ///
        /// Options:
        ///
        /// - `.done(false)`: accepts [`bool`], &[`bool`], or an [`Option`]<[`bool`]>. `None` clears it. Filter tasks by completion state.
        ///
        /// `.await` gives [`tasks::Result`]<[`Vec`]<[`Task`]>>.
        pub fn list(&self) -> tasks::List {
            tasks::List {
                __client: self.clone(),
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
        ///
        /// - `task`: [`NewTask`] or &[`NewTask`]. The task to create.
        ///
        /// Retry: an unmarked `POST` may be sent again when the policy lists the
        /// response status with `Retry::not_applied_on`; all other retry checks still apply.
        ///
        /// `.await` gives [`tasks::Result`]<[`Task`]>.
        pub fn create(&self, task: impl ::concord::arg::Arg<NewTask>) -> tasks::Create {
            tasks::Create {
                __client: self.clone(),
                task: task.into_input(),
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
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the task to fetch.
        ///
        /// `.await` gives [`tasks::Result`]<[`Task`]>.
        pub fn get(&self, id: impl ::concord::arg::Arg<u64>) -> tasks::Get {
            tasks::Get {
                __client: self.clone(),
                id: id.into_input(),
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
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the task to delete.
        ///
        /// `.await` gives [`tasks::Result`]<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
        pub fn delete(&self, id: impl ::concord::arg::Arg<u64>) -> tasks::Delete {
            tasks::Delete {
                __client: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }
    }

    impl ::std::fmt::Debug for Tasks {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Tasks").finish_non_exhaustive()
        }
    }

    // /// List tasks in their current order.
    // GET list(
    //     /// Filter tasks by completion state.
    //     done?: bool,
    // ) -> Vec<Task> {
    //     query { done; }
    // }
    /// The options of this endpoint.
    impl tasks::List {
        /// Filter tasks by completion state.
        ///
        /// Accepts [`bool`], &[`bool`], or an [`Option`]<[`bool`]>. `None` clears it.
        pub fn done(mut self, done: impl ::concord::arg::Arg<::core::option::Option<bool>>) -> Self {
            self.done = done.into_input();
            self
        }
    }

    impl tasks::List {
        fn __build_draft(&self) -> tasks::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://tasks.example.com/api/tasks",
            )]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let done = &self.done;
                __draft.query(
                    "done",
                    &[::concord::__private::Piece::Val(
                        "done",
                        &(&::concord::__private::ValueRef(done)).adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> tasks::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, "tasks.list")
            } else {
                __draft.finish(&self.__client.__core, &self.__options, "tasks.list")
            }
        }

        const __ENDPOINT: &'static str = "tasks.list";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> tasks::Result<::concord::Answer<Vec<Task>>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Vec<Task>, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => return ::core::result::Result::Err(::concord::__private::refused_plain(__response, &__info).await),
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(self) -> tasks::Result<::concord::__private::Prepared<Vec<Task>, ()>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        tasks::List,
        Vec<Task>,
        (),
        tasks::Result,
        "[`tasks::Result`]<[`Vec`]<[`Task`]>>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for tasks::List {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("List")
                .field("endpoint", &"tasks.list")
                .finish_non_exhaustive()
        }
    }

    // /// Create a task with a title.
    // POST create(
    //     /// The task to create.
    //     task: NewTask,
    // ) -> Task {
    //     body json(task);
    // }
    impl tasks::Create {
        fn __build_draft(&self) -> tasks::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::POST);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://tasks.example.com/api/tasks",
            )]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let task = &self.task;
                #[allow(unused_braces, clippy::redundant_closure_call)]
                let __body = (|| ::std::result::Result::Ok({ json(task) }))();
                let __body = match __body {
                    ::std::result::Result::Ok(__value) => __value,
                    ::std::result::Result::Err(__error) => {
                        return ::std::result::Result::Err(::concord::__private::build_error(
                            Self::__ENDPOINT,
                            __error,
                        ))
                    }
                };
                __draft.body(__body);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> tasks::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, "tasks.create")
            } else {
                __draft.finish(&self.__client.__core, &self.__options, "tasks.create")
            }
        }

        const __ENDPOINT: &'static str = "tasks.create";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> tasks::Result<::concord::Answer<Task>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Task, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => return ::core::result::Result::Err(::concord::__private::refused_plain(__response, &__info).await),
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(self) -> tasks::Result<::concord::__private::Prepared<Task, ()>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        tasks::Create,
        Task,
        (),
        tasks::Result,
        "[`tasks::Result`]<[`Task`]>",
        awaitable,
        unconditional
    );

    impl ::std::fmt::Debug for tasks::Create {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Create")
                .field("endpoint", &"tasks.create")
                .finish_non_exhaustive()
        }
    }

    // /// Fetch one task by its identifier.
    // GET get(
    //     /// The identifier of the task to fetch.
    //     id: u64,
    // ) -> Task {
    //     path "{id}";
    // }
    impl tasks::Get {
        fn __build_draft(&self) -> tasks::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://tasks.example.com/api/tasks",
            )]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> tasks::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, "tasks.get")
            } else {
                __draft.finish(&self.__client.__core, &self.__options, "tasks.get")
            }
        }

        const __ENDPOINT: &'static str = "tasks.get";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> tasks::Result<::concord::Answer<Task>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client;
                            (|| ::concord::__private::reader::<Task, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => return ::core::result::Result::Err(::concord::__private::refused_plain(__response, &__info).await),
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(self) -> tasks::Result<::concord::__private::Prepared<Task, ()>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        tasks::Get,
        Task,
        (),
        tasks::Result,
        "[`tasks::Result`]<[`Task`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for tasks::Get {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Get")
                .field("endpoint", &"tasks.get")
                .finish_non_exhaustive()
        }
    }

    // /// Delete one task by its identifier.
    // DELETE delete(
    //     /// The identifier of the task to delete.
    //     id: u64,
    // ) {
    //     path "{id}";
    // }
    impl tasks::Delete {
        fn __build_draft(&self) -> tasks::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::DELETE);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://tasks.example.com/api/tasks",
            )]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> tasks::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, "tasks.delete")
            } else {
                __draft.finish(&self.__client.__core, &self.__options, "tasks.delete")
            }
        }

        const __ENDPOINT: &'static str = "tasks.delete";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> tasks::Result<::concord::Answer<()>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            match __status {
                __status if __status.is_success() => (),
                _ => return ::core::result::Result::Err(::concord::__private::refused_plain(__response, &__info).await),
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, ()).with_response_url(__response_url))
        }

        fn __prepare_inner(self) -> tasks::Result<::concord::__private::Prepared<(), ()>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    ::concord::__private::call_methods!(
        tasks::Delete,
        (),
        (),
        tasks::Result,
        "[`tasks::Result`]<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>",
        awaitable,
        unconditional
    );

    impl ::std::fmt::Debug for tasks::Delete {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Delete")
                .field("endpoint", &"tasks.delete")
                .finish_non_exhaustive()
        }
    }
};
