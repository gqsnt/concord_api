// The tree declarations. Implementations remain beside each authored description.

include!("call_spec.rs");

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
/// Create one with [`Tasks::new`]: `Tasks::new()`.
///
/// Cheap to clone: a clone shares the middlewares, the transport and the settings. [`Send`] + [`Sync`].
#[derive(Clone)]
pub struct Tasks {
    __core: ::concord::__private::Core<()>,
}

/// Calls and results for [`Tasks`].
pub mod tasks {
    type __Problem = ();
    type __Sending<T> = concord::Sending<T>;
    type __Pages<T> = concord::Pages<T>;
    include!("call.rs");

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __ListInputs {
        pub(super) __client: super::Tasks,
        pub(super) done: <() as __Types>::__Input0,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __CreateInputs {
        pub(super) __client: super::Tasks,
        pub(super) task: <() as __Types>::__Input1,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __GetInputs {
        pub(super) __client: super::Tasks,
        pub(super) id: <() as __Types>::__Input2,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __DeleteInputs {
        pub(super) __client: super::Tasks,
        pub(super) id: <() as __Types>::__Input3,
    }

    #[doc(hidden)]
    pub trait __Types {
        type __Input0: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input1: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input2: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input3: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Problem: ::std::marker::Send + ::std::marker::Sync + 'static;
    }

    /// The error of every call of [`Tasks`](crate::Tasks): a [`concord::Error`].
    ///
    /// The API has no problem type; `problem()` gives nothing.
    ///
    /// Start with:
    ///
    /// - [`kind()`](concord::Error::kind): identify Build, Transport, Refused, or Mismatch.
    /// - [`is_status(404)`](concord::Error::is_status): test an HTTP status.
    /// - [`problem()`](concord::Error::problem): borrow the API problem, when present.
    /// - [`is_timeout()`](concord::Error::is_timeout): test whether a timeout ended the call.
    /// - [`status()`](concord::Error::status): inspect the HTTP status, when an answer arrived.
    ///
    /// For example, `{e}` prints names and status, without input values:
    /// `refused: status 429, in tasks.list, after 3 attempts`.
    pub type Error = ::concord::Error;

    /// The result of a [`Tasks`](crate::Tasks) call, with [`Error`].
    pub type Result<T> = ::std::result::Result<T, Error>;

    /// The retry policy of [`Tasks`](crate::Tasks): a [`concord::Retry`]. The API has no problem type: a rule's `tried.problem()` gives nothing.
    ///
    /// Give it to `with_retry` on the client or on a call: `tasks::Retry::standard()`.
    pub type Retry = ::concord::Retry;

    /// List tasks in their current order.
    ///
    /// Request: `GET https://tasks.example.com/api/tasks`.
    ///
    /// Call type: `tour::tasks::Call<tour::tasks::List>`. Create it with `Tasks::list`.
    ///
    /// Options:
    ///
    /// - `tasks::List::done(__call: tasks::Call<tasks::List>, done: impl concord::arg::Arg<Option<bool>>) -> tasks::Call<tasks::List>`: accepts [`bool`], &[`bool`], or an [`Option`]<[`bool`]>. `None` clears it. Filter tasks by completion state.
    ///
    /// `.await` returns [`tasks::Result`](crate::tasks::Result)<[`Vec`]<[`Task`](crate::Task)>>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum List {}

    /// Create a task with a title.
    ///
    /// Request: `POST https://tasks.example.com/api/tasks`.
    ///
    /// Call type: `tour::tasks::Call<tour::tasks::Create>`. Create it with `Tasks::create`.
    ///
    /// Inputs:
    ///
    /// - `task`: [`NewTask`](crate::NewTask) or &[`NewTask`](crate::NewTask). The task to create.
    ///
    /// Retry: an unmarked `POST` may be sent again when the policy lists the
    /// response status with `Retry::not_applied_on`; all other retry checks still apply.
    ///
    /// `.await` returns [`tasks::Result`](crate::tasks::Result)<[`Task`](crate::Task)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    ///
    /// ```compile_fail
    /// let _ = tour::Tasks::new().create(tour::NewTask { title: String::new() }).send_if_none_match("etag");
    /// ```
    ///
    /// ```compile_fail
    /// let _ = tour::tasks::Call::<tour::tasks::Create>::__prepare_if_none_match;
    /// ```
    #[allow(missing_debug_implementations)]
    pub enum Create {}

    /// Fetch one task by its identifier.
    ///
    /// Request: `GET https://tasks.example.com/api/tasks/{id}`.
    ///
    /// Call type: `tour::tasks::Call<tour::tasks::Get>`. Create it with `Tasks::get`.
    ///
    /// Inputs:
    ///
    /// - `id`: [`u64`] or &[`u64`]. The identifier of the task to fetch.
    ///
    /// `.await` returns [`tasks::Result`](crate::tasks::Result)<[`Task`](crate::Task)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    ///
    /// ```compile_fail
    /// let _ = tour::Tasks::new().get(1).send_if_none_match(String::from("etag"));
    /// ```
    #[allow(missing_debug_implementations)]
    pub enum Get {}

    /// Delete one task by its identifier.
    ///
    /// Request: `DELETE https://tasks.example.com/api/tasks/{id}`.
    ///
    /// Call type: `tour::tasks::Call<tour::tasks::Delete>`. Create it with `Tasks::delete`.
    ///
    /// Inputs:
    ///
    /// - `id`: [`u64`] or &[`u64`]. The identifier of the task to delete.
    ///
    /// `.await` returns [`tasks::Result`](crate::tasks::Result)<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    ///
    /// ```compile_fail
    /// let _ = tour::tasks::Call::<tour::tasks::Delete>::send_if_none_match;
    /// ```
    ///
    /// ```compile_fail
    /// let _ = tour::tasks::Call::<tour::tasks::Delete>::__prepare_if_none_match;
    /// ```
    #[allow(missing_debug_implementations)]
    pub enum Delete {}
}

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
///
/// Create one with [`Riot::new`]: `Riot::new(key)`.
///
/// Cheap to clone: a clone shares the middlewares, the transport and the settings. [`Send`] + [`Sync`].
#[derive(Clone)]
pub struct Riot {
    __core: ::concord::__private::Core<<() as riot::__Types>::__Problem>,
    key: <() as riot::__Types>::__Input0,
}

/// Calls and results for [`Riot`].
pub mod riot {
    type __Problem = super::RiotProblem;
    type __Sending<T> = concord::Sending<T, super::RiotProblem>;
    type __Pages<T> = concord::Pages<T, super::RiotProblem>;
    include!("call.rs");

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __MatchesIdsInputs {
        pub(super) __scope: Matches,
        pub(super) puuid: <() as __Types>::__Input6,
        pub(super) start: <() as __Types>::__Input7,
        pub(super) count: <() as __Types>::__Input8,
        pub(super) queue: <() as __Types>::__Input9,
        pub(super) start_time: <() as __Types>::__Input10,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __MatchesGetInputs {
        pub(super) __scope: Matches,
        pub(super) id: <() as __Types>::__Input11,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __AccountByRiotIdInputs {
        pub(super) __scope: Account,
        pub(super) game_name: <() as __Types>::__Input2,
        pub(super) tag_line: <() as __Types>::__Input3,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __AccountMeInputs {
        pub(super) __scope: Account,
        pub(super) access_token: <() as __Types>::__Input4,
    }

    #[doc(hidden)]
    pub trait __Types {
        type __Input0: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input1: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input2: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input3: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input4: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input5: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input6: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input7: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input8: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input9: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input10: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input11: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Problem: ::std::marker::Send + ::std::marker::Sync + 'static;
    }

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __IdsPuuid;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __IdsStart;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __IdsCount;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __IdsQueue;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __IdsStartTime;

    /// Accounts (regional routing).
    ///
    /// Create one with [`Riot::account`](crate::Riot::account).
    #[derive(Clone)]
    pub struct Account {
        pub(super) __client: super::Riot,
        pub(super) route: <() as __Types>::__Input1,
    }

    /// Matches (regional routing).
    ///
    /// Create one with [`Riot::matches`](crate::Riot::matches).
    #[derive(Clone)]
    pub struct Matches {
        pub(super) __client: super::Riot,
        pub(super) route: <() as __Types>::__Input5,
    }

    /// The error of every call of [`Riot`](crate::Riot): a [`concord::Error`] whose API problem is [`RiotProblem`](crate::RiotProblem).
    ///
    /// Start with:
    ///
    /// - [`kind()`](concord::Error::kind): identify Build, Transport, Refused, or Mismatch.
    /// - [`is_status(404)`](concord::Error::is_status): test an HTTP status.
    /// - [`problem()`](concord::Error::problem): borrow the API problem, when present.
    /// - [`is_timeout()`](concord::Error::is_timeout): test whether a timeout ended the call.
    /// - [`status()`](concord::Error::status): inspect the HTTP status, when an answer arrived.
    ///
    /// For example, `{e}` prints names and status, without input values:
    /// `refused: status 429, in riot.matches.ids, after 3 attempts`.
    pub type Error = ::concord::Error<super::RiotProblem>;

    /// The result of a [`Riot`](crate::Riot) call, with [`Error`].
    pub type Result<T> = ::std::result::Result<T, Error>;

    /// The retry policy of [`Riot`](crate::Riot): a [`concord::Retry`] whose rules read the API problem, as [`Error`] does.
    ///
    /// Give it to `with_retry` on the client or on a call: `riot::Retry::standard()`.
    pub type Retry = ::concord::Retry<<() as __Types>::__Problem>;

    /// Calls on [`riot::Matches`](crate::riot::Matches).
    pub mod matches {
        /// Match ids of a player, newest first.
        ///
        /// Request: `GET https://{route}.api.riotgames.com/lol/match/v5/matches/by-puuid/{puuid}/ids`.
        ///
        /// Call type: `tour::riot::Call<tour::riot::matches::Ids>`. Create it with `Matches::ids`.
        ///
        /// Headers: `x-riot-token`.
        ///
        /// Inputs:
        ///
        /// - `puuid`: [`Puuid`](crate::Puuid) or &[`Puuid`](crate::Puuid). The player's persistent identifier.
        ///
        /// Options:
        ///
        /// - `riot::matches::Ids::start(__call: riot::Call<riot::matches::Ids>, start: impl concord::arg::Arg<Option<u32>>) -> riot::Call<riot::matches::Ids>`: accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it. Index of the first match id to return.
        /// - `riot::matches::Ids::count(__call: riot::Call<riot::matches::Ids>, count: impl concord::arg::Arg<Option<u32>>) -> riot::Call<riot::matches::Ids>`: accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it. Maximum number of match ids in one page.
        /// - `riot::matches::Ids::queue(__call: riot::Call<riot::matches::Ids>, queue: impl concord::arg::Arg<Option<i32>>) -> riot::Call<riot::matches::Ids>`: accepts [`i32`], &[`i32`], or an [`Option`]<[`i32`]>. `None` clears it. Only the matches of this queue.
        /// - `riot::matches::Ids::start_time(__call: riot::Call<riot::matches::Ids>, start_time: impl concord::arg::Arg<Option<i64>>) -> riot::Call<riot::matches::Ids>`: accepts [`i64`], &[`i64`], or an [`Option`]<[`i64`]>. `None` clears it. Only the matches that started at or after this time, in seconds since the epoch.
        ///
        /// `.send().await` returns [`riot::Result`](crate::riot::Result)<[`Vec`]<[`MatchId`](crate::MatchId)>>.
        ///
        /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
        /// Paging ends on an empty page or a page shorter than the supplied `count`; `count` is the maximum for each page.
        ///
        /// See [`Call`](crate::riot::Call) for methods shared by every call. This endpoint type has no values.
        ///
        /// ```compile_fail
        /// # async fn c8_8_a_paged_call_cannot_be_awaited() {
        /// let _ = tour::Riot::new("key").matches(tour::Europe).ids(tour::Puuid::from("p")).await;
        /// # }
        /// ```
        #[allow(missing_debug_implementations)]
        pub enum Ids {}

        /// One match.
        ///
        /// Request: `GET https://{route}.api.riotgames.com/lol/match/v5/matches/{id}`.
        ///
        /// Call type: `tour::riot::Call<tour::riot::matches::Get>`. Create it with `Matches::get`.
        ///
        /// Headers: `x-riot-token`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`MatchId`](crate::MatchId) or &[`MatchId`](crate::MatchId). The regional identifier of the match to fetch.
        ///
        /// `.await` returns [`riot::Result`](crate::riot::Result)<[`Match`](crate::Match)>.
        ///
        /// See [`Call`](crate::riot::Call) for methods shared by every call. This endpoint type has no values.
        #[allow(missing_debug_implementations)]
        pub enum Get {}
    }

    /// Calls on [`riot::Account`](crate::riot::Account).
    pub mod account {
        /// The account behind a Riot ID (`name#tag`). `None` when there is no such player.
        ///
        /// Request: `GET https://{route}.api.riotgames.com/riot/account/v1/accounts/by-riot-id/{game_name}/{tag_line}`.
        ///
        /// Call type: `tour::riot::Call<tour::riot::account::ByRiotId>`. Create it with `Account::by_riot_id`.
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
        /// `.await` returns [`riot::Result`](crate::riot::Result)<[`Option`]<[`tour::Account`](crate::Account)>>.
        ///
        /// See [`Call`](crate::riot::Call) for methods shared by every call. This endpoint type has no values.
        #[allow(missing_debug_implementations)]
        pub enum ByRiotId {}

        /// The account of a signed-in player.
        /// Uses the player's own token instead of the API key.
        ///
        /// Request: `GET https://{route}.api.riotgames.com/riot/account/v1/accounts/me`.
        ///
        /// Call type: `tour::riot::Call<tour::riot::account::Me>`. Create it with `Account::me`.
        ///
        /// Headers: `authorization`.
        ///
        /// Inputs:
        ///
        /// - `access_token`: [`Secret`](concord::Secret), &[`Secret`](concord::Secret), [`String`], or &[`str`]. The player's bearer token.
        ///
        /// `.await` returns [`riot::Result`](crate::riot::Result)<[`tour::Account`](crate::Account)>.
        ///
        /// See [`Call`](crate::riot::Call) for methods shared by every call. This endpoint type has no values.
        #[allow(missing_debug_implementations)]
        pub enum Me {}
    }
}

/// A client for documents, file contents, and file search.
///
/// # Example
///
/// ```no_run
/// use tour::{DocId, Files};
///
/// # async fn example() -> tour::files::Result<()> {
/// let files = Files::new("key");
/// let exists = files.exists(DocId::from("readme")).await?;
/// # Ok(())
/// # }
/// ```
///
/// Create one with [`Files::new`]: `Files::new(key)`.
///
/// Every request goes through a middleware before the transport.
/// Signs every request with the client key.
/// `with_transport` keeps this middleware; caller middlewares run before it.
///
/// Cheap to clone: a clone shares the middlewares, the transport and the settings. [`Send`] + [`Sync`].
#[derive(Clone)]
pub struct Files {
    __core: ::concord::__private::Core<()>,
    key: <() as files::__Types>::__Input0,
}

/// Calls and results for [`Files`].
pub mod files {
    type __Problem = ();
    type __Sending<T> = concord::Sending<T>;
    type __Pages<T> = concord::Pages<T>;
    include!("call.rs");

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __GrantInputs {
        pub(super) __client: super::Files,
        pub(super) grant_type: <() as __Types>::__Input8,
        pub(super) client_id: <() as __Types>::__Input9,
        pub(super) client_secret: <() as __Types>::__Input10,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __ExistsInputs {
        pub(super) __client: super::Files,
        pub(super) id: <() as __Types>::__Input1,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __ReadmeInputs {
        pub(super) __client: super::Files,
        pub(super) repo: <() as __Types>::__Input2,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __DownloadInputs {
        pub(super) __client: super::Files,
        pub(super) id: <() as __Types>::__Input3,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __UploadInputs {
        pub(super) __client: super::Files,
        pub(super) name: <() as __Types>::__Input4,
        pub(super) file: <() as __Types>::__Input5,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __SearchInputs {
        pub(super) __client: super::Files,
        pub(super) text: <() as __Types>::__Input6,
        pub(super) cursor: <() as __Types>::__Input7,
    }

    #[doc(hidden)]
    pub trait __Types {
        type __Input0: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input1: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input2: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input3: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input4: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input5: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input6: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input7: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input8: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input9: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input10: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Problem: ::std::marker::Send + ::std::marker::Sync + 'static;
    }

    /// Exchange credentials with `body form { grant_type; client_id; client_secret; }`.
    /// Text arguments accept owned or borrowed strings; the secret also accepts `Secret`.
    ///
    /// Request: `POST https://files.example.com/oauth/token`.
    ///
    /// Call type: `tour::files::Call<tour::files::Grant>`. Create it with `Files::grant`.
    ///
    /// `.await` returns [`files::Result`](crate::files::Result)<()>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    ///
    /// ```compile_fail
    /// let _ = tour::files::Call::<tour::files::Grant>::send_if_none_match;
    /// ```
    ///
    /// ```compile_fail
    /// let _ = tour::files::Call::<tour::files::Grant>::__prepare_if_none_match;
    /// ```
    #[allow(missing_debug_implementations)]
    pub enum Grant {}

    /// The error of every call of [`Files`](crate::Files): a [`concord::Error`].
    ///
    /// The API has no problem type; `problem()` gives nothing.
    ///
    /// Start with:
    ///
    /// - [`kind()`](concord::Error::kind): identify Build, Transport, Refused, or Mismatch.
    /// - [`is_status(404)`](concord::Error::is_status): test an HTTP status.
    /// - [`problem()`](concord::Error::problem): borrow the API problem, when present.
    /// - [`is_timeout()`](concord::Error::is_timeout): test whether a timeout ended the call.
    /// - [`status()`](concord::Error::status): inspect the HTTP status, when an answer arrived.
    ///
    /// For example, `{e}` prints names and status, without input values:
    /// `refused: status 429, in files.download, after 3 attempts`.
    pub type Error = ::concord::Error;

    /// The result of a [`Files`](crate::Files) call, with [`Error`].
    pub type Result<T> = ::std::result::Result<T, Error>;

    /// The retry policy of [`Files`](crate::Files): a [`concord::Retry`]. The API has no problem type: a rule's `tried.problem()` gives nothing.
    ///
    /// Give it to `with_retry` on the client or on a call: `files::Retry::standard()`.
    pub type Retry = ::concord::Retry;

    /// Is it there?
    ///
    /// Request: `HEAD https://files.example.com/{id}`.
    ///
    /// Call type: `tour::files::Call<tour::files::Exists>`. Create it with `Files::exists`.
    ///
    /// Inputs:
    ///
    /// - `id`: [`DocId`](crate::DocId) or &[`DocId`](crate::DocId). The identifier of the document to check.
    ///
    /// Status: `2xx`.
    ///
    /// Status: `404`.
    ///
    /// `.await` returns [`files::Result`](crate::files::Result)<[`bool`]>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Exists {}

    /// Plain text, not JSON.
    ///
    /// Request: `GET https://files.example.com/{repo}/readme`.
    ///
    /// Call type: `tour::files::Call<tour::files::Readme>`. Create it with `Files::readme`.
    ///
    /// Inputs:
    ///
    /// - `repo`: [`String`], &[`String`], or &[`str`]. The repository whose README should be read.
    ///
    /// `.await` returns [`files::Result`](crate::files::Result)<[`String`]>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Readme {}

    /// A large file, as it comes.
    ///
    /// Request: `GET https://files.example.com/files/{id}`.
    ///
    /// Call type: `tour::files::Call<tour::files::Download>`. Create it with `Files::download`.
    ///
    /// Inputs:
    ///
    /// - `id`: [`FileId`](crate::FileId) or &[`FileId`](crate::FileId). The identifier of the file to download.
    ///
    /// `.await` returns [`files::Result`](crate::files::Result)<[`concord::ByteStream`]>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Download {}

    /// An upload, on another host.
    ///
    /// Request: `PUT https://uploads.example.com/files/{name}`.
    ///
    /// Call type: `tour::files::Call<tour::files::Upload>`. Create it with `Files::upload`.
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
    /// `.await` returns [`files::Result`](crate::files::Result)<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    ///
    /// ```compile_fail
    /// let _ = tour::Files::new("key").upload("a", concord::Body::new("", "text/plain")).send_if_none_match("etag");
    /// ```
    ///
    /// ```compile_fail
    /// let _ = tour::files::Call::<tour::files::Upload>::__prepare_if_none_match;
    /// ```
    #[allow(missing_debug_implementations)]
    pub enum Upload {}

    /// A body made of several inputs. A `cursor` that was not set is left out.
    ///
    /// Request: `POST https://files.example.com/search`.
    ///
    /// Call type: `tour::files::Call<tour::files::Search>`. Create it with `Files::search`.
    ///
    /// Inputs:
    ///
    /// - `text`: [`String`], &[`String`], or &[`str`]. The text to search for.
    ///
    /// Options:
    ///
    /// - `files::Search::cursor(__call: files::Call<files::Search>, cursor: impl concord::arg::Arg<Option<String>>) -> files::Call<files::Search>`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. Continue the search after this cursor.
    ///   When it is not set, the body has no `cursor` member.
    ///
    /// Retry: an unmarked `POST` may be sent again when the policy lists the
    /// response status with `Retry::not_applied_on`; all other retry checks still apply.
    ///
    /// `.await` returns [`files::Result`](crate::files::Result)<[`Page`](crate::Page)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    ///
    /// ```compile_fail
    /// let _ = tour::files::Call::<tour::files::Search>::send_if_none_match;
    /// ```
    ///
    /// ```compile_fail
    /// let _ = tour::files::Call::<tour::files::Search>::__prepare_if_none_match;
    /// ```
    #[allow(missing_debug_implementations)]
    pub enum Search {}
}

/// A client for a feed whose pages are linked by cursors.
///
/// # Example
///
/// ```no_run
/// use tour::Feed;
///
/// # async fn example() -> tour::feed::Result<()> {
/// let feed = Feed::new();
/// let mut pages = feed.by_cursor().pages();
/// while let Some(page) = pages.try_next().await? {
///     for entry in page { println!("{}", entry.name); }
/// }
/// # Ok(())
/// # }
/// ```
///
/// Create one with [`Feed::new`]: `Feed::new()`.
///
/// Cheap to clone: a clone shares the middlewares, the transport and the settings. [`Send`] + [`Sync`].
#[derive(Clone)]
pub struct Feed {
    __core: ::concord::__private::Core<()>,
}

/// Calls and results for [`Feed`].
pub mod feed {
    type __Problem = ();
    type __Sending<T> = concord::Sending<T>;
    type __Pages<T> = concord::Pages<T>;
    include!("call.rs");

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __ListInputs {
        pub(super) __client: super::Feed,
        pub(super) starting_after: <() as __Types>::__Input0,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __ByCursorInputs {
        pub(super) __client: super::Feed,
        pub(super) starting_after: <() as __Types>::__Input1,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __LinkedInputs {
        pub(super) __client: super::Feed,
    }

    #[doc(hidden)]
    pub trait __Types {
        type __Input0: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Problem: ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input1: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
    }

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __ListStartingAfter;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __ByCursorStartingAfter;

    /// The error of every call of [`Feed`](crate::Feed): a [`concord::Error`].
    ///
    /// The API has no problem type; `problem()` gives nothing.
    ///
    /// Start with:
    ///
    /// - [`kind()`](concord::Error::kind): identify Build, Transport, Refused, or Mismatch.
    /// - [`is_status(404)`](concord::Error::is_status): test an HTTP status.
    /// - [`problem()`](concord::Error::problem): borrow the API problem, when present.
    /// - [`is_timeout()`](concord::Error::is_timeout): test whether a timeout ended the call.
    /// - [`status()`](concord::Error::status): inspect the HTTP status, when an answer arrived.
    ///
    /// For example, `{e}` prints names and status, without input values:
    /// `refused: status 429, in feed.list, after 3 attempts`.
    pub type Error = ::concord::Error;

    /// The result of a [`Feed`](crate::Feed) call, with [`Error`].
    pub type Result<T> = ::std::result::Result<T, Error>;

    /// The retry policy of [`Feed`](crate::Feed): a [`concord::Retry`]. The API has no problem type: a rule's `tried.problem()` gives nothing.
    ///
    /// Give it to `with_retry` on the client or on a call: `feed::Retry::standard()`.
    pub type Retry = ::concord::Retry;

    /// List feed entries using the cursor returned by the server.
    ///
    /// Request: `GET https://feed.example.com/items`.
    ///
    /// Call type: `tour::feed::Call<tour::feed::List>`. Create it with `Feed::list`.
    ///
    /// Options:
    ///
    /// - `feed::List::starting_after(__call: feed::Call<feed::List>, starting_after: impl concord::arg::Arg<Option<String>>) -> feed::Call<feed::List>`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. Continue after this cursor.
    ///
    /// `.send().await` returns [`feed::Result`](crate::feed::Result)<[`Page`](crate::Page)>.
    ///
    /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    ///
    /// ```compile_fail
    /// # async fn c8_8_a_paged_call_cannot_be_awaited() {
    /// let _ = tour::Feed::new().list().await;
    /// # }
    /// ```
    #[allow(missing_debug_implementations)]
    pub enum List {}

    /// List feed entries with an ordinary cursor pager.
    ///
    /// Request: `GET https://feed.example.com/cursor-items`.
    ///
    /// Call type: `tour::feed::Call<tour::feed::ByCursor>`. Create it with `Feed::by_cursor`.
    ///
    /// Options:
    ///
    /// - `feed::ByCursor::starting_after(__call: feed::Call<feed::ByCursor>, starting_after: impl concord::arg::Arg<Option<String>>) -> feed::Call<feed::ByCursor>`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. Continue after this cursor.
    ///
    /// `.send().await` returns [`feed::Result`](crate::feed::Result)<[`Page`](crate::Page)>.
    ///
    /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    ///
    /// ```compile_fail
    /// # async fn c8_8_a_paged_call_cannot_be_awaited() {
    /// let _ = tour::Feed::new().by_cursor().await;
    /// # }
    /// ```
    #[allow(missing_debug_implementations)]
    pub enum ByCursor {}

    /// List feed entries by following the next Link header.
    ///
    /// Request: `GET https://feed.example.com/linked-items`.
    ///
    /// Call type: `tour::feed::Call<tour::feed::Linked>`. Create it with `Feed::linked`.
    ///
    /// `.send().await` returns [`feed::Result`](crate::feed::Result)<[`Page`](crate::Page)>.
    ///
    /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    ///
    /// ```compile_fail
    /// # async fn c8_8_a_paged_call_cannot_be_awaited() {
    /// let _ = tour::Feed::new().linked().await;
    /// # }
    /// ```
    #[allow(missing_debug_implementations)]
    pub enum Linked {}
}

/// A client whose endpoints have the shapes that the tests of one call need.
///
/// # Example
///
/// ```no_run
/// use tour::Shapes;
///
/// # async fn example() -> tour::shapes::Result<()> {
/// let shapes = Shapes::new();
/// let item = shapes.wrapped().get(1).await?;
/// # Ok(())
/// # }
/// ```
///
/// Create one with [`Shapes::new`]: `Shapes::new()`.
///
/// Cheap to clone: a clone shares the middlewares, the transport and the settings. [`Send`] + [`Sync`].
#[derive(Clone)]
pub struct Shapes {
    __core: ::concord::__private::Core<<() as shapes::__Types>::__Problem>,
}

/// Calls and results for [`Shapes`].
pub mod shapes {
    type __Problem = super::ShapesProblem;
    type __Sending<T> = concord::Sending<T, super::ShapesProblem>;
    type __Pages<T> = concord::Pages<T, super::ShapesProblem>;
    include!("call.rs");

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __AgainInputs {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input0,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __ReplaceInputs {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input1,
        pub(super) item: <() as __Types>::__Input2,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __OrderInputs {
        pub(super) __client: super::Shapes,
        pub(super) trace: <() as __Types>::__Input3,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __UploadInputs {
        pub(super) __client: super::Shapes,
        pub(super) body: <() as __Types>::__Input4,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __OutcomeInputs {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input5,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __PingInputs {
        pub(super) __client: super::Shapes,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __PresentInputs {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input6,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __CreateInputs {
        pub(super) __client: super::Shapes,
        pub(super) item: <() as __Types>::__Input7,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __HostedInputs {
        pub(super) __client: super::Shapes,
        pub(super) tenant: <() as __Types>::__Input8,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __SearchInputs {
        pub(super) __client: super::Shapes,
        pub(super) ratio: <() as __Types>::__Input9,
        pub(super) filter: <() as __Types>::__Input10,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __MeasuredInputs {
        pub(super) __client: super::Shapes,
        pub(super) item: <() as __Types>::__Input11,
        pub(super) length: <() as __Types>::__Input12,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __ParsedInputs {
        pub(super) __client: super::Shapes,
        pub(super) raw: <() as __Types>::__Input13,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __CountsInputs {
        pub(super) __client: super::Shapes,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __StrictInputs {
        pub(super) __client: super::Shapes,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __LongMessageInputs {
        pub(super) __client: super::Shapes,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __NotedInputs {
        pub(super) __client: super::Shapes,
        pub(super) trace: <() as __Types>::__Input19,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __TitledInputs {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input20,
        pub(super) title: <() as __Types>::__Input21,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __HeldInputs {
        pub(super) __client: super::Shapes,
        pub(super) body: <() as __Types>::__Input22,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __RefusalInputs {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input23,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __GuardedInputs {
        pub(super) __client: super::Shapes,
        pub(super) path_key: <() as __Types>::__Input24,
        pub(super) query_key: <() as __Types>::__Input25,
        pub(super) header_key: <() as __Types>::__Input26,
        pub(super) cursor: <() as __Types>::__Input27,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __HeadedInputs {
        pub(super) __client: super::Shapes,
        pub(super) cursor: <() as __Types>::__Input28,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __PostedInputs {
        pub(super) __client: super::Shapes,
        pub(super) token: <() as __Types>::__Input29,
        pub(super) cursor: <() as __Types>::__Input30,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __StreamedInputs {
        pub(super) __client: super::Shapes,
        pub(super) file: <() as __Types>::__Input31,
        pub(super) cursor: <() as __Types>::__Input32,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __RenewedInputs {
        pub(super) __client: super::Shapes,
        pub(super) cursor: <() as __Types>::__Input33,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __KeyedHostInputs {
        pub(super) __client: super::Shapes,
        pub(super) host_key: <() as __Types>::__Input34,
        pub(super) cursor: <() as __Types>::__Input35,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __RoamingInputs {
        pub(super) __client: super::Shapes,
        pub(super) tenant: <() as __Types>::__Input36,
        pub(super) cursor: <() as __Types>::__Input37,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __OffsetInputs {
        pub(super) __client: super::Shapes,
        pub(super) start: <() as __Types>::__Input38,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __NumberedInputs {
        pub(super) __client: super::Shapes,
        pub(super) page: <() as __Types>::__Input39,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __AddressedInputs {
        pub(super) __client: super::Shapes,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __RemoveInputs {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input40,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __SettledInputs {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input41,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __PartInputs {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input42,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __WrappedGetInputs {
        pub(super) __scope: Wrapped,
        pub(super) id: <() as __Types>::__Input14,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __WrappedRawInputs {
        pub(super) __scope: Wrapped,
        pub(super) id: <() as __Types>::__Input15,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __WrappedFindInputs {
        pub(super) __scope: Wrapped,
        pub(super) id: <() as __Types>::__Input16,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __WrappedTouchInputs {
        pub(super) __scope: Wrapped,
        pub(super) id: <() as __Types>::__Input17,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __WrappedHoldsInputs {
        pub(super) __scope: Wrapped,
        pub(super) id: <() as __Types>::__Input18,
    }

    #[doc(hidden)]
    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub struct __SealedFileInputs {
        pub(super) __scope: Sealed,
        pub(super) id: <() as __Types>::__Input43,
    }

    #[doc(hidden)]
    pub trait __Types {
        type __Input0: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input1: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input2: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input3: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input4: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input5: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input6: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input7: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input8: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input9: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input10: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input11: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input12: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input13: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input14: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input15: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input16: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input17: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input18: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input19: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input20: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input21: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input22: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input23: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input24: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input25: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input26: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input27: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input28: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input29: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input30: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input31: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input32: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input33: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input34: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input35: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input36: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input37: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input38: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input39: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input40: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input41: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input42: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Input43: ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync + 'static;
        type __Problem: ::std::marker::Send + ::std::marker::Sync + 'static;
    }

    /// Items in an envelope.
    ///
    /// Create one with [`Shapes::wrapped`](crate::Shapes::wrapped).
    #[derive(Clone)]
    pub struct Wrapped {
        pub(super) __client: super::Shapes,
    }

    /// Files whose answers carry a seal.
    ///
    /// Create one with [`Shapes::sealed`](crate::Shapes::sealed).
    #[derive(Clone)]
    pub struct Sealed {
        pub(super) __client: super::Shapes,
    }

    /// The error of every call of [`Shapes`](crate::Shapes): a [`concord::Error`] whose API problem is [`ShapesProblem`](crate::ShapesProblem).
    ///
    /// Start with:
    ///
    /// - [`kind()`](concord::Error::kind): identify Build, Transport, Refused, or Mismatch.
    /// - [`is_status(404)`](concord::Error::is_status): test an HTTP status.
    /// - [`problem()`](concord::Error::problem): borrow the API problem, when present.
    /// - [`is_timeout()`](concord::Error::is_timeout): test whether a timeout ended the call.
    /// - [`status()`](concord::Error::status): inspect the HTTP status, when an answer arrived.
    ///
    /// For example, `{e}` prints names and status, without input values:
    /// `refused: status 429, in shapes.again, after 3 attempts`.
    pub type Error = ::concord::Error<super::ShapesProblem>;

    /// The result of a [`Shapes`](crate::Shapes) call, with [`Error`].
    pub type Result<T> = ::std::result::Result<T, Error>;

    /// The retry policy of [`Shapes`](crate::Shapes): a [`concord::Retry`] whose rules read the API problem, as [`Error`] does.
    ///
    /// Give it to `with_retry` on the client or on a call: `shapes::Retry::standard()`.
    pub type Retry = ::concord::Retry<<() as __Types>::__Problem>;

    /// Send an item again. This `POST` may be repeated.
    ///
    /// Request: `POST https://shapes.example.com/again/{id}`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Again>`. Create it with `Shapes::again`.
    ///
    /// Inputs:
    ///
    /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Again {}

    /// Replace an item. This `PUT` may not be repeated.
    ///
    /// Request: `PUT https://shapes.example.com/items/{id}`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Replace>`. Create it with `Shapes::replace`.
    ///
    /// Inputs:
    ///
    /// - `id`: [`u64`] or &[`u64`]. The identifier of the item to replace.
    /// - `item`: [`Item`](crate::Item) or &[`Item`](crate::Item). The new item.
    ///
    /// Retry: not sent again, even when retry is on, unless the policy lists the
    /// response status with `Retry::not_applied_on`.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Replace {}

    /// Note when each expression of a send runs.
    ///
    /// Request: `POST https://shapes.example.com/order`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Order>`. Create it with `Shapes::order`.
    ///
    /// Inputs:
    ///
    /// - `trace`: [`Trace`](crate::Trace) or &[`Trace`](crate::Trace). What each expression notes, and what it gives.
    ///
    /// Retry: whether this call may be sent again is decided from its inputs, for each send.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Order {}

    /// Upload a body that the caller gives: bytes, or a stream.
    ///
    /// Request: `POST https://shapes.example.com/upload`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Upload>`. Create it with `Shapes::upload`.
    ///
    /// Inputs:
    ///
    /// - `body`: [`concord::Body`] or &[`concord::Body`]. The body to send.
    ///   Make one with [`concord::Body::new(bytes, "application/octet-stream")`](concord::Body::new)
    ///   or [`concord::Body::stream(stream)`](concord::Body::stream); a stream is sent once.
    ///
    /// Retry: an unmarked `POST` may be sent again when the policy lists the
    /// response status with `Retry::not_applied_on`; all other retry checks still apply.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Upload {}

    /// An item, or the conflict that holds it.
    ///
    /// Request: `GET https://shapes.example.com/outcome/{id}`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Outcome>`. Create it with `Shapes::outcome`.
    ///
    /// Inputs:
    ///
    /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
    ///
    /// Status: `409`.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Result`]<[`Item`](crate::Item), [`Conflict`](crate::Conflict)>>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Outcome {}

    /// Check that the service answers.
    ///
    /// Request: `HEAD https://shapes.example.com/ping`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Ping>`. Create it with `Shapes::ping`.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Ping {}

    /// Whether an item is present. Only a `204` says yes.
    ///
    /// Request: `HEAD https://shapes.example.com/items/{id}`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Present>`. Create it with `Shapes::present`.
    ///
    /// Inputs:
    ///
    /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
    ///
    /// Status: `204`.
    ///
    /// Status: `404`.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`bool`]>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Present {}

    /// Create an item, and keep where it was put.
    ///
    /// Request: `POST https://shapes.example.com/items`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Create>`. Create it with `Shapes::create`.
    ///
    /// Inputs:
    ///
    /// - `item`: [`Item`](crate::Item) or &[`Item`](crate::Item). The item to create.
    ///
    /// Status: `201`.
    ///
    /// Status: `202`.
    ///
    /// Status: `3xx`.
    ///
    /// Retry: an unmarked `POST` may be sent again when the policy lists the
    /// response status with `Retry::not_applied_on`; all other retry checks still apply.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Receipt`](crate::Receipt)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Create {}

    /// An item, on the host of a tenant.
    ///
    /// Request: `GET https://{tenant}.shapes.example.com/tenant`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Hosted>`. Create it with `Shapes::hosted`.
    ///
    /// Inputs:
    ///
    /// - `tenant`: [`String`], &[`String`], or &[`str`]. The tenant: one label of the host.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Hosted {}

    /// Search with a number, and a filter that a query cannot hold.
    ///
    /// Request: `GET https://shapes.example.com/search`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Search>`. Create it with `Shapes::search`.
    ///
    /// Inputs:
    ///
    /// - `ratio`: [`f64`] or &[`f64`]. A number, written in the query as JSON writes it.
    ///
    /// Options:
    ///
    /// - `shapes::Search::filter(__call: shapes::Call<shapes::Search>, filter: impl concord::arg::Arg<Option<Filter>>) -> shapes::Call<shapes::Search>`: accepts [`Filter`](crate::Filter), &[`Filter`](crate::Filter), or an [`Option`]<[`Filter`](crate::Filter)>. `None` clears it. A struct: a query refuses it when the request is built.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Vec`]<[`Item`](crate::Item)>>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Search {}

    /// Send an item with a length written by hand, in a body that may fail.
    ///
    /// Request: `POST https://shapes.example.com/measured`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Measured>`. Create it with `Shapes::measured`.
    ///
    /// Headers: `content-length`.
    ///
    /// Inputs:
    ///
    /// - `item`: [`Item`](crate::Item) or &[`Item`](crate::Item). The item. One with an empty title is not written.
    /// - `length`: [`u64`] or &[`u64`]. The content-length to send.
    ///
    /// Retry: an unmarked `POST` may be sent again when the policy lists the
    /// response status with `Retry::not_applied_on`; all other retry checks still apply.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Measured {}

    /// An item, asked with a number that is parsed when the request is built.
    ///
    /// Request: `GET https://shapes.example.com/parsed`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Parsed>`. Create it with `Shapes::parsed`.
    ///
    /// Inputs:
    ///
    /// - `raw`: [`String`], &[`String`], or &[`str`]. The number, as text.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Parsed {}

    /// Counts by name.
    ///
    /// Request: `GET https://shapes.example.com/counts`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Counts>`. Create it with `Shapes::counts`.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Tally`](crate::Tally)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Counts {}

    /// An item, read only when the answer is labelled JSON.
    ///
    /// Request: `GET https://shapes.example.com/strict`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Strict>`. Create it with `Shapes::strict`.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Strict {}

    /// An item, read by a reader whose message is long.
    ///
    /// Request: `GET https://shapes.example.com/long-message`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::LongMessage>`. Create it with `Shapes::long_message`.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum LongMessage {}

    /// An item, read by a reader that notes each answer it reads.
    ///
    /// Request: `GET https://shapes.example.com/noted`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Noted>`. Create it with `Shapes::noted`.
    ///
    /// Inputs:
    ///
    /// - `trace`: [`Trace`](crate::Trace) or &[`Trace`](crate::Trace). What the reader notes.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Noted {}

    /// An item; a missing one is made of what was asked.
    ///
    /// Request: `GET https://shapes.example.com/titled/{id}`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Titled>`. Create it with `Shapes::titled`.
    ///
    /// Inputs:
    ///
    /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
    /// - `title`: [`String`], &[`String`], or &[`str`]. The title of a missing item.
    ///
    /// Status: `404`.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Titled {}

    /// An item, or the conflict that holds it, asked by an input named `body`.
    ///
    /// Request: `GET https://shapes.example.com/held/{body}`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Held>`. Create it with `Shapes::held`.
    ///
    /// Inputs:
    ///
    /// - `body`: [`u64`] or &[`u64`]. The identifier of the item. Its name is the field of an arm.
    ///
    /// Status: `409`.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Result`]<[`Item`](crate::Item), [`Conflict`](crate::Conflict)>>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Held {}

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __GuardedPathKey;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __GuardedQueryKey;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __GuardedHeaderKey;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __GuardedCursor;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __HeadedCursor;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __PostedToken;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __PostedCursor;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __StreamedFile;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __StreamedCursor;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __RenewedCursor;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __KeyedHostHostKey;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __KeyedHostCursor;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __RoamingTenant;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __RoamingCursor;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __OffsetStart;

    #[doc(hidden)]
    #[derive(Clone, Copy)]
    pub(super) struct __NumberedPage;

    /// An item. A refusal is read by the arm of the endpoint, and ends with its problem.
    ///
    /// Request: `GET https://shapes.example.com/refusal/{id}`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Refusal>`. Create it with `Shapes::refusal`.
    ///
    /// Inputs:
    ///
    /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
    ///
    /// Status: `4xx`.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Refusal {}

    /// Pages of items behind three keys: one in the path, one in the query, one in a header.
    ///
    /// Request: `GET https://shapes.example.com/guarded/{path_key}`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Guarded>`. Create it with `Shapes::guarded`.
    ///
    /// Headers: `x-key`.
    ///
    /// Inputs:
    ///
    /// - `path_key`: [`Secret`](concord::Secret), &[`Secret`](concord::Secret), [`String`], or &[`str`]. A key written in the path.
    ///
    /// Options:
    ///
    /// - `shapes::Guarded::query_key(__call: shapes::Call<shapes::Guarded>, query_key: impl concord::arg::Arg<Option<Secret>>) -> shapes::Call<shapes::Guarded>`: accepts [`Secret`](concord::Secret), &[`Secret`](concord::Secret), [`String`], &[`str`], or an [`Option`]<[`Secret`](concord::Secret)>. `None` clears it. A key written as the query pair `key`.
    /// - `shapes::Guarded::header_key(__call: shapes::Call<shapes::Guarded>, header_key: impl concord::arg::Arg<Option<Secret>>) -> shapes::Call<shapes::Guarded>`: accepts [`Secret`](concord::Secret), &[`Secret`](concord::Secret), [`String`], &[`str`], or an [`Option`]<[`Secret`](concord::Secret)>. `None` clears it. A key written in the header `x-key`.
    /// - `shapes::Guarded::cursor(__call: shapes::Call<shapes::Guarded>, cursor: impl concord::arg::Arg<Option<String>>) -> shapes::Call<shapes::Guarded>`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. The cursor of the page.
    ///
    /// `.send().await` returns [`shapes::Result`](crate::shapes::Result)<[`Batch`](crate::Batch)>.
    ///
    /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Guarded {}

    /// Pages of items whose cursor goes in a header.
    ///
    /// Request: `GET https://shapes.example.com/headed`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Headed>`. Create it with `Shapes::headed`.
    ///
    /// Headers: `x-cursor`.
    ///
    /// Options:
    ///
    /// - `shapes::Headed::cursor(__call: shapes::Call<shapes::Headed>, cursor: impl concord::arg::Arg<Option<String>>) -> shapes::Call<shapes::Headed>`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. The cursor of the page, sent as `x-cursor`.
    ///
    /// `.send().await` returns [`shapes::Result`](crate::shapes::Result)<[`Batch`](crate::Batch)>.
    ///
    /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Headed {}

    /// Pages of items asked with a body of entries: a token and the cursor.
    ///
    /// Request: `POST https://shapes.example.com/posted`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Posted>`. Create it with `Shapes::posted`.
    ///
    /// Options:
    ///
    /// - `shapes::Posted::token(__call: shapes::Call<shapes::Posted>, token: impl concord::arg::Arg<Option<Secret>>) -> shapes::Call<shapes::Posted>`: accepts [`Secret`](concord::Secret), &[`Secret`](concord::Secret), [`String`], &[`str`], or an [`Option`]<[`Secret`](concord::Secret)>. `None` clears it. A token written in the body.
    /// - `shapes::Posted::cursor(__call: shapes::Call<shapes::Posted>, cursor: impl concord::arg::Arg<Option<String>>) -> shapes::Call<shapes::Posted>`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. The cursor of the page, written in the body.
    ///
    /// `.send().await` returns [`shapes::Result`](crate::shapes::Result)<[`Batch`](crate::Batch)>.
    ///
    /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Posted {}

    /// Pages of items asked with a body that the caller gives: bytes, or a stream.
    ///
    /// Request: `POST https://shapes.example.com/streamed`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Streamed>`. Create it with `Shapes::streamed`.
    ///
    /// Inputs:
    ///
    /// - `file`: [`concord::Body`] or &[`concord::Body`]. The body of every page.
    ///
    /// Options:
    ///
    /// - `shapes::Streamed::cursor(__call: shapes::Call<shapes::Streamed>, cursor: impl concord::arg::Arg<Option<String>>) -> shapes::Call<shapes::Streamed>`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. The cursor of the page.
    ///
    /// `.send().await` returns [`shapes::Result`](crate::shapes::Result)<[`Batch`](crate::Batch)>.
    ///
    /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Streamed {}

    /// Pages of items asked with a body that is a new stream for each send.
    ///
    /// Request: `POST https://shapes.example.com/renewed`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Renewed>`. Create it with `Shapes::renewed`.
    ///
    /// Options:
    ///
    /// - `shapes::Renewed::cursor(__call: shapes::Call<shapes::Renewed>, cursor: impl concord::arg::Arg<Option<String>>) -> shapes::Call<shapes::Renewed>`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. The cursor of the page.
    ///
    /// `.send().await` returns [`shapes::Result`](crate::shapes::Result)<[`Batch`](crate::Batch)>.
    ///
    /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Renewed {}

    /// Pages of items on a host whose first label is a key.
    ///
    /// Request: `GET https://{host_key}.shapes.example.com/keyed`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::KeyedHost>`. Create it with `Shapes::keyed_host`.
    ///
    /// Inputs:
    ///
    /// - `host_key`: [`Secret`](concord::Secret), &[`Secret`](concord::Secret), [`String`], or &[`str`]. A key written as the first label of the host.
    ///
    /// Options:
    ///
    /// - `shapes::KeyedHost::cursor(__call: shapes::Call<shapes::KeyedHost>, cursor: impl concord::arg::Arg<Option<String>>) -> shapes::Call<shapes::KeyedHost>`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. The cursor of the page.
    ///
    /// `.send().await` returns [`shapes::Result`](crate::shapes::Result)<[`Batch`](crate::Batch)>.
    ///
    /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum KeyedHost {}

    /// Pages of items whose pager moves the tenant, which stands in the host.
    ///
    /// Request: `GET https://{tenant}.shapes.example.com/roaming`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Roaming>`. Create it with `Shapes::roaming`.
    ///
    /// Inputs:
    ///
    /// - `tenant`: [`String`], &[`String`], or &[`str`]. The tenant: one label of the host.
    ///
    /// Options:
    ///
    /// - `shapes::Roaming::cursor(__call: shapes::Call<shapes::Roaming>, cursor: impl concord::arg::Arg<Option<String>>) -> shapes::Call<shapes::Roaming>`: accepts [`String`], &[`String`], &[`str`], or an [`Option`]<[`String`]>. `None` clears it. The cursor of the page.
    ///
    /// `.send().await` returns [`shapes::Result`](crate::shapes::Result)<[`Batch`](crate::Batch)>.
    ///
    /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Roaming {}

    /// Pages of items that start at an offset.
    ///
    /// Request: `GET https://shapes.example.com/offset`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Offset>`. Create it with `Shapes::offset`.
    ///
    /// Options:
    ///
    /// - `shapes::Offset::start(__call: shapes::Call<shapes::Offset>, start: impl concord::arg::Arg<Option<u32>>) -> shapes::Call<shapes::Offset>`: accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it. The index of the first item.
    ///
    /// `.send().await` returns [`shapes::Result`](crate::shapes::Result)<[`Vec`]<[`Item`](crate::Item)>>.
    ///
    /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Offset {}

    /// Pages of items asked by their number.
    ///
    /// Request: `GET https://shapes.example.com/numbered`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Numbered>`. Create it with `Shapes::numbered`.
    ///
    /// Options:
    ///
    /// - `shapes::Numbered::page(__call: shapes::Call<shapes::Numbered>, page: impl concord::arg::Arg<Option<u32>>) -> shapes::Call<shapes::Numbered>`: accepts [`u32`], &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it. The number of the page; the first is 1.
    ///
    /// `.send().await` returns [`shapes::Result`](crate::shapes::Result)<[`Vec`]<[`Item`](crate::Item)>>.
    ///
    /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Numbered {}

    /// Pages of items whose page gives the address of the next one.
    ///
    /// Request: `GET https://shapes.example.com/addressed`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Addressed>`. Create it with `Shapes::addressed`.
    ///
    /// `.send().await` returns [`shapes::Result`](crate::shapes::Result)<[`Batch`](crate::Batch)>.
    ///
    /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Addressed {}


    /// Remove an item. An accepted removal is not done; a missing item is removed.
    ///
    /// Request: `DELETE https://shapes.example.com/items/{id}`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Remove>`. Create it with `Shapes::remove`.
    ///
    /// Inputs:
    ///
    /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
    ///
    /// Status: `202`.
    ///
    /// Status: `404`.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Remove {}

    /// An item, or the conflict that holds it. An accepted change is a conflict too.
    ///
    /// Request: `GET https://shapes.example.com/settled/{id}`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Settled>`. Create it with `Shapes::settled`.
    ///
    /// Inputs:
    ///
    /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
    ///
    /// Status: `202`.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Result`]<[`Item`](crate::Item), [`Conflict`](crate::Conflict)>>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Settled {}

    /// A part of a file, as it comes, or none when there is nothing to send.
    ///
    /// Request: `GET https://shapes.example.com/parts/{id}`.
    ///
    /// Call type: `tour::shapes::Call<tour::shapes::Part>`. Create it with `Shapes::part`.
    ///
    /// Inputs:
    ///
    /// - `id`: [`u64`] or &[`u64`]. The identifier of the part.
    ///
    /// Status: `204`.
    ///
    /// Status: `404`.
    ///
    /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Option`]<[`concord::ByteStream`]>>.
    ///
    /// See [`Call`] for methods shared by every call. This endpoint type has no values.
    #[allow(missing_debug_implementations)]
    pub enum Part {}

    /// Calls on [`shapes::Wrapped`](crate::shapes::Wrapped).
    pub mod wrapped {
        /// An item, out of its envelope.
        ///
        /// Request: `GET https://shapes.example.com/wrapped/{id}`.
        ///
        /// Call type: `tour::shapes::Call<tour::shapes::wrapped::Get>`. Create it with `Wrapped::get`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// Status: `2xx`.
        ///
        /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
        ///
        /// See [`Call`](crate::shapes::Call) for methods shared by every call. This endpoint type has no values.
        #[allow(missing_debug_implementations)]
        pub enum Get {}

        /// The envelope itself.
        ///
        /// Request: `GET https://shapes.example.com/wrapped/raw/{id}`.
        ///
        /// Call type: `tour::shapes::Call<tour::shapes::wrapped::Raw>`. Create it with `Wrapped::raw`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Envelope`](crate::Envelope)<[`Item`](crate::Item)>>.
        ///
        /// See [`Call`](crate::shapes::Call) for methods shared by every call. This endpoint type has no values.
        #[allow(missing_debug_implementations)]
        pub enum Raw {}

        /// An item out of its envelope, or none.
        ///
        /// Request: `GET https://shapes.example.com/wrapped/find/{id}`.
        ///
        /// Call type: `tour::shapes::Call<tour::shapes::wrapped::Find>`. Create it with `Wrapped::find`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// Status: `2xx`.
        ///
        /// Status: `404`.
        ///
        /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`Option`]<[`Item`](crate::Item)>>.
        ///
        /// See [`Call`](crate::shapes::Call) for methods shared by every call. This endpoint type has no values.
        #[allow(missing_debug_implementations)]
        pub enum Find {}

        /// Touch an item. An accepted touch is not done; a missing item is fine.
        ///
        /// Request: `HEAD https://shapes.example.com/wrapped/touch/{id}`.
        ///
        /// Call type: `tour::shapes::Call<tour::shapes::wrapped::Touch>`. Create it with `Wrapped::touch`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// Status: `202`.
        ///
        /// Status: `404`.
        ///
        /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
        ///
        /// See [`Call`](crate::shapes::Call) for methods shared by every call. This endpoint type has no values.
        #[allow(missing_debug_implementations)]
        pub enum Touch {}

        /// Whether an item is held. Only a `204` says yes.
        ///
        /// Request: `HEAD https://shapes.example.com/wrapped/holds/{id}`.
        ///
        /// Call type: `tour::shapes::Call<tour::shapes::wrapped::Holds>`. Create it with `Wrapped::holds`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the item.
        ///
        /// Status: `204`.
        ///
        /// Status: `404`.
        ///
        /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`bool`]>.
        ///
        /// See [`Call`](crate::shapes::Call) for methods shared by every call. This endpoint type has no values.
        #[allow(missing_debug_implementations)]
        pub enum Holds {}
    }

    /// Calls on [`shapes::Sealed`](crate::shapes::Sealed).
    pub mod sealed {
        /// A file, as it comes, once the seal of its answer is checked.
        ///
        /// Request: `GET https://shapes.example.com/sealed/{id}`.
        ///
        /// Call type: `tour::shapes::Call<tour::shapes::sealed::File>`. Create it with `Sealed::file`.
        ///
        /// Inputs:
        ///
        /// - `id`: [`u64`] or &[`u64`]. The identifier of the file.
        ///
        /// Status: `2xx`.
        ///
        /// `.await` returns [`shapes::Result`](crate::shapes::Result)<[`concord::ByteStream`]>.
        ///
        /// See [`Call`](crate::shapes::Call) for methods shared by every call. This endpoint type has no values.
        #[allow(missing_debug_implementations)]
        pub enum File {}
    }
}
