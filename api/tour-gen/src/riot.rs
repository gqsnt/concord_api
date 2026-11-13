concord::api! {
/// The Riot Games API (League of Legends subset).
///
/// # Example
///
/// ```no_run
/// use tour::{Europe, Puuid, Riot};
///
/// # async fn example() -> tour::riot::Result<()> {
/// let riot = Riot::new("test-key");
/// let puuid = Puuid::from("p-1");
/// match riot.matches(Europe).ids(&puuid).count(5).send().await {
///     Ok(ids) => println!("{} matches", ids.len()),
///     Err(error) if error.is_status(404) => println!("No match history"),
///     Err(error) => {
///         if let Some(problem) = error.problem() {
///             eprintln!("{}", problem.status.message);
///         }
///         return Err(error);
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub client Riot(
    /// The API key. Sent on every request unless an endpoint removes it.
    key: Secret,
) {
    headers { "x-riot-token" = key; }
    read Json;
    problem RiotProblem;
    /// Accounts (regional routing).
    scope account(
        /// The regional routing group for account requests.
        route: AccountRoute,
    ) {
        base "https://{route}.api.riotgames.com";
        path "riot/account/v1/accounts";
        /// The account behind a Riot ID (`name#tag`). `None` when there is no such player.
        GET by_riot_id(
            /// The name before the # in the Riot ID.
            game_name: String,
            /// The tag after the # in the Riot ID.
            tag_line: String,
        ) -> Option<Account> {
            path "by-riot-id/{game_name}/{tag_line}";
            status { 404 => None }
        }
        /// The account of a signed-in player.
        /// Uses the player's own token instead of the API key.
        GET me(
            /// The player's bearer token.
            access_token: Secret,
        ) -> Account {
            path "me";
            headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }
        }
    }
    /// Matches (regional routing).
    scope matches(
        /// The regional routing group for match requests.
        route: MatchRoute,
    ) {
        base "https://{route}.api.riotgames.com";
        path "lol/match/v5/matches";
        /// Match ids of a player, newest first.
        GET ids(
            /// The player's persistent identifier.
            puuid: Puuid,
            /// Index of the first match id to return.
            start?: u32,
            /// Maximum number of match ids in one page.
            count?: u32,
            /// Only the matches of this queue.
            queue?: i32,
            /// Only the matches that started at or after this time, in seconds since the epoch.
            start_time?: i64,
        ) -> Vec<MatchId> {
            path "by-puuid/{puuid}/ids";
            query { start; count; queue; "startTime" = start_time; }
            paginate by_offset_limit(start, count);
        }
        /// One match.
        GET get(
            /// The regional identifier of the match to fetch.
            id: MatchId,
        ) -> Match {
            path "{id}";
        }
    }
}
}
