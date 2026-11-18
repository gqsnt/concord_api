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

    impl ::concord::page::Slot<riot::Call<riot::matches::Ids>> for riot::__IdsPuuid {
        type Value = Puuid;
        fn get<'a>(
            &self,
            __call: &'a riot::Call<riot::matches::Ids>,
        ) -> ::core::option::Option<&'a Self::Value> {
            ::core::option::Option::Some(&__call.__inputs.puuid)
        }
        fn set(
            &self,
            mut __call: riot::Call<riot::matches::Ids>,
            __value: Self::Value,
        ) -> riot::Call<riot::matches::Ids> {
            __call.__inputs.puuid = __value;
            __call
        }
    }

    impl ::concord::page::Slot<riot::Call<riot::matches::Ids>> for riot::__IdsStart {
        type Value = u32;
        fn get<'a>(
            &self,
            __call: &'a riot::Call<riot::matches::Ids>,
        ) -> ::core::option::Option<&'a Self::Value> {
            __call.__inputs.start.as_ref()
        }
        fn set(
            &self,
            mut __call: riot::Call<riot::matches::Ids>,
            __value: Self::Value,
        ) -> riot::Call<riot::matches::Ids> {
            __call.__inputs.start = ::core::option::Option::Some(__value);
            __call
        }
    }

    impl ::concord::page::Slot<riot::Call<riot::matches::Ids>> for riot::__IdsCount {
        type Value = u32;
        fn get<'a>(
            &self,
            __call: &'a riot::Call<riot::matches::Ids>,
        ) -> ::core::option::Option<&'a Self::Value> {
            __call.__inputs.count.as_ref()
        }
        fn set(
            &self,
            mut __call: riot::Call<riot::matches::Ids>,
            __value: Self::Value,
        ) -> riot::Call<riot::matches::Ids> {
            __call.__inputs.count = ::core::option::Option::Some(__value);
            __call
        }
    }

    impl ::concord::page::Slot<riot::Call<riot::matches::Ids>> for riot::__IdsQueue {
        type Value = i32;
        fn get<'a>(
            &self,
            __call: &'a riot::Call<riot::matches::Ids>,
        ) -> ::core::option::Option<&'a Self::Value> {
            __call.__inputs.queue.as_ref()
        }
        fn set(
            &self,
            mut __call: riot::Call<riot::matches::Ids>,
            __value: Self::Value,
        ) -> riot::Call<riot::matches::Ids> {
            __call.__inputs.queue = ::core::option::Option::Some(__value);
            __call
        }
    }

    impl ::concord::page::Slot<riot::Call<riot::matches::Ids>> for riot::__IdsStartTime {
        type Value = i64;
        fn get<'a>(
            &self,
            __call: &'a riot::Call<riot::matches::Ids>,
        ) -> ::core::option::Option<&'a Self::Value> {
            __call.__inputs.start_time.as_ref()
        }
        fn set(
            &self,
            mut __call: riot::Call<riot::matches::Ids>,
            __value: Self::Value,
        ) -> riot::Call<riot::matches::Ids> {
            __call.__inputs.start_time = ::core::option::Option::Some(__value);
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
        /// `.send().await` returns [`riot::Result`]<[`Vec`]<[`MatchId`]>>.
        ///
        /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
        /// Paging ends on an empty page or a page shorter than the supplied `count`; `count` is the maximum for each page.
        #[must_use = "await `.send()` for one page, or use `.pages()` for all pages"]
        pub fn ids(&self, puuid: impl ::concord::arg::Arg<Puuid>) -> riot::Call<riot::matches::Ids> {
            riot::Call::__new(riot::__MatchesIdsInputs {
                __scope: self.clone(),
                puuid: puuid.into_input(),
                start: ::core::option::Option::None,
                count: ::core::option::Option::None,
                queue: ::core::option::Option::None,
                start_time: ::core::option::Option::None,
            })
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
        /// `.await` returns [`riot::Result`]<[`Match`]>.
        #[must_use = "a call does nothing until sent or awaited"]
        pub fn get(&self, id: impl ::concord::arg::Arg<MatchId>) -> riot::Call<riot::matches::Get> {
            riot::Call::__new(riot::__MatchesGetInputs {
                __scope: self.clone(),
                id: id.into_input(),
            })
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
        /// `.await` returns [`riot::Result`]<[`Option`]<[`tour::Account`](crate::Account)>>.
        #[must_use = "a call does nothing until sent or awaited"]
        pub fn by_riot_id(
            &self,
            game_name: impl ::concord::arg::Arg<String>,
            tag_line: impl ::concord::arg::Arg<String>,
        ) -> riot::Call<riot::account::ByRiotId> {
            riot::Call::__new(riot::__AccountByRiotIdInputs {
                __scope: self.clone(),
                game_name: game_name.into_input(),
                tag_line: tag_line.into_input(),
            })
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
        /// `.await` returns [`riot::Result`]<[`tour::Account`](crate::Account)>.
        #[must_use = "a call does nothing until sent or awaited"]
        pub fn me(&self, access_token: impl ::concord::arg::Arg<Secret>) -> riot::Call<riot::account::Me> {
            riot::Call::__new(riot::__AccountMeInputs {
                __scope: self.clone(),
                access_token: access_token.into_input(),
            })
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
    #[doc(hidden)]
    impl riot::Call<riot::matches::Ids> {
        /// Index of the first match id to return.
        ///
        /// Accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it.
        pub fn start(mut self, start: impl ::concord::arg::Arg<::core::option::Option<u32>>) -> Self {
            self.__inputs.start = start.into_input();
            self
        }

        /// Maximum number of match ids in one page.
        ///
        /// Accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it.
        pub fn count(mut self, count: impl ::concord::arg::Arg<::core::option::Option<u32>>) -> Self {
            self.__inputs.count = count.into_input();
            self
        }

        /// Only the matches of this queue.
        ///
        /// Accepts [`i32`], &[`i32`], or an [`Option`]<[`i32`]>. `None` clears it.
        pub fn queue(mut self, queue: impl ::concord::arg::Arg<::core::option::Option<i32>>) -> Self {
            self.__inputs.queue = queue.into_input();
            self
        }

        /// Only the matches that started at or after this time, in seconds since the epoch.
        ///
        /// Accepts [`i64`], &[`i64`], or an [`Option`]<[`i64`]>. `None` clears it.
        pub fn start_time(
            mut self,
            start_time: impl ::concord::arg::Arg<::core::option::Option<i64>>,
        ) -> Self {
            self.__inputs.start_time = start_time.into_input();
            self
        }
    }

    impl riot::matches::Ids {
        /// Option `start` of `matches::ids`.
        ///
        /// The first argument is the call.
        ///
        /// Index of the first match id to return.
        ///
        /// Accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it.
        pub fn start(
            __call: riot::Call<riot::matches::Ids>,
            start: impl ::concord::arg::Arg<::core::option::Option<u32>>,
        ) -> riot::Call<riot::matches::Ids> {
            __call.start(start)
        }
        /// Option `count` of `matches::ids`.
        ///
        /// The first argument is the call.
        ///
        /// Maximum number of match ids in one page.
        ///
        /// Accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it.
        pub fn count(
            __call: riot::Call<riot::matches::Ids>,
            count: impl ::concord::arg::Arg<::core::option::Option<u32>>,
        ) -> riot::Call<riot::matches::Ids> {
            __call.count(count)
        }
        /// Option `queue` of `matches::ids`.
        ///
        /// The first argument is the call.
        ///
        /// Only the matches of this queue.
        ///
        /// Accepts [`i32`], &[`i32`], or an [`Option`]<[`i32`]>. `None` clears it.
        pub fn queue(
            __call: riot::Call<riot::matches::Ids>,
            queue: impl ::concord::arg::Arg<::core::option::Option<i32>>,
        ) -> riot::Call<riot::matches::Ids> {
            __call.queue(queue)
        }
        /// Option `start_time` of `matches::ids`.
        ///
        /// The first argument is the call.
        ///
        /// Only the matches that started at or after this time, in seconds since the epoch.
        ///
        /// Accepts [`i64`], &[`i64`], or an [`Option`]<[`i64`]>. `None` clears it.
        pub fn start_time(
            __call: riot::Call<riot::matches::Ids>,
            start_time: impl ::concord::arg::Arg<::core::option::Option<i64>>,
        ) -> riot::Call<riot::matches::Ids> {
            __call.start_time(start_time)
        }
    }

    impl riot::Call<riot::matches::Ids> {
        fn __build_draft(&self) -> riot::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__inputs.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__inputs.__scope.route,
                };
                let route = matches.route;
                __draft.base(&[
                    ::concord::__private::Piece::Lit("https://"),
                    ::concord::__private::Piece::Val(
                        "route",
                        &(&::concord::__private::ValueRef(route)).__adapt(),
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
                    key: &self.__inputs.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__inputs.__scope.route,
                };
                let puuid = &self.__inputs.puuid;
                let start = &self.__inputs.start;
                let count = &self.__inputs.count;
                let queue = &self.__inputs.queue;
                let start_time = &self.__inputs.start_time;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "puuid",
                    &(&::concord::__private::ValueRef(puuid)).__adapt(),
                )]);
            }
            __draft.segment(&[::concord::__private::Piece::Lit("ids")]);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__inputs.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__inputs.__scope.route,
                };
                let puuid = &self.__inputs.puuid;
                let start = &self.__inputs.start;
                let count = &self.__inputs.count;
                let queue = &self.__inputs.queue;
                let start_time = &self.__inputs.start_time;
                __draft.query(
                    "start",
                    &[::concord::__private::Piece::Val(
                        "start",
                        &(&::concord::__private::ValueRef(start)).__adapt(),
                    )],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__inputs.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__inputs.__scope.route,
                };
                let puuid = &self.__inputs.puuid;
                let start = &self.__inputs.start;
                let count = &self.__inputs.count;
                let queue = &self.__inputs.queue;
                let start_time = &self.__inputs.start_time;
                __draft.query(
                    "count",
                    &[::concord::__private::Piece::Val(
                        "count",
                        &(&::concord::__private::ValueRef(count)).__adapt(),
                    )],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__inputs.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__inputs.__scope.route,
                };
                let puuid = &self.__inputs.puuid;
                let start = &self.__inputs.start;
                let count = &self.__inputs.count;
                let queue = &self.__inputs.queue;
                let start_time = &self.__inputs.start_time;
                __draft.query(
                    "queue",
                    &[::concord::__private::Piece::Val(
                        "queue",
                        &(&::concord::__private::ValueRef(queue)).__adapt(),
                    )],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__inputs.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__inputs.__scope.route,
                };
                let puuid = &self.__inputs.puuid;
                let start = &self.__inputs.start;
                let count = &self.__inputs.count;
                let queue = &self.__inputs.queue;
                let start_time = &self.__inputs.start_time;
                __draft.query(
                    "startTime",
                    &[::concord::__private::Piece::Val(
                        "startTime",
                        &(&::concord::__private::ValueRef(start_time)).__adapt(),
                    )],
                );
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__inputs.__scope.__client.key,
                };
                // Written by the client: only `client` is in reach.
                let key = client.key;
                __draft.header(
                    "x-riot-token",
                    &[::concord::__private::Piece::Val(
                        "x-riot-token",
                        &(&::concord::__private::ValueRef(key)).__adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> riot::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(
                    &self.__inputs.__scope.__client.__core,
                    &self.__options,
                    "riot.matches.ids",
                )
            } else {
                __draft.finish(
                    &self.__inputs.__scope.__client.__core,
                    &self.__options,
                    "riot.matches.ids",
                )
            }
        }

        const __ENDPOINT: &'static str = <riot::matches::Ids as riot::Endpoint>::NAME;

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> riot::Result<::concord::Answer<Vec<MatchId>>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client { key: &self.__inputs.__scope.__client.key };
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
                            let client = __Client { key: &self.__inputs.__scope.__client.key };
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
                &self.__inputs.__scope.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__inputs.__scope.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    impl riot::Endpoint for riot::matches::Ids {
        type Value = Vec<MatchId>;
        const NAME: &'static str = "riot.matches.ids";
    }

    call_spec!(
        riot,
        riot::matches::Ids,
        riot::__MatchesIdsInputs,
        Vec<MatchId>,
        RiotProblem,
        "Ids"
    );
    impl riot::__spec::Paged for riot::matches::Ids {
        fn pages(call: riot::Call<riot::matches::Ids>) -> ::concord::Pages<Vec<MatchId>, RiotProblem> {
            // The owners in reach, as views of references, then each input as its
            // place. `call` moves into the machine before the pager is written in
            // place (a pager that is a call is built by `pages_of_pager`), so the
            // views borrow a clone of the handle. The pager owns what it holds.
            let __owners = call.__inputs.__scope.clone();
            ::concord::__private::pages_of_pager(call, || {
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
    impl riot::__spec::Conditional for riot::matches::Ids {}

    // /// One match.
    // GET get(
    //     /// The regional identifier of the match to fetch.
    //     id: MatchId,
    // ) -> Match {
    //     path "{id}";
    // }
    impl riot::Call<riot::matches::Get> {
        fn __build_draft(&self) -> riot::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__inputs.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__inputs.__scope.route,
                };
                let route = matches.route;
                __draft.base(&[
                    ::concord::__private::Piece::Lit("https://"),
                    ::concord::__private::Piece::Val(
                        "route",
                        &(&::concord::__private::ValueRef(route)).__adapt(),
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
                    key: &self.__inputs.__scope.__client.key,
                };
                let matches = __Matches {
                    route: &self.__inputs.__scope.route,
                };
                let id = &self.__inputs.id;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "id",
                    &(&::concord::__private::ValueRef(id)).__adapt(),
                )]);
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__inputs.__scope.__client.key,
                };
                // Written by the client: only `client` is in reach.
                let key = client.key;
                __draft.header(
                    "x-riot-token",
                    &[::concord::__private::Piece::Val(
                        "x-riot-token",
                        &(&::concord::__private::ValueRef(key)).__adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> riot::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(
                    &self.__inputs.__scope.__client.__core,
                    &self.__options,
                    "riot.matches.get",
                )
            } else {
                __draft.finish(
                    &self.__inputs.__scope.__client.__core,
                    &self.__options,
                    "riot.matches.get",
                )
            }
        }

        const __ENDPOINT: &'static str = <riot::matches::Get as riot::Endpoint>::NAME;

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> riot::Result<::concord::Answer<Match>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client { key: &self.__inputs.__scope.__client.key };
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
                            let client = __Client { key: &self.__inputs.__scope.__client.key };
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
                &self.__inputs.__scope.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__inputs.__scope.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    impl riot::Endpoint for riot::matches::Get {
        type Value = Match;
        const NAME: &'static str = "riot.matches.get";
    }

    call_spec!(
        riot,
        riot::matches::Get,
        riot::__MatchesGetInputs,
        Match,
        RiotProblem,
        "Get"
    );
    impl riot::__spec::Awaitable for riot::matches::Get {}
    impl riot::__spec::Conditional for riot::matches::Get {}

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
    impl riot::Call<riot::account::ByRiotId> {
        fn __build_draft(&self) -> riot::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__inputs.__scope.__client.key,
                };
                let account = __Account {
                    route: &self.__inputs.__scope.route,
                };
                let route = account.route;
                __draft.base(&[
                    ::concord::__private::Piece::Lit("https://"),
                    ::concord::__private::Piece::Val(
                        "route",
                        &(&::concord::__private::ValueRef(route)).__adapt(),
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
                    key: &self.__inputs.__scope.__client.key,
                };
                let account = __Account {
                    route: &self.__inputs.__scope.route,
                };
                let game_name = &self.__inputs.game_name;
                let tag_line = &self.__inputs.tag_line;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "game_name",
                    &(&::concord::__private::ValueRef(game_name)).__adapt(),
                )]);
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__inputs.__scope.__client.key,
                };
                let account = __Account {
                    route: &self.__inputs.__scope.route,
                };
                let game_name = &self.__inputs.game_name;
                let tag_line = &self.__inputs.tag_line;
                __draft.segment(&[::concord::__private::Piece::Val(
                    "tag_line",
                    &(&::concord::__private::ValueRef(tag_line)).__adapt(),
                )]);
            }
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__inputs.__scope.__client.key,
                };
                // Written by the client: only `client` is in reach.
                let key = client.key;
                __draft.header(
                    "x-riot-token",
                    &[::concord::__private::Piece::Val(
                        "x-riot-token",
                        &(&::concord::__private::ValueRef(key)).__adapt(),
                    )],
                );
            }
            ::core::result::Result::Ok(__draft)
        }

        fn __build_request(&self, __take: bool) -> riot::Result<::concord::reqwest::Request> {
            let __draft = self.__build_draft()?;
            if __take {
                __draft.finish_for_send(
                    &self.__inputs.__scope.__client.__core,
                    &self.__options,
                    "riot.account.by_riot_id",
                )
            } else {
                __draft.finish(
                    &self.__inputs.__scope.__client.__core,
                    &self.__options,
                    "riot.account.by_riot_id",
                )
            }
        }

        const __ENDPOINT: &'static str = <riot::account::ByRiotId as riot::Endpoint>::NAME;

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> riot::Result<::concord::Answer<Option<Account>>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                ::concord::http::StatusCode::NOT_FOUND => {
                    // The arm of the endpoint: the owners in reach, then its inputs.
                    let client = __Client { key: &self.__inputs.__scope.__client.key };
                    let account = __Account { route: &self.__inputs.__scope.route };
                    let game_name = &self.__inputs.game_name;
                    let tag_line = &self.__inputs.tag_line;
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
                            let client = __Client { key: &self.__inputs.__scope.__client.key };
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
                            let client = __Client { key: &self.__inputs.__scope.__client.key };
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
                &self.__inputs.__scope.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__inputs.__scope.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    impl riot::Endpoint for riot::account::ByRiotId {
        type Value = Option<Account>;
        const NAME: &'static str = "riot.account.by_riot_id";
    }

    call_spec!(
        riot,
        riot::account::ByRiotId,
        riot::__AccountByRiotIdInputs,
        Option<Account>,
        RiotProblem,
        "ByRiotId"
    );
    impl riot::__spec::Awaitable for riot::account::ByRiotId {}
    impl riot::__spec::Conditional for riot::account::ByRiotId {}

    // /// The account of a signed-in player.
    // /// Uses the player's own token instead of the API key.
    // GET me(
    //     /// The player's bearer token.
    //     access_token: Secret,
    // ) -> Account {
    //     path "me";
    //     headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }
    // }
    impl riot::Call<riot::account::Me> {
        fn __build_draft(&self) -> riot::Result<::concord::__private::Draft> {
            let mut __draft = ::concord::__private::Draft::new(::concord::http::Method::GET);
            #[allow(unused_variables)]
            {
                let client = __Client {
                    key: &self.__inputs.__scope.__client.key,
                };
                let account = __Account {
                    route: &self.__inputs.__scope.route,
                };
                let route = account.route;
                __draft.base(&[
                    ::concord::__private::Piece::Lit("https://"),
                    ::concord::__private::Piece::Val(
                        "route",
                        &(&::concord::__private::ValueRef(route)).__adapt(),
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
                    key: &self.__inputs.__scope.__client.key,
                };
                let account = __Account {
                    route: &self.__inputs.__scope.route,
                };
                let access_token = &self.__inputs.access_token;
                __draft.header(
                    "authorization",
                    &[
                        ::concord::__private::Piece::Lit("Bearer "),
                        ::concord::__private::Piece::Val(
                            "access_token",
                            &(&::concord::__private::ValueRef(access_token)).__adapt(),
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
                    &self.__inputs.__scope.__client.__core,
                    &self.__options,
                    "riot.account.me",
                )
            } else {
                __draft.finish(
                    &self.__inputs.__scope.__client.__core,
                    &self.__options,
                    "riot.account.me",
                )
            }
        }

        const __ENDPOINT: &'static str = <riot::account::Me as riot::Endpoint>::NAME;

        async fn __read_answer(
            &self,
            __response: ::concord::reqwest::Response,
            __info: ::concord::transport::TryInfo,
        ) -> riot::Result<::concord::Answer<Account>> {
            let __response_url = __response.url().clone();
            let __status = __response.status();
            let __headers = ::concord::Headers::from(__response.headers().clone());
            // docs/internals/macro.md requires each reader expression to run inside a non-async closure.
            #[allow(unused_variables, clippy::redundant_closure_call)]
            let __value = match __status {
                __status if __status.is_success() => {
                    ::concord::__private::read_body(
                        &{
                            let client = __Client { key: &self.__inputs.__scope.__client.key };
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
                            let client = __Client { key: &self.__inputs.__scope.__client.key };
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
                &self.__inputs.__scope.__client.__core,
                &self.__options,
                Self::__ENDPOINT,
            );
            let __idempotent = ::concord::__private::Idempotent::ByMethod;
            let __draft = self.__build_draft()?;
            __draft.prepare(
                __run,
                self.__inputs.__scope.__client.__core.clone(),
                self.__options.clone(),
                __idempotent,
                move |__response, __info| {
                    let __call = self.clone();
                    async move { __call.__read_answer(__response, __info).await }
                },
            )
        }
    }

    impl riot::Endpoint for riot::account::Me {
        type Value = Account;
        const NAME: &'static str = "riot.account.me";
    }

    call_spec!(
        riot,
        riot::account::Me,
        riot::__AccountMeInputs,
        Account,
        RiotProblem,
        "Me"
    );
    impl riot::__spec::Awaitable for riot::account::Me {}
    impl riot::__spec::Conditional for riot::account::Me {}

};
