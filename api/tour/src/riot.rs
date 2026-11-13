const _: () = {
    use ::concord::__private::AdaptValue as _;

    // Owner views expose input references, never the owning handle's fields.
    struct __Client<'a> {
        key: &'a Secret,
    }
    // A view has a field for each input of its level, also when no expression reads it.
    #[allow(dead_code)]
    struct __Matches<'a> {
        route: &'a MatchRoute,
    }
    struct __Account<'a> {
        route: &'a AccountRoute,
    }
    use concord::{by_offset_limit, Json};

    // /// The Riot Games API (League of Legends subset).
    // ///
    // /// # Example
    // ///
    // /// ```no_run
    // /// use tour::{Europe, Puuid, Riot};
    // ///
    // /// # async fn example() -> tour::riot::Result<()> {
    // /// let riot = Riot::new("test-key");
    // /// let puuid = Puuid::from("p-1");
    // /// match riot.matches(Europe).ids(&puuid).count(5).send().await {
    // ///     Ok(ids) => println!("{} matches", ids.len()),
    // ///     Err(error) if error.is_status(404) => println!("No match history"),
    // ///     Err(error) => {
    // ///         if let Some(problem) = error.problem() {
    // ///             eprintln!("{}", problem.status.message);
    // ///         }
    // ///         return Err(error);
    // ///     }
    // /// }
    // /// # Ok(())
    // /// # }
    // /// ```
    // pub client Riot(
    //     /// The API key. Sent on every request unless an endpoint removes it.
    //     key: Secret,
    // ) {
    //     headers { "x-riot-token" = key; }
    //     read Json;
    //     problem RiotProblem;
    //     /// Accounts (regional routing).
    //     scope account(
    //         /// The regional routing group for account requests.
    //         route: AccountRoute,
    //     ) {
    //         base "https://{route}.api.riotgames.com";
    //         path "riot/account/v1/accounts";
    //         /// The account behind a Riot ID (`name#tag`). `None` when there is no such player.
    //         GET by_riot_id(
    //             /// The name before the # in the Riot ID.
    //             game_name: String,
    //             /// The tag after the # in the Riot ID.
    //             tag_line: String,
    //         ) -> Option<Account> {
    //             path "by-riot-id/{game_name}/{tag_line}";
    //             status { 404 => None }
    //         }
    //         /// The account of a signed-in player.
    //         /// Uses the player's own token instead of the API key.
    //         GET me(
    //             /// The player's bearer token.
    //             access_token: Secret,
    //         ) -> Account {
    //             path "me";
    //             headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }
    //         }
    //     }
    //     /// Matches (regional routing).
    //     scope matches(
    //         /// The regional routing group for match requests.
    //         route: MatchRoute,
    //     ) {
    //         base "https://{route}.api.riotgames.com";
    //         path "lol/match/v5/matches";
    //         /// Match ids of a player, newest first.
    //         GET ids(
    //             /// The player's persistent identifier.
    //             puuid: Puuid,
    //             /// Index of the first match id to return.
    //             start?: u32,
    //             /// Maximum number of match ids in one page.
    //             count?: u32,
    //             /// Only the matches of this queue.
    //             queue?: i32,
    //             /// Only the matches that started at or after this time, in seconds since the epoch.
    //             start_time?: i64,
    //         ) -> Vec<MatchId> {
    //             path "by-puuid/{puuid}/ids";
    //             query { start; count; queue; "startTime" = start_time; }
    //             paginate by_offset_limit(start, count);
    //         }
    //         /// One match.
    //         GET get(
    //             /// The regional identifier of the match to fetch.
    //             id: MatchId,
    //         ) -> Match {
    //             path "{id}";
    //         }
    //     }
    // }
    impl riot::__Types for () {
        type __Input0 = Secret;
        type __Input1 = AccountRoute;
        type __Input2 = String;
        type __Input3 = String;
        type __Input4 = Secret;
        type __Input5 = MatchRoute;
        type __Input6 = Puuid;
        type __Input7 = ::core::option::Option<u32>;
        type __Input8 = ::core::option::Option<u32>;
        type __Input9 = ::core::option::Option<i32>;
        type __Input10 = ::core::option::Option<i64>;
        type __Input11 = MatchId;
        type __Problem = RiotProblem;
    }

    impl ::concord::page::Slot<riot::matches::Ids> for riot::__IdsPuuid {
        type Value = Puuid;
        fn get<'a>(&self, __call: &'a riot::matches::Ids) -> ::core::option::Option<&'a Self::Value> {
            ::core::option::Option::Some(&__call.puuid)
        }
        fn set(&self, mut __call: riot::matches::Ids, __value: Self::Value) -> riot::matches::Ids {
            __call.puuid = __value;
            __call
        }
    }

    impl ::concord::page::Slot<riot::matches::Ids> for riot::__IdsStart {
        type Value = u32;
        fn get<'a>(&self, __call: &'a riot::matches::Ids) -> ::core::option::Option<&'a Self::Value> {
            __call.start.as_ref()
        }
        fn set(&self, mut __call: riot::matches::Ids, __value: Self::Value) -> riot::matches::Ids {
            __call.start = ::core::option::Option::Some(__value);
            __call
        }
    }

    impl ::concord::page::Slot<riot::matches::Ids> for riot::__IdsCount {
        type Value = u32;
        fn get<'a>(&self, __call: &'a riot::matches::Ids) -> ::core::option::Option<&'a Self::Value> {
            __call.count.as_ref()
        }
        fn set(&self, mut __call: riot::matches::Ids, __value: Self::Value) -> riot::matches::Ids {
            __call.count = ::core::option::Option::Some(__value);
            __call
        }
    }

    impl ::concord::page::Slot<riot::matches::Ids> for riot::__IdsQueue {
        type Value = i32;
        fn get<'a>(&self, __call: &'a riot::matches::Ids) -> ::core::option::Option<&'a Self::Value> {
            __call.queue.as_ref()
        }
        fn set(&self, mut __call: riot::matches::Ids, __value: Self::Value) -> riot::matches::Ids {
            __call.queue = ::core::option::Option::Some(__value);
            __call
        }
    }

    impl ::concord::page::Slot<riot::matches::Ids> for riot::__IdsStartTime {
        type Value = i64;
        fn get<'a>(&self, __call: &'a riot::matches::Ids) -> ::core::option::Option<&'a Self::Value> {
            __call.start_time.as_ref()
        }
        fn set(&self, mut __call: riot::matches::Ids, __value: Self::Value) -> riot::matches::Ids {
            __call.start_time = ::core::option::Option::Some(__value);
            __call
        }
    }

    impl Riot {
        /// Create a client with the default transport and settings.
        ///
        /// `key`: [`Secret`], &[`Secret`], [`String`], or &[`str`]. The API key. Sent on every request unless an endpoint removes it.
        pub fn new(key: impl ::concord::arg::Arg<Secret>) -> Self {
            Self {
                __core: ::concord::__private::Core::default(),
                key: key.into_input(),
            }
        }

        /// The retry policy for calls made by this client. Accepts a [`concord::Retry`]. Retries are off by default.
        pub fn with_retry(mut self, __retry: ::concord::Retry<RiotProblem>) -> Self {
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

        /// Matches (regional routing).
        ///
        /// `route`: [`MatchRoute`] or &[`MatchRoute`]. The regional routing group for match requests.
        ///
        /// The handle owns its inputs. Nothing is sent yet.
        pub fn matches(&self, route: impl ::concord::arg::Arg<MatchRoute>) -> riot::Matches {
            riot::Matches {
                __client: self.clone(),
                route: route.into_input(),
            }
        }

        /// Accounts (regional routing).
        ///
        /// `route`: [`AccountRoute`] or &[`AccountRoute`]. The regional routing group for account requests.
        ///
        /// The handle owns its inputs. Nothing is sent yet.
        pub fn account(&self, route: impl ::concord::arg::Arg<AccountRoute>) -> riot::Account {
            riot::Account {
                __client: self.clone(),
                route: route.into_input(),
            }
        }
    }

    impl ::std::fmt::Debug for Riot {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Riot").finish_non_exhaustive()
        }
    }

    // /// Matches (regional routing).
    // scope matches(
    //     /// The regional routing group for match requests.
    //     route: MatchRoute,
    // ) {
    //     base "https://{route}.api.riotgames.com";
    //     path "lol/match/v5/matches";
    //     /// Match ids of a player, newest first.
    //     GET ids(
    //         /// The player's persistent identifier.
    //         puuid: Puuid,
    //         /// Index of the first match id to return.
    //         start?: u32,
    //         /// Maximum number of match ids in one page.
    //         count?: u32,
    //         /// Only the matches of this queue.
    //         queue?: i32,
    //         /// Only the matches that started at or after this time, in seconds since the epoch.
    //         start_time?: i64,
    //     ) -> Vec<MatchId> {
    //         path "by-puuid/{puuid}/ids";
    //         query { start; count; queue; "startTime" = start_time; }
    //         paginate by_offset_limit(start, count);
    //     }
    //     /// One match.
    //     GET get(
    //         /// The regional identifier of the match to fetch.
    //         id: MatchId,
    //     ) -> Match {
    //         path "{id}";
    //     }
    // }
    impl riot::Matches {
        /// Match ids of a player, newest first.
        ///
        /// Call: `riot.matches(route).ids(puuid).send().await?`.
        ///
        /// Request: `GET https://{route}.api.riotgames.com/lol/match/v5/matches/by-puuid/{puuid}/ids`.
        ///
        /// Headers: `x-riot-token`.
        ///
        /// Inputs:
        ///
        /// - `puuid`: [`Puuid`] or &[`Puuid`]. The player's persistent identifier.
        ///
        /// Options:
        ///
        /// - `.start(5)`: accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it. Index of the first match id to return.
        /// - `.count(5)`: accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it. Maximum number of match ids in one page.
        /// - `.queue(5)`: accepts [`i32`], &[`i32`], or an [`Option`]<[`i32`]>. `None` clears it. Only the matches of this queue.
        /// - `.start_time(5)`: accepts [`i64`], &[`i64`], or an [`Option`]<[`i64`]>. `None` clears it. Only the matches that started at or after this time, in seconds since the epoch.
        ///
        /// `.send().await` gives [`riot::Result`]<[`Vec`]<[`MatchId`]>>.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        /// Paging ends on an empty page or a page shorter than the supplied `count`; `count` is the maximum for each page.
        pub fn ids(&self, puuid: impl ::concord::arg::Arg<Puuid>) -> riot::matches::Ids {
            riot::matches::Ids {
                __scope: self.clone(),
                puuid: puuid.into_input(),
                start: ::core::option::Option::None,
                count: ::core::option::Option::None,
                queue: ::core::option::Option::None,
                start_time: ::core::option::Option::None,
                __options: ::concord::__private::Options::default(),
            }
        }

        /// One match.
        ///
        /// Call: `riot.matches(route).get(id).await?`.
        ///
        /// Request: `GET https://{route}.api.riotgames.com/lol/match/v5/matches/{id}`.
        ///
        /// Headers: `x-riot-token`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`MatchId`] or &[`MatchId`]. The regional identifier of the match to fetch.
        ///
        /// `.await` gives [`riot::Result`]<[`Match`]>.
        pub fn get(&self, id: impl ::concord::arg::Arg<MatchId>) -> riot::matches::Get {
            riot::matches::Get {
                __scope: self.clone(),
                id: id.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }
    }

    impl ::std::fmt::Debug for riot::Matches {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("riot::Matches").finish_non_exhaustive()
        }
    }

    // /// Accounts (regional routing).
    // scope account(
    //     /// The regional routing group for account requests.
    //     route: AccountRoute,
    // ) {
    //     base "https://{route}.api.riotgames.com";
    //     path "riot/account/v1/accounts";
    //     /// The account behind a Riot ID (`name#tag`). `None` when there is no such player.
    //     GET by_riot_id(
    //         /// The name before the # in the Riot ID.
    //         game_name: String,
    //         /// The tag after the # in the Riot ID.
    //         tag_line: String,
    //     ) -> Option<Account> {
    //         path "by-riot-id/{game_name}/{tag_line}";
    //         status { 404 => None }
    //     }
    //     /// The account of a signed-in player.
    //     /// Uses the player's own token instead of the API key.
    //     GET me(
    //         /// The player's bearer token.
    //         access_token: Secret,
    //     ) -> Account {
    //         path "me";
    //         headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }
    //     }
    // }
    impl riot::Account {
        /// The account behind a Riot ID (`name#tag`). `None` when there is no such player.
        ///
        /// Call: `riot.account(route).by_riot_id(game_name, tag_line).await?`.
        ///
        /// Request: `GET https://{route}.api.riotgames.com/riot/account/v1/accounts/by-riot-id/{game_name}/{tag_line}`.
        ///
        /// Headers: `x-riot-token`.
        ///
        /// Inputs:
        ///
        /// - `game_name`: [`String`], &[`String`], or &[`str`]. The name before the # in the Riot ID.
        /// - `tag_line`: [`String`], &[`String`], or &[`str`]. The tag after the # in the Riot ID.
        ///
        /// Status: `404`.
        ///
        /// Status: `2xx`.
        ///
        /// `.await` gives [`riot::Result`]<[`Option`]<[`tour::Account`](crate::Account)>>.
        pub fn by_riot_id(
            &self,
            game_name: impl ::concord::arg::Arg<String>,
            tag_line: impl ::concord::arg::Arg<String>,
        ) -> riot::account::ByRiotId {
            riot::account::ByRiotId {
                __scope: self.clone(),
                game_name: game_name.into_input(),
                tag_line: tag_line.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }

        /// The account of a signed-in player.
        /// Uses the player's own token instead of the API key.
        ///
        /// Call: `riot.account(route).me(access_token).await?`.
        ///
        /// Request: `GET https://{route}.api.riotgames.com/riot/account/v1/accounts/me`.
        ///
        /// Headers: `authorization`.
        ///
        /// Inputs:
        ///
        /// - `access_token`: [`Secret`], &[`Secret`], [`String`], or &[`str`]. The player's bearer token.
        ///
        /// `.await` gives [`riot::Result`]<[`tour::Account`](crate::Account)>.
        pub fn me(&self, access_token: impl ::concord::arg::Arg<Secret>) -> riot::account::Me {
            riot::account::Me {
                __scope: self.clone(),
                access_token: access_token.into_input(),
                __options: ::concord::__private::Options::default(),
            }
        }
    }

    impl ::std::fmt::Debug for riot::Account {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("riot::Account").finish_non_exhaustive()
        }
    }

    // /// Match ids of a player, newest first.
    // GET ids(
    //     /// The player's persistent identifier.
    //     puuid: Puuid,
    //     /// Index of the first match id to return.
    //     start?: u32,
    //     /// Maximum number of match ids in one page.
    //     count?: u32,
    //     /// Only the matches of this queue.
    //     queue?: i32,
    //     /// Only the matches that started at or after this time, in seconds since the epoch.
    //     start_time?: i64,
    // ) -> Vec<MatchId> {
    //     path "by-puuid/{puuid}/ids";
    //     query { start; count; queue; "startTime" = start_time; }
    //     paginate by_offset_limit(start, count);
    // }
    /// The options of this endpoint.
    impl riot::matches::Ids {
        /// Index of the first match id to return.
        ///
        /// Accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it.
        pub fn start(mut self, start: impl ::concord::arg::Arg<::core::option::Option<u32>>) -> Self {
            self.start = start.into_input();
            self
        }

        /// Maximum number of match ids in one page.
        ///
        /// Accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it.
        pub fn count(mut self, count: impl ::concord::arg::Arg<::core::option::Option<u32>>) -> Self {
            self.count = count.into_input();
            self
        }

        /// Only the matches of this queue.
        ///
        /// Accepts [`i32`], &[`i32`], or an [`Option`]<[`i32`]>. `None` clears it.
        pub fn queue(mut self, queue: impl ::concord::arg::Arg<::core::option::Option<i32>>) -> Self {
            self.queue = queue.into_input();
            self
        }

        /// Only the matches that started at or after this time, in seconds since the epoch.
        ///
        /// Accepts [`i64`], &[`i64`], or an [`Option`]<[`i64`]>. `None` clears it.
        pub fn start_time(
            mut self,
            start_time: impl ::concord::arg::Arg<::core::option::Option<i64>>,
        ) -> Self {
            self.start_time = start_time.into_input();
            self
        }
    }

    impl riot::matches::Ids {
        fn __build_draft(&self) -> riot::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__scope.route,
                };
                let route = matches.route;
                __draft.base(&[
                    ::concord::__private::Piece::Lit("https://"),
                    ::concord::__private::Piece::Val(
                        "route",
                        &(&::concord::__private::ValueRef(route)).adapt(),
                    ),
                    ::concord::__private::Piece::Lit(".api.riotgames.com"),
                ]);
            }
            __draft.segment(&[::concord::__private::Piece::Lit("lol")]);
            __draft.segment(&[::concord::__private::Piece::Lit("match")]);
            __draft.segment(&[::concord::__private::Piece::Lit("v5")]);
            __draft.segment(&[::concord::__private::Piece::Lit("matches")]);
            __draft.segment(&[::concord::__private::Piece::Lit("by-puuid")]);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__scope.route,
                };
                let puuid = &self.puuid;
                let start = &self.start;
                let count = &self.count;
                let queue = &self.queue;
                let start_time = &self.start_time;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "puuid",
                    &(&::concord::__private::ValueRef(puuid)).adapt(),
                )]);
            }
            __draft.segment(&[::concord::__private::Piece::Lit("ids")]);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__scope.route,
                };
                let puuid = &self.puuid;
                let start = &self.start;
                let count = &self.count;
                let queue = &self.queue;
                let start_time = &self.start_time;
                __draft.query(
                    "start",
                    &[::concord::__private::Piece::Val(
                        "start",
                        &(&::concord::__private::ValueRef(start)).adapt(),
                    )],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__scope.route,
                };
                let puuid = &self.puuid;
                let start = &self.start;
                let count = &self.count;
                let queue = &self.queue;
                let start_time = &self.start_time;
                __draft.query(
                    "count",
                    &[::concord::__private::Piece::Val(
                        "count",
                        &(&::concord::__private::ValueRef(count)).adapt(),
                    )],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__scope.route,
                };
                let puuid = &self.puuid;
                let start = &self.start;
                let count = &self.count;
                let queue = &self.queue;
                let start_time = &self.start_time;
                __draft.query(
                    "queue",
                    &[::concord::__private::Piece::Val(
                        "queue",
                        &(&::concord::__private::ValueRef(queue)).adapt(),
                    )],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__scope.route,
                };
                let puuid = &self.puuid;
                let start = &self.start;
                let count = &self.count;
                let queue = &self.queue;
                let start_time = &self.start_time;
                __draft.query(
                    "startTime",
                    &[::concord::__private::Piece::Val(
                        "startTime",
                        &(&::concord::__private::ValueRef(start_time)).adapt(),
                    )],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                // Written by the client: only `client` is in reach.
                let key = client.key;
                __draft.header(
                    "x-riot-token",
                    &[::concord::__private::Piece::Val(
                        "x-riot-token",
                        &(&::concord::__private::ValueRef(key)).adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> riot::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(
                    &self.__scope.__client.__core,
                    &self.__options,
                    "riot.matches.ids",
                )
            } else {
                __draft.finish(
                    &self.__scope.__client.__core,
                    &self.__options,
                    "riot.matches.ids",
                )
            }
        }

        const __ENDPOINT: &'static str = "riot.matches.ids";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> riot::Result<::concord::Answer<Vec<MatchId>>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client { key: &self.__scope.__client.key };
                            let key = client.key;
                            (|| ::concord::__private::reader::<Vec<MatchId>, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client { key: &self.__scope.__client.key };
                            let key = client.key;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> riot::Result<::concord::__private::Prepared<Vec<MatchId>, RiotProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__scope.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__scope.__client.__core.clone(),
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
        riot::matches::Ids,
        Vec<MatchId>,
        RiotProblem,
        riot::Result,
        "[`riot::Result`]<[`Vec`]<[`MatchId`]>>",
        paged, conditional,
        pages {
            /// Iterate over [`Vec`]<[`MatchId`]> pages.
            ///
            /// `try_next().await` gives [`riot::Result`]<[`Option`]<[`Vec`]<[`MatchId`]>>>.
            /// Each page is one call. Nothing is asked until iteration starts.
            pub fn pages(self) -> ::concord::Pages<Vec<MatchId>, RiotProblem> {
                // The owners in reach, as views of references, then each input as its
                // place. `self` moves into the machine before the pager is written in
                // place (a closure there takes its type from `pages`), so the views borrow
                // a clone of the handle. The pager owns what it holds.
                let __owners = self.__scope.clone();
                ::concord::__private::pages(self, {
                    #[allow(unused_variables)]
                    let client = __Client {
                        key: &__owners.__client.key,
                    };
                    #[allow(unused_variables)]
                    let matches = __Matches {
                        route: &__owners.route,
                    };
                    #[allow(unused_variables)]
                    let (puuid, start, count, queue, start_time) = (
                        riot::__IdsPuuid,
                        riot::__IdsStart,
                        riot::__IdsCount,
                        riot::__IdsQueue,
                        riot::__IdsStartTime,
                    );
                    by_offset_limit(start, count)
                })
            }

        }
    );

    impl ::std::fmt::Debug for riot::matches::Ids {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Ids")
                .field("endpoint", &"riot.matches.ids")
                .finish_non_exhaustive()
        }
    }

    // /// One match.
    // GET get(
    //     /// The regional identifier of the match to fetch.
    //     id: MatchId,
    // ) -> Match {
    //     path "{id}";
    // }
    impl riot::matches::Get {
        fn __build_draft(&self) -> riot::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__scope.route,
                };
                let route = matches.route;
                __draft.base(&[
                    ::concord::__private::Piece::Lit("https://"),
                    ::concord::__private::Piece::Val(
                        "route",
                        &(&::concord::__private::ValueRef(route)).adapt(),
                    ),
                    ::concord::__private::Piece::Lit(".api.riotgames.com"),
                ]);
            }
            __draft.segment(&[::concord::__private::Piece::Lit("lol")]);
            __draft.segment(&[::concord::__private::Piece::Lit("match")]);
            __draft.segment(&[::concord::__private::Piece::Lit("v5")]);
            __draft.segment(&[::concord::__private::Piece::Lit("matches")]);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__scope.route,
                };
                let id = &self.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).adapt(),
                )]);
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                // Written by the client: only `client` is in reach.
                let key = client.key;
                __draft.header(
                    "x-riot-token",
                    &[::concord::__private::Piece::Val(
                        "x-riot-token",
                        &(&::concord::__private::ValueRef(key)).adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> riot::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(
                    &self.__scope.__client.__core,
                    &self.__options,
                    "riot.matches.get",
                )
            } else {
                __draft.finish(
                    &self.__scope.__client.__core,
                    &self.__options,
                    "riot.matches.get",
                )
            }
        }

        const __ENDPOINT: &'static str = "riot.matches.get";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> riot::Result<::concord::Answer<Match>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client { key: &self.__scope.__client.key };
                            let key = client.key;
                            (|| ::concord::__private::reader::<Match, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client { key: &self.__scope.__client.key };
                            let key = client.key;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> riot::Result<::concord::__private::Prepared<Match, RiotProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__scope.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__scope.__client.__core.clone(),
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
        riot::matches::Get,
        Match,
        RiotProblem,
        riot::Result,
        "[`riot::Result`]<[`Match`]>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for riot::matches::Get {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Get")
                .field("endpoint", &"riot.matches.get")
                .finish_non_exhaustive()
        }
    }

    // /// The account behind a Riot ID (`name#tag`). `None` when there is no such player.
    // GET by_riot_id(
    //     /// The name before the # in the Riot ID.
    //     game_name: String,
    //     /// The tag after the # in the Riot ID.
    //     tag_line: String,
    // ) -> Option<Account> {
    //     path "by-riot-id/{game_name}/{tag_line}";
    //     status { 404 => None }
    // }
    impl riot::account::ByRiotId {
        fn __build_draft(&self) -> riot::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                let account = __Account {
                    route: &self.__scope.route,
                };
                let route = account.route;
                __draft.base(&[
                    ::concord::__private::Piece::Lit("https://"),
                    ::concord::__private::Piece::Val(
                        "route",
                        &(&::concord::__private::ValueRef(route)).adapt(),
                    ),
                    ::concord::__private::Piece::Lit(".api.riotgames.com"),
                ]);
            }
            __draft.segment(&[::concord::__private::Piece::Lit("riot")]);
            __draft.segment(&[::concord::__private::Piece::Lit("account")]);
            __draft.segment(&[::concord::__private::Piece::Lit("v1")]);
            __draft.segment(&[::concord::__private::Piece::Lit("accounts")]);
            __draft.segment(&[::concord::__private::Piece::Lit("by-riot-id")]);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                let account = __Account {
                    route: &self.__scope.route,
                };
                let game_name = &self.game_name;
                let tag_line = &self.tag_line;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "game_name",
                    &(&::concord::__private::ValueRef(game_name)).adapt(),
                )]);
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                let account = __Account {
                    route: &self.__scope.route,
                };
                let game_name = &self.game_name;
                let tag_line = &self.tag_line;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "tag_line",
                    &(&::concord::__private::ValueRef(tag_line)).adapt(),
                )]);
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                // Written by the client: only `client` is in reach.
                let key = client.key;
                __draft.header(
                    "x-riot-token",
                    &[::concord::__private::Piece::Val(
                        "x-riot-token",
                        &(&::concord::__private::ValueRef(key)).adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> riot::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(
                    &self.__scope.__client.__core,
                    &self.__options,
                    "riot.account.by_riot_id",
                )
            } else {
                __draft.finish(
                    &self.__scope.__client.__core,
                    &self.__options,
                    "riot.account.by_riot_id",
                )
            }
        }

        const __ENDPOINT: &'static str = "riot.account.by_riot_id";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> riot::Result<::concord::Answer<Option<Account>>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                ::concord::http::StatusCode::NOT_FOUND => {
                    // The arm of the endpoint: the owners in reach, then its inputs.
                    let client = __Client { key: &self.__scope.__client.key };
                    let account = __Account { route: &self.__scope.route };
                    let game_name = &self.game_name;
                    let tag_line = &self.tag_line;
                    #[allow(unused_braces)]
                    let __arm = ||
                     -> ::std::result::Result<
                        Option<Account>,
                        ::concord::error::Fail<RiotProblem>,
                    > {
                                    ::core::result::Result::Ok({ None })
                    };
                    ::concord::__private::arm(__arm(), __status, &__headers, &__info)?
                }
                __status if __status.is_success() => ::core::option::Option::Some(
                    ::concord::__private::read_body(
                        &{
                            let client = __Client { key: &self.__scope.__client.key };
                            let key = client.key;
                            (|| ::concord::__private::reader::<Account, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?,
                ),
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client { key: &self.__scope.__client.key };
                            let key = client.key;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> riot::Result<::concord::__private::Prepared<Option<Account>, RiotProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__scope.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__scope.__client.__core.clone(),
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
        riot::account::ByRiotId,
        Option<Account>,
        RiotProblem,
        riot::Result,
        "[`riot::Result`]<[`Option`]<[`tour::Account`](crate::Account)>>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for riot::account::ByRiotId {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("ByRiotId")
                .field("endpoint", &"riot.account.by_riot_id")
                .finish_non_exhaustive()
        }
    }

    // /// The account of a signed-in player.
    // /// Uses the player's own token instead of the API key.
    // GET me(
    //     /// The player's bearer token.
    //     access_token: Secret,
    // ) -> Account {
    //     path "me";
    //     headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }
    // }
    impl riot::account::Me {
        fn __build_draft(&self) -> riot::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                let account = __Account {
                    route: &self.__scope.route,
                };
                let route = account.route;
                __draft.base(&[
                    ::concord::__private::Piece::Lit("https://"),
                    ::concord::__private::Piece::Val(
                        "route",
                        &(&::concord::__private::ValueRef(route)).adapt(),
                    ),
                    ::concord::__private::Piece::Lit(".api.riotgames.com"),
                ]);
            }
            __draft.segment(&[::concord::__private::Piece::Lit("riot")]);
            __draft.segment(&[::concord::__private::Piece::Lit("account")]);
            __draft.segment(&[::concord::__private::Piece::Lit("v1")]);
            __draft.segment(&[::concord::__private::Piece::Lit("accounts")]);
            __draft.segment(&[::concord::__private::Piece::Lit("me")]);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__scope.__client.key,
                };
                let account = __Account {
                    route: &self.__scope.route,
                };
                let access_token = &self.access_token;
                __draft.header(
                    "authorization",
                    &[
                        ::concord::__private::Piece::Lit("Bearer "),
                        ::concord::__private::Piece::Val(
                            "access_token",
                            &(&::concord::__private::ValueRef(access_token)).adapt(),
                        ),
                    ],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> riot::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(
                    &self.__scope.__client.__core,
                    &self.__options,
                    "riot.account.me",
                )
            } else {
                __draft.finish(
                    &self.__scope.__client.__core,
                    &self.__options,
                    "riot.account.me",
                )
            }
        }

        const __ENDPOINT: &'static str = "riot.account.me";

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> riot::Result<::concord::Answer<Account>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client { key: &self.__scope.__client.key };
                            let key = client.key;
                            (|| ::concord::__private::reader::<Account, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await?
                }
                _ => {
                    return ::core::result::Result::Err(::concord::__private::refused(
                        &{
                            let client = __Client { key: &self.__scope.__client.key };
                            let key = client.key;
                            (|| ::concord::__private::reader::<_, _>(Json))()
                        },
                        __response,
                        &__info,
                    )
                    .await)
                }
            };
            ::core::result::Result::Ok(::concord::Answer::new(__status, __headers, __value)
                .with_response_url(__response_url))
        }

        fn __prepare_inner(
            self,
        ) -> riot::Result<::concord::__private::Prepared<Account, RiotProblem>> {
            let __run = ::concord::__private::Run::start(
                &self.__scope.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__scope.__client.__core.clone(),
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
        riot::account::Me,
        Account,
        RiotProblem,
        riot::Result,
        "[`riot::Result`]<[`tour::Account`](crate::Account)>",
        awaitable,
        conditional
    );

    impl ::std::fmt::Debug for riot::account::Me {
        fn fmt(&self, __f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            __f.debug_struct("Me")
                .field("endpoint", &"riot.account.me")
                .finish_non_exhaustive()
        }
    }
};
