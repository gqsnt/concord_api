const _: () = {
    use ::concord::__private::AdaptValue as _;

    #[allow(dead_code)]
    struct __Client<'a> {
        key: &'a Secret,
    }
    use concord::read::Stream;
    use concord::{json, Json, Text};

    // /// A client for documents, file contents, and file search.
    // ///
    // /// # Example
    // ///
    // /// ```no_run
    // /// use tour::{DocId, Files};
    // ///
    // /// # async fn example() -> tour::files::Result<()> {
    // /// let files = Files::new("key");
    // /// let exists = files.exists(DocId::from("readme")).await?;
    // /// # Ok(())
    // /// # }
    // /// ```
    // ///
    // pub client Files(
    //     /// The key used to sign every request.
    //     key: Secret,
    // ) {
    //     read Json;
    //     base "https://files.example.com";
    //     headers { "authorization" = "Bearer {client.key}"; "x-client" = "tour".to_owned(); }
    //     /// Signs every request with the client key.
    //     middleware Signed::new(key.clone());
    //     /// Is it there?
    //     HEAD exists(
    //         /// The identifier of the document to check.
    //         id: DocId,
    //     ) -> bool {
    //         path "{id}";
    //         status { 2xx => true, 404 => false }
    //     }
    //     /// Plain text, not JSON.
    //     GET readme(
    //         /// The repository whose README should be read.
    //         repo: String,
    //     ) -> String {
    //         path "{repo}/readme";
    //         read Text;
    //     }
    //     /// A large file, as it comes.
    //     GET download(
    //         /// The identifier of the file to download.
    //         id: FileId,
    //     ) -> concord::ByteStream {
    //         path "files/{id}";
    //         read Stream;
    //         idle_timeout std::time::Duration::from_secs(120);
    //     }
    //     /// An upload, on another host.
    //     PUT upload(
    //         /// The name under which to store the file.
    //         name: String,
    //         /// The file contents and their content type.
    //         file: concord::Body,
    //     ) {
    //         base "https://uploads.example.com";
    //         headers { override "authorization" = "Bearer {client.key}"; }
    //         path "files/{name}";
    //         body file;
    //         idempotent false;
    //     }
    //     /// A body made of several inputs. A `cursor` that was not set is left out.
    //     POST search(
    //         /// The text to search for.
    //         text: String,
    //         /// Continue the search after this cursor.
    //         cursor?: String,
    //     ) -> Page {
    //         path "search";
    //         body json { text; cursor; }
    //     }
    // }
    impl files::__Types for () {
        type __Input0 = Secret;
        type __Input1 = DocId;
        type __Input2 = String;
        type __Input3 = FileId;
        type __Input4 = String;
        type __Input5 = ::concord::Body;
        type __Input6 = String;
        type __Input7 = ::core::option::Option<String>;
        type __Input8 = String;
        type __Input9 = String;
        type __Input10 = Secret;
        type __Problem = ();
    }

    impl Files {
        /// Create a client with the default transport and settings.
        ///
        /// `key`: [`Secret`], &[`Secret`], [`String`], or &[`str`]. The key used to sign every request.
        pub fn new(key: impl ::concord::arg::Arg<Secret>) -> Self {
            let key = key.into_input();
            let __core = {
                // Middleware expressions see the owner `client` and the required inputs
                // of the client, as references.
                #[allow(unused_variables)]
                let client = __Client { key: &key };
                #[allow(unused_variables)]
                let key = client.key;
                ::concord::__private::Core::default().add_middleware(Signed::new(key.clone()))
            };
            Self { __core, key }
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

        /// Is it there?
        ///
        /// Call: `files.exists(id).await?`.
        ///
        /// Request: `HEAD https://files.example.com/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`DocId`] or &[`DocId`]. The identifier of the document to check.
        ///
        /// Status: `2xx`.
        ///
        /// Status: `404`.
        ///
        /// `.await` gives [`files::Result`]<[`bool`]>.
        pub fn exists(&self, id: impl ::concord::arg::Arg<DocId>) -> files::Exists {
            files::Exists {
                __client: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// Plain text, not JSON.
        ///
        /// Call: `files.readme(repo).await?`.
        ///
        /// Request: `GET https://files.example.com/{repo}/readme`.
        ///
        /// Inputs:
        ///
        /// - `repo`: [`String`], &[`String`], or &[`str`]. The repository whose README should be read.
        ///
        /// `.await` gives [`files::Result`]<[`String`]>.
        pub fn readme(&self, repo: impl ::concord::arg::Arg<String>) -> files::Readme {
            files::Readme {
                __client: self.clone(),
                repo: repo.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// A large file, as it comes.
        ///
        /// Call: `files.download(id).await?`.
        ///
        /// Request: `GET https://files.example.com/files/{id}`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`FileId`] or &[`FileId`]. The identifier of the file to download.
        ///
        /// `.await` gives [`files::Result`]<[`concord::ByteStream`]>.
        pub fn download(&self, id: impl ::concord::arg::Arg<FileId>) -> files::Download {
            files::Download {
                __client: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// An upload, on another host.
        ///
        /// Call: `files.upload(name, file).await?`.
        ///
        /// Request: `PUT https://uploads.example.com/files/{name}`.
        ///
        /// Inputs:
        ///
        /// - `name`: [`String`], &[`String`], or &[`str`]. The name under which to store the file.
        /// - `file`: [`concord::Body`] or &[`concord::Body`]. The file contents and their content type.
        ///   Make one with [`concord::Body::new(bytes, "application/octet-stream")`](concord::Body::new)
        ///   or [`concord::Body::stream(stream)`](concord::Body::stream); a stream is sent once.
        ///
        /// Retry: `idempotent false` disables method-based repetition. Only a status
        /// explicitly listed by `Retry::not_applied_on` can permit another try;
        /// a one-shot body still cannot be sent again.
        ///
        /// `.await` gives [`files::Result`]<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
        pub fn upload(
            &self,
            name: impl ::concord::arg::Arg<String>,
            file: impl ::concord::arg::Arg<::concord::Body>,
        ) -> files::Upload {
            files::Upload {
                __client: self.clone(),
                name: name.into_input(),
                file: file.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// A body made of several inputs. A `cursor` that was not set is left out.
        ///
        /// Call: `files.search(text).await?`.
        ///
        /// Request: `POST https://files.example.com/search`.
        ///
        /// Inputs:
        ///
        /// - `text`: [`String`], &[`String`], or &[`str`]. The text to search for.
        ///
        /// Options:
        ///
        /// - `.cursor("next")`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. Continue the search after this cursor.
        ///   When it is not set, the body has no `cursor` member.
        ///
        /// Retry: an unmarked `POST` may be sent again when the policy lists the
        /// response status with `Retry::not_applied_on`; all other retry checks still apply.
        ///
        /// `.await` gives [`files::Result`]<[`Page`]>.
        pub fn search(&self, text: impl ::concord::arg::Arg<String>) -> files::Search {
            files::Search {
                __client: self.clone(),
                text: text.into_input(),
                cursor: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
        }
    }

    impl ::std::fmt::Debug for Files {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Files").finish_non_exhaustive()
        }
    }

    // /// Is it there?
    // HEAD exists(
    //     /// The identifier of the document to check.
    //     id: DocId,
    // ) -> bool {
    //     path "{id}";
    //     status { 2xx => true, 404 => false }
    // }
    impl files::Exists {
        fn __build_draft(&self) -> files::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::HEAD);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://files.example.com",
            )]);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__client.key,
                };
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            // Headers follow paths; entries within this kind run in tree order.
            // Both entries are the client's: `client`, then its input.
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__client.key,
                };
                let key = client.key;
                __draft.header(
                    "authorization",
                    &[
                        ::concord::__private::Piece::Lit("Bearer "),
                        ::concord::__private::Piece::Val(
                            "client.key",
                            &(&::concord::__private::ValueRef(client.key)).adapt(),
                        ),
                    ],
                );
                let __entry = "tour".to_owned();
                __draft.header(
                    "x-client",
                    &[::concord::__private::Piece::Val(
                        "x-client",
                        &(&::concord::__private::ValueRef(&__entry)).adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> files::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, "files.exists")
            } else {
                __draft.finish(&self.__client.__core, &self.__options, "files.exists")
            }
        }

        const __ENDPOINT: &'static str = "files.exists";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> files::Result<::concord::Answer<bool>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            #[allow(unused_variables)]
            let __value = match __status {
                ::concord::http::StatusCode::NOT_FOUND => {
                    // The arms of the endpoint: the owner in reach, then its inputs.
                    let client = __Client { key: &self.__client.key };
                    let id = &self.id;
                    #[allow(unused_braces)]
                    let __arm = || -> ::std::result::Result<bool, ::concord::error::Fail<()>> {
                        ::core::result::Result::Ok({ false })
                    };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                __status if __status.is_success() => {
                    // The arms of the endpoint: the owner in reach, then its inputs.
                    let client = __Client { key: &self.__client.key };
                    let id = &self.id;
                    #[allow(unused_braces)]
                    let __arm = || -> ::std::result::Result<bool, ::concord::error::Fail<()>> {
                        ::core::result::Result::Ok({ true })
                    };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                _ => return ::core::result::Result::Err(::concord::__private::refused_plain(__response, &__info).await),
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(self) -> files::Result<::concord::__private::Prepared<bool, ()>> {
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
        files::Exists,
        bool,
        (),
        files::Result,
        "[`files::Result`]<[`bool`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for files::Exists {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Exists")
                .field("endpoint", &"files.exists")
                .finish_non_exhaustive()
        }
    }

    // /// Plain text, not JSON.
    // GET readme(
    //     /// The repository whose README should be read.
    //     repo: String,
    // ) -> String {
    //     path "{repo}/readme";
    //     read Text;
    // }
    impl files::Readme {
        fn __build_draft(&self) -> files::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://files.example.com",
            )]);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__client.key,
                };
                let repo = &self.repo;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "repo",
                    &(&::concord::__private::ValueRef(repo)).adapt(),
                )]);
            }
            __draft.segment(&[::concord::__private::Piece::Lit("readme")]);
            // Headers follow paths; entries within this kind run in tree order.
            // Both entries are the client's: `client`, then its input.
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__client.key,
                };
                let key = client.key;
                __draft.header(
                    "authorization",
                    &[
                        ::concord::__private::Piece::Lit("Bearer "),
                        ::concord::__private::Piece::Val(
                            "client.key",
                            &(&::concord::__private::ValueRef(client.key)).adapt(),
                        ),
                    ],
                );
                let __entry = "tour".to_owned();
                __draft.header(
                    "x-client",
                    &[::concord::__private::Piece::Val(
                        "x-client",
                        &(&::concord::__private::ValueRef(&__entry)).adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> files::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, "files.readme")
            } else {
                __draft.finish(&self.__client.__core, &self.__options, "files.readme")
            }
        }

        const __ENDPOINT: &'static str = "files.readme";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> files::Result<::concord::Answer<String>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client { key: &self.__client.key };
                            let repo = &self.repo;
                            (|| ::concord::__private::reader::<_, _>(Text))()
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

        fn __prepare_inner(self) -> files::Result<::concord::__private::Prepared<String, ()>> {
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
        files::Readme,
        String,
        (),
        files::Result,
        "[`files::Result`]<[`String`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for files::Readme {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Readme")
                .field("endpoint", &"files.readme")
                .finish_non_exhaustive()
        }
    }

    // /// A large file, as it comes.
    // GET download(
    //     /// The identifier of the file to download.
    //     id: FileId,
    // ) -> concord::ByteStream {
    //     path "files/{id}";
    //     read Stream;
    //     idle_timeout std::time::Duration::from_secs(120);
    // }
    impl files::Download {
        fn __build_draft(&self) -> files::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://files.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("files")]);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__client.key,
                };
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            // Headers follow paths; entries within this kind run in tree order.
            // Both entries are the client's: `client`, then its input.
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__client.key,
                };
                let key = client.key;
                __draft.header(
                    "authorization",
                    &[
                        ::concord::__private::Piece::Lit("Bearer "),
                        ::concord::__private::Piece::Val(
                            "client.key",
                            &(&::concord::__private::ValueRef(client.key)).adapt(),
                        ),
                    ],
                );
                let __entry = "tour".to_owned();
                __draft.header(
                    "x-client",
                    &[::concord::__private::Piece::Val(
                        "x-client",
                        &(&::concord::__private::ValueRef(&__entry)).adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> files::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, "files.download")
            } else {
                __draft.finish(&self.__client.__core, &self.__options, "files.download")
            }
        }

        const __ENDPOINT: &'static str = "files.download";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> files::Result<::concord::Answer<::concord::ByteStream>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client { key: &self.__client.key };
                            let id = &self.id;
                            (|| ::concord::__private::reader::<_, _>(Stream))()
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

        fn __prepare_inner(
            mut self,
        ) -> files::Result<::concord::__private::Prepared<::concord::ByteStream, ()>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            // For each send: `idle_timeout` first of all, inside a non-async closure, after
            // the owner in reach and the inputs of its node; copied as written.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __idle_timeout = {
                let client = __Client {
                    key: &self.__client.key,
                };
                let id = &self.id;
                (|| -> ::std::time::Duration { std::time::Duration::from_secs(120) })()
            };
            self.__options.description_idle_timeout(__idle_timeout);
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
        files::Download,
        ::concord::ByteStream,
        (),
        files::Result,
        "[`files::Result`]<[`concord::ByteStream`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for files::Download {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Download")
                .field("endpoint", &"files.download")
                .finish_non_exhaustive()
        }
    }

    // /// An upload, on another host.
    // PUT upload(
    //     /// The name under which to store the file.
    //     name: String,
    //     /// The file contents and their content type.
    //     file: concord::Body,
    // ) {
    //     base "https://uploads.example.com";
    //     headers { override "authorization" = "Bearer {client.key}"; }
    //     path "files/{name}";
    //     body file;
    //     idempotent false;
    // }
    impl files::Upload {
        fn __build_draft(&self) -> files::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::PUT);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://uploads.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("files")]);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__client.key,
                };
                let name = &self.name;
                let file = &self.file;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "name",
                    &(&::concord::__private::ValueRef(name)).adapt(),
                )]);
            }
            // The root base already bound authorization. This second base,
            // even if its text were identical, requires the explicit override.
            // A secret path would instead be written under each base that needs it.
            // Evaluate the surviving root value before the child override.
            // Emit in the first-written header order: authorization, x-client.
            // Both entries are the client's: `client`, then its input, before its expression.
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__client.key,
                };
                let key = client.key;
                let __entry = "tour".to_owned();
                __draft.header(
                    "authorization",
                    &[
                        ::concord::__private::Piece::Lit("Bearer "),
                        ::concord::__private::Piece::Val(
                            "client.key",
                            &(&::concord::__private::ValueRef(client.key)).adapt(),
                        ),
                    ],
                );
                __draft.header(
                    "x-client",
                    &[::concord::__private::Piece::Val(
                        "x-client",
                        &::concord::__private::inherited(
                            (&::concord::__private::ValueRef(&__entry)).adapt(),
                        ),
                    )],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__client.key,
                };
                let name = &self.name;
                let file = &self.file;
                #[allow(unused_braces, clippy::redundant_closure_call)]
                let __body = (|| ::std::result::Result::Ok({ file }))();
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

        fn __build_request(&self, __take: bool) -> files::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, "files.upload")
            } else {
                __draft.finish(&self.__client.__core, &self.__options, "files.upload")
            }
        }

        const __ENDPOINT: &'static str = "files.upload";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> files::Result<::concord::Answer<()>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            match __status {
                __status if __status.is_success() => (),
                _ => return ::core::result::Result::Err(::concord::__private::refused_plain(__response, &__info).await),
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, ()).with_response_url(__response_url))
        }

        fn __prepare_inner(self) -> files::Result<::concord::__private::Prepared<(), ()>> {
            let __run = ::concord::__private::Run::start(
                &self.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            // docs/internals/macro.md requires the idempotent expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __idempotent = {
                let client = __Client {
                    key: &self.__client.key,
                };
                let name = &self.name;
                let file = &self.file;
                (|| ::concord::__private::idempotent(false))()
            };
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
        files::Upload,
        (),
        (),
        files::Result,
        "[`files::Result`]<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>",
        awaitable,
        unconditional
    );

    impl ::std::fmt::Debug for files::Upload {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Upload")
                .field("endpoint", &"files.upload")
                .finish_non_exhaustive()
        }
    }

    // /// A body made of several inputs. A `cursor` that was not set is left out.
    // POST search(
    //     /// The text to search for.
    //     text: String,
    //     /// Continue the search after this cursor.
    //     cursor?: String,
    // ) -> Page {
    //     path "search";
    //     body json { text; cursor; }
    // }
    /// The options of this endpoint.
    impl files::Search {
        /// Continue the search after this cursor.
        ///
        /// Accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. When not given, the JSON member is absent.
        pub fn cursor(mut self, cursor: impl ::concord::arg::Arg<::core::option::Option<String>>) -> Self {
            self.cursor = cursor.into_input();
            self
        }
    }

    impl files::Search {
        fn __build_draft(&self) -> files::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::POST);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://files.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("search")]);
            // Headers follow paths; entries within this kind run in tree order.
            // Both entries are the client's: `client`, then its input.
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__client.key,
                };
                let key = client.key;
                __draft.header(
                    "authorization",
                    &[
                        ::concord::__private::Piece::Lit("Bearer "),
                        ::concord::__private::Piece::Val(
                            "client.key",
                            &(&::concord::__private::ValueRef(client.key)).adapt(),
                        ),
                    ],
                );
                let __entry = "tour".to_owned();
                __draft.header(
                    "x-client",
                    &[::concord::__private::Piece::Val(
                        "x-client",
                        &(&::concord::__private::ValueRef(&__entry)).adapt(),
                    )],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__client.key,
                };
                let text = &self.text;
                let cursor = &self.cursor;
                // Every entry uses the same adapter and insertion path, in the order written.
                let __value0 = ::concord::__private::ValueRef(text);
                let __value1 = ::concord::__private::ValueRef(cursor);
                let mut __entries = ::concord::body::Entries::new();
                __entries.__push("text", (&__value0).adapt());
                __entries.__push("cursor", (&__value1).adapt());
                __draft.body(json(__entries));
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> files::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, "files.search")
            } else {
                __draft.finish(&self.__client.__core, &self.__options, "files.search")
            }
        }

        const __ENDPOINT: &'static str = "files.search";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> files::Result<::concord::Answer<Page>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client { key: &self.__client.key };
                            let key = client.key;
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

        fn __prepare_inner(self) -> files::Result<::concord::__private::Prepared<Page, ()>> {
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
        files::Search,
        Page,
        (),
        files::Result,
        "[`files::Result`]<[`Page`]>",
        awaitable,
        unconditional
    );

    impl ::std::fmt::Debug for files::Search {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Search")
                .field("endpoint", &"files.search")
                .finish_non_exhaustive()
        }
    }
};
