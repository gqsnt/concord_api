const _: () = {
    use ::concord::__private::AdaptValue as _;

    // This client's owner view has no inputs.
    struct __Client;
    use concord::{by_cursor, by_link, Json};

    // /// A client for a feed whose pages are linked by cursors.
    // ///
    // /// # Example
    // ///
    // /// ```no_run
    // /// use tour::Feed;
    // ///
    // /// # async fn example() -> tour::feed::Result<()> {
    // /// let feed = Feed::new();
    // /// let mut pages = feed.by_cursor().pages();
    // /// while let Some(page) = pages.try_next().await? {
    // ///     for entry in page { println!("{}", entry.name); }
    // /// }
    // /// # Ok(())
    // /// # }
    // /// ```
    // ///
    // pub client Feed {
    //     read Json;
    //     base "https://feed.example.com";
    //     /// List feed entries using the cursor returned by the server.
    //     GET list(
    //         /// Continue after this cursor.
    //         starting_after?: String,
    //     ) -> Page {
    //         path "items";
    //         query { starting_after; }
    //         paginate move |call, page| page.value().next_cursor().map(|cursor| call.starting_after(cursor));
    //     }
    //     /// List feed entries with an ordinary cursor pager.
    //     GET by_cursor(
    //         /// Continue after this cursor.
    //         starting_after?: String,
    //     ) -> Page {
    //         path "cursor-items";
    //         query { starting_after; }
    //         paginate by_cursor(starting_after, page_cursor);
    //     }
    //     /// List feed entries by following the next Link header.
    //     GET linked() -> Page {
    //         path "linked-items";
    //         paginate by_link();
    //     }
    // }
    impl feed::__Types for () {
        type __Input0 = ::core::option::Option<String>;
        type __Problem = ();
        type __Input1 = ::core::option::Option<String>;
    }

    impl ::concord::page::Slot<feed::Call<feed::List>> for feed::__ListStartingAfter {
        type Value = String;
        fn get<'a>(
            &self,
            __call: &'a feed::Call<feed::List>,
        ) -> ::core::option::Option<&'a Self::Value> {
            __call.__inputs.starting_after.as_ref()
        }
        fn set(
            &self,
            mut __call: feed::Call<feed::List>,
            __value: Self::Value,
        ) -> feed::Call<feed::List> {
            __call.__inputs.starting_after = ::core::option::Option::Some(__value);
            __call
        }
        fn input(&self) -> ::concord::page::InputId {
            ::concord::page::InputId::new(0)
        }
    }

    impl ::concord::page::Slot<feed::Call<feed::ByCursor>> for feed::__ByCursorStartingAfter {
        type Value = String;
        fn get<'a>(
            &self,
            __call: &'a feed::Call<feed::ByCursor>,
        ) -> ::core::option::Option<&'a Self::Value> {
            __call.__inputs.starting_after.as_ref()
        }
        fn set(
            &self,
            mut __call: feed::Call<feed::ByCursor>,
            __value: Self::Value,
        ) -> feed::Call<feed::ByCursor> {
            __call.__inputs.starting_after = ::core::option::Option::Some(__value);
            __call
        }
        fn input(&self) -> ::concord::page::InputId {
            ::concord::page::InputId::new(0)
        }
    }

    impl Feed {
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

        /// List feed entries using the cursor returned by the server.
        ///
        /// Call: `feed.list().send().await?`.
        ///
        /// Request: `GET https://feed.example.com/items`.
        ///
        /// Options:
        ///
        /// - `.starting_after("next")`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. Continue after this cursor.
        ///
        /// `.send().await` returns [`feed::Result`]<[`Page`]>.
        ///
        /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
        #[must_use = "await `.send()` for one page, or use `.pages()` for all pages"]
        pub fn list(&self) -> feed::Call<feed::List> {
            feed::Call::__new(feed::__ListInputs {
                __client: self.clone(),
                starting_after: ::core::option::Option::None,
            })
        }

        /// List feed entries with an ordinary cursor pager.
        ///
        /// Call: `feed.by_cursor().send().await?`.
        ///
        /// Request: `GET https://feed.example.com/cursor-items`.
        ///
        /// Options:
        ///
        /// - `.starting_after("next")`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. Continue after this cursor.
        ///
        /// `.send().await` returns [`feed::Result`]<[`Page`]>.
        ///
        /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
        #[must_use = "await `.send()` for one page, or use `.pages()` for all pages"]
        pub fn by_cursor(&self) -> feed::Call<feed::ByCursor> {
            feed::Call::__new(feed::__ByCursorInputs {
                __client: self.clone(),
                starting_after: ::core::option::Option::None,
            })
        }

        /// List feed entries by following the next Link header.
        ///
        /// Call: `feed.linked().send().await?`.
        ///
        /// Request: `GET https://feed.example.com/linked-items`.
        ///
        /// `.send().await` returns [`feed::Result`]<[`Page`]>.
        ///
        /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
        #[must_use = "await `.send()` for one page, or use `.pages()` for all pages"]
        pub fn linked(&self) -> feed::Call<feed::Linked> {
            feed::Call::__new(feed::__LinkedInputs {
                __client: self.clone(),
            })
        }
    }

    impl ::std::fmt::Debug for Feed {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Feed").finish_non_exhaustive()
        }
    }

    // /// List feed entries using the cursor returned by the server.
    // GET list(
    //     /// Continue after this cursor.
    //     starting_after?: String,
    // ) -> Page {
    //     path "items";
    //     query { starting_after; }
    //     paginate move |call, page| page.value().next_cursor().map(|cursor| call.starting_after(cursor));
    // }
    /// The options of this endpoint.
    #[doc(hidden)]
    impl feed::Call<feed::List> {
        /// Continue after this cursor.
        ///
        /// Accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it.
        pub fn starting_after(
            mut self,
            starting_after: impl ::concord::arg::Arg<::core::option::Option<String>>,
        ) -> Self {
            self.__inputs.starting_after = starting_after.into_input();
            self
        }
    }

    impl feed::List {
        /// Option `starting_after` of `list`.
        ///
        /// The first argument is the call.
        ///
        /// Continue after this cursor.
        ///
        /// Accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it.
        pub fn starting_after(
            __call: feed::Call<feed::List>,
            starting_after: impl ::concord::arg::Arg<::core::option::Option<String>>,
        ) -> feed::Call<feed::List> {
            __call.starting_after(starting_after)
        }
    }

    impl feed::Call<feed::List> {
        fn __build_draft(&self) -> feed::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit("https://feed.example.com")]);
            __draft.segment(&[::concord::__private::Piece::Lit("items")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let starting_after = &self.__inputs.starting_after;
                __draft.query(
                    "starting_after",
                    &[::concord::__private::Piece::Val(
                        "starting_after",
                        &(&::concord::__private::ValueRef(starting_after)).__adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> feed::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__inputs.__client.__core, &self.__options, "feed.list")
            } else {
                __draft.finish(&self.__inputs.__client.__core, &self.__options, "feed.list")
            }
        }

        const __ENDPOINT: &'static str = <feed::List as feed::Endpoint>::NAME;

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> feed::Result<::concord::Answer<Page>> {
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
                            (|| ::concord::__private::reader::<Page, _>(Json))()
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

        fn __prepare_inner(self) -> feed::Result<::concord::__private::Prepared<Page, ()>> {
            let __run = ::concord::__private::Run::start(
                &self.__inputs.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__inputs.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    impl feed::Endpoint for feed::List {
        type Value = Page;
        const NAME: &'static str = "feed.list";
    }

    call_spec!(feed, feed::List, feed::__ListInputs, Page, (), "List");
    impl feed::__spec::Paged for feed::List {
        fn pages(call: feed::Call<feed::List>) -> ::concord::Pages<Page> {
            // The owner in reach, then each input as its place; the expression is
            // copied as written.
            ::concord::__private::pages(call, {
                #[allow(unused_variables)]
                let client = __Client;
                #[allow(unused_variables)]
                let starting_after = feed::__ListStartingAfter;
                move |call, page| page.value().next_cursor().map(|cursor| call.starting_after(cursor))
            })
        }
    }
    impl feed::__spec::Conditional for feed::List {}

    // /// List feed entries with an ordinary cursor pager.
    // GET by_cursor(
    //     /// Continue after this cursor.
    //     starting_after?: String,
    // ) -> Page {
    //     path "cursor-items";
    //     query { starting_after; }
    //     paginate by_cursor(starting_after, page_cursor);
    // }
    /// The options of this endpoint.
    #[doc(hidden)]
    impl feed::Call<feed::ByCursor> {
        /// Continue after this cursor.
        ///
        /// Accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it.
        pub fn starting_after(
            mut self,
            starting_after: impl ::concord::arg::Arg<::core::option::Option<String>>,
        ) -> Self {
            self.__inputs.starting_after = starting_after.into_input();
            self
        }
    }

    impl feed::ByCursor {
        /// Option `starting_after` of `by_cursor`.
        ///
        /// The first argument is the call.
        ///
        /// Continue after this cursor.
        ///
        /// Accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it.
        pub fn starting_after(
            __call: feed::Call<feed::ByCursor>,
            starting_after: impl ::concord::arg::Arg<::core::option::Option<String>>,
        ) -> feed::Call<feed::ByCursor> {
            __call.starting_after(starting_after)
        }
    }

    impl feed::Call<feed::ByCursor> {
        fn __build_draft(&self) -> feed::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit("https://feed.example.com")]);
            __draft.segment(&[::concord::__private::Piece::Lit("cursor-items")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let starting_after = &self.__inputs.starting_after;
                __draft.query(
                    "starting_after",
                    &[::concord::__private::Piece::Val(
                        "starting_after",
                        &(&::concord::__private::ValueRef(starting_after)).__adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> feed::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__inputs.__client.__core, &self.__options, "feed.by_cursor")
            } else {
                __draft.finish(&self.__inputs.__client.__core, &self.__options, "feed.by_cursor")
            }
        }

        const __ENDPOINT: &'static str = <feed::ByCursor as feed::Endpoint>::NAME;

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> feed::Result<::concord::Answer<Page>> {
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
                            (|| ::concord::__private::reader::<Page, _>(Json))()
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

        fn __prepare_inner(self) -> feed::Result<::concord::__private::Prepared<Page, ()>> {
            let __run = ::concord::__private::Run::start(
                &self.__inputs.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__inputs.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    impl feed::Endpoint for feed::ByCursor {
        type Value = Page;
        const NAME: &'static str = "feed.by_cursor";
    }

    call_spec!(
        feed,
        feed::ByCursor,
        feed::__ByCursorInputs,
        Page,
        (),
        "ByCursor"
    );
    impl feed::__spec::Paged for feed::ByCursor {
        fn pages(call: feed::Call<feed::ByCursor>) -> ::concord::Pages<Page> {
            ::concord::__private::pages_of_pager(call, || {
                #[allow(unused_variables)]
                let client = __Client;
                #[allow(unused_variables)]
                let starting_after = feed::__ByCursorStartingAfter;
                by_cursor(starting_after, page_cursor)
            })
        }
    }
    impl feed::__spec::Conditional for feed::ByCursor {}

    // /// List feed entries by following the next Link header.
    // GET linked(
    // ) -> Page {
    //     path "linked-items";
    //     paginate by_link();
    // }

    impl feed::Call<feed::Linked> {
        fn __build_draft(&self) -> feed::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit("https://feed.example.com")]);
            __draft.segment(&[::concord::__private::Piece::Lit("linked-items")]);

            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> feed::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__inputs.__client.__core, &self.__options, "feed.linked")
            } else {
                __draft.finish(&self.__inputs.__client.__core, &self.__options, "feed.linked")
            }
        }

        const __ENDPOINT: &'static str = <feed::Linked as feed::Endpoint>::NAME;

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> feed::Result<::concord::Answer<Page>> {
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
                            (|| ::concord::__private::reader::<Page, _>(Json))()
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

        fn __prepare_inner(self) -> feed::Result<::concord::__private::Prepared<Page, ()>> {
            let __run = ::concord::__private::Run::start(
                &self.__inputs.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__inputs.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    impl feed::Endpoint for feed::Linked {
        type Value = Page;
        const NAME: &'static str = "feed.linked";
    }

    call_spec!(feed, feed::Linked, feed::__LinkedInputs, Page, (), "Linked");
    impl feed::__spec::Paged for feed::Linked {
        fn pages(call: feed::Call<feed::Linked>) -> ::concord::Pages<Page> {
            ::concord::__private::pages_of_pager(call, || {
                #[allow(unused_variables)]
                let client = __Client;
                by_link()
            })
        }
    }
    impl feed::__spec::Conditional for feed::Linked {}

};
