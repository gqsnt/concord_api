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

    impl ::concord::page::Slot<feed::List> for feed::__ListStartingAfter {
        type Value = String;
        fn get<'a>(&self, __call: &'a feed::List) -> ::core::option::Option<&'a Self::Value> {
            __call.starting_after.as_ref()
        }
        fn set(&self, mut __call: feed::List, __value: Self::Value) -> feed::List {
            __call.starting_after = ::core::option::Option::Some(__value);
            __call
        }
    }

    impl ::concord::page::Slot<feed::ByCursor> for feed::__ByCursorStartingAfter {
        type Value = String;
        fn get<'a>(&self, __call: &'a feed::ByCursor) -> ::core::option::Option<&'a Self::Value> {
            __call.starting_after.as_ref()
        }
        fn set(&self, mut __call: feed::ByCursor, __value: Self::Value) -> feed::ByCursor {
            __call.starting_after = ::core::option::Option::Some(__value);
            __call
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
        /// `.send().await` gives [`feed::Result`]<[`Page`]>.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        pub fn list(&self) -> feed::List {
            feed::List {
                __client: self.clone(),
                starting_after: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
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
        /// `.send().await` gives [`feed::Result`]<[`Page`]>.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        pub fn by_cursor(&self) -> feed::ByCursor {
            feed::ByCursor {
                __client: self.clone(),
                starting_after: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
        }

        /// List feed entries by following the next Link header.
        ///
        /// Call: `feed.linked().send().await?`.
        ///
        /// Request: `GET https://feed.example.com/linked-items`.
        ///
        /// `.send().await` gives [`feed::Result`]<[`Page`]>.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        pub fn linked(&self) -> feed::Linked {
            feed::Linked {
                __client: self.clone(),
                __options: ::concord::__private::Options::default(),
            }
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
    impl feed::List {
        /// Continue after this cursor.
        ///
        /// Accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it.
        pub fn starting_after(
            mut self,
            starting_after: impl ::concord::arg::Arg<::core::option::Option<String>>,
        ) -> Self {
            self.starting_after = starting_after.into_input();
            self
        }
    }

    impl feed::List {
        fn __build_draft(&self) -> feed::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit("https://feed.example.com")]);
            __draft.segment(&[::concord::__private::Piece::Lit("items")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let starting_after = &self.starting_after;
                __draft.query(
                    "starting_after",
                    &[::concord::__private::Piece::Val(
                        "starting_after",
                        &(&::concord::__private::ValueRef(starting_after)).adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> feed::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, "feed.list")
            } else {
                __draft.finish(&self.__client.__core, &self.__options, "feed.list")
            }
        }

        const __ENDPOINT: &'static str = "feed.list";

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
        feed::List,
        Page,
        (),
        feed::Result,
        "[`feed::Result`]<[`Page`]>",
        paged, conditional,
        pages {
            /// Iterate over [`Page`] pages.
            ///
            /// `try_next().await` gives [`feed::Result`]<[`Option`]<[`Page`]>>.
            /// Each page is one call. Nothing is asked until iteration starts.
            pub fn pages(self) -> ::concord::Pages<Page> {
                // The owner in reach, then each input as its place; the expression is
                // copied as written.
                ::concord::__private::pages(self, {
                    #[allow(unused_variables)]
                    let client = __Client;
                    #[allow(unused_variables)]
                    let starting_after = feed::__ListStartingAfter;
                    move |call, page| page.value().next_cursor().map(|cursor| call.starting_after(cursor))
                })
            }

        }
    );

    impl ::std::fmt::Debug for feed::List {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("List")
                .field("endpoint", &"feed.list")
                .finish_non_exhaustive()
        }
    }

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
    impl feed::ByCursor {
        /// Continue after this cursor.
        ///
        /// Accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it.
        pub fn starting_after(
            mut self,
            starting_after: impl ::concord::arg::Arg<::core::option::Option<String>>,
        ) -> Self {
            self.starting_after = starting_after.into_input();
            self
        }
    }

    impl feed::ByCursor {
        fn __build_draft(&self) -> feed::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit("https://feed.example.com")]);
            __draft.segment(&[::concord::__private::Piece::Lit("cursor-items")]);
            #[allow(unused_variables)]
            {
                let client = __Client;
                let starting_after = &self.starting_after;
                __draft.query(
                    "starting_after",
                    &[::concord::__private::Piece::Val(
                        "starting_after",
                        &(&::concord::__private::ValueRef(starting_after)).adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> feed::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, "feed.by_cursor")
            } else {
                __draft.finish(&self.__client.__core, &self.__options, "feed.by_cursor")
            }
        }

        const __ENDPOINT: &'static str = "feed.by_cursor";

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
        feed::ByCursor,
        Page,
        (),
        feed::Result,
        "[`feed::Result`]<[`Page`]>",
        paged, conditional,
        pages {
            /// Iterate over [`Page`] pages.
            ///
            /// `try_next().await` gives [`feed::Result`]<[`Option`]<[`Page`]>>.
            /// Each page is one call. Nothing is asked until iteration starts.
            pub fn pages(self) -> ::concord::Pages<Page> {
                ::concord::__private::pages(self, {
                    #[allow(unused_variables)]
                    let client = __Client;
                    #[allow(unused_variables)]
                    let starting_after = feed::__ByCursorStartingAfter;
                    by_cursor(starting_after, page_cursor)
                })
            }

        }
    );

    impl ::std::fmt::Debug for feed::ByCursor {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("ByCursor")
                .field("endpoint", &"feed.by_cursor")
                .finish_non_exhaustive()
        }
    }

    // /// List feed entries by following the next Link header.
    // GET linked(
    // ) -> Page {
    //     path "linked-items";
    //     paginate by_link();
    // }

    impl feed::Linked {
        fn __build_draft(&self) -> feed::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit("https://feed.example.com")]);
            __draft.segment(&[::concord::__private::Piece::Lit("linked-items")]);

            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> feed::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, "feed.linked")
            } else {
                __draft.finish(&self.__client.__core, &self.__options, "feed.linked")
            }
        }

        const __ENDPOINT: &'static str = "feed.linked";

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
        feed::Linked,
        Page,
        (),
        feed::Result,
        "[`feed::Result`]<[`Page`]>",
        paged, conditional,
        pages {
            /// Iterate over [`Page`] pages.
            ///
            /// `try_next().await` gives [`feed::Result`]<[`Option`]<[`Page`]>>.
            /// Each page is one call. Nothing is asked until iteration starts.
            pub fn pages(self) -> ::concord::Pages<Page> {
                ::concord::__private::pages(self, {
                    #[allow(unused_variables)]
                    let client = __Client;
                    by_link()
                })
            }

        }
    );

    impl ::std::fmt::Debug for feed::Linked {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Linked")
                .field("endpoint", &"feed.linked")
                .finish_non_exhaustive()
        }
    }
};
