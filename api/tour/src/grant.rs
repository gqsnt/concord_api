const _: () = {
    use concord::form;

    // Added to the Files description:
    // POST grant(grant_type: String, client_id: String, client_secret: Secret) {
    //     path "oauth/token";
    //     body form { grant_type; client_id; client_secret; }
    // }
    // Only a nameless import: a name that the expansion brought in here would be seen
    // by the author's expressions, and could clash with an import of the author's.
    use ::concord::__private::AdaptValue as _;

    // The owner view of the Files client, as in files.rs: this endpoint belongs to it.
    #[allow(dead_code)]
    struct __Client<'a> {
        key: &'a Secret,
    }

    impl Files {
        /// Exchange credentials with `body form { grant_type; client_id; client_secret; }`.
        /// Text arguments accept owned or borrowed strings; the secret also accepts `Secret`.
        pub fn grant(
            &self,
            grant_type: impl ::concord::arg::Arg<String>,
            client_id: impl ::concord::arg::Arg<String>,
            client_secret: impl ::concord::arg::Arg<Secret>,
        ) -> files::Grant {
            files::Grant {
                __client: self.clone(),
                grant_type: grant_type.into_input(),
                client_id: client_id.into_input(),
                client_secret: client_secret.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }
    }

    impl files::Grant {
        const __ENDPOINT: &'static str = "files.grant";

        fn __body(&self) -> ::core::result::Result<::concord::Body, Box<dyn std::error::Error + Send + Sync>> {
            let grant_type = &self.grant_type;
            let client_id = &self.client_id;
            let client_secret = &self.client_secret;
            let __value0 = ::concord::__private::ValueRef(grant_type);
            let __value1 = ::concord::__private::ValueRef(client_id);
            let __value2 = ::concord::__private::ValueRef(client_secret);
            let mut __entries = ::concord::body::Entries::new();
            __entries.__push("grant_type", (&__value0).adapt());
            __entries.__push("client_id", (&__value1).adapt());
            __entries.__push("client_secret", (&__value2).adapt());
            form(__entries).map_err(Into::into)
        }

        fn __build_draft(&self) -> files::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::POST);
            __draft.base(&[::concord::__private::Piece::Lit(
                "https://files.example.com",
            )]);
            __draft.segment(&[::concord::__private::Piece::Lit("oauth")]);
            __draft.segment(&[::concord::__private::Piece::Lit("token")]);
            // Both entries are the client's: `client`, then its input, before its expression.
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
            __draft.body(self.__body());
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> files::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            } else {
                __draft.finish(&self.__client.__core, &self.__options, Self::__ENDPOINT)
            }
        }

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> files::Result<::concord::Answer<()>> {
            let __url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            if !__status.is_success() {
                return ::core::result::Result::Err(::concord::__private::refused_plain(__response, &__info).await);
            }
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, ()).with_response_url(__url))
        }

        fn __prepare_inner(self) -> files::Result<::concord::__private::Prepared<(), ()>> {
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
        files::Grant,
        (),
        (),
        files::Result,
        "[`files::Result`]<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>",
        awaitable,
        unconditional
    );
    impl ::std::fmt::Debug for files::Grant {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Grant")
                .field("endpoint", &Self::__ENDPOINT)
                .finish_non_exhaustive()
        }
    }
};
