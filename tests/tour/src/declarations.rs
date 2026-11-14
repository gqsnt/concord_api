// The tree declarations. Implementations remain beside each authored description.

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
    /// `refused: status 429, in tasks.list, after 3 tries`.
    pub type Error = ::concord::Error;

    /// The result of a [`Tasks`](crate::Tasks) call, with [`Error`].
    pub type Result<T> = ::std::result::Result<T, Error>;

    /// The retry policy of [`Tasks`](crate::Tasks): a [`concord::Retry`]. The API has no problem type: a rule's `tried.problem()` gives nothing.
    ///
    /// Give it to `with_retry` on the client or on a call: `tasks::Retry::standard()`.
    pub type Retry = ::concord::Retry;

    /// The call `tasks.list`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`tasks::Result`](crate::tasks::Result)<[`Vec`]<[`Task`](crate::Task)>>.
    ///
    /// Create it with [`Tasks::list`](crate::Tasks::list).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct List {
        pub(super) __client: super::Tasks,
        pub(super) done: <() as __Types>::__Input0,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `tasks.create`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`tasks::Result`](crate::tasks::Result)<[`Task`](crate::Task)>.
    ///
    /// Create it with [`Tasks::create`](crate::Tasks::create).
    ///
    /// ```compile_fail
    /// let _ = tour::Tasks::new().create(tour::NewTask { title: String::new() }).send_if_none_match("etag");
    /// ```
    ///
    /// ```compile_fail
    /// let _ = tour::tasks::Create::__prepare_if_none_match;
    /// ```
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Create {
        pub(super) __client: super::Tasks,
        pub(super) task: <() as __Types>::__Input1,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `tasks.get`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`tasks::Result`](crate::tasks::Result)<[`Task`](crate::Task)>.
    ///
    /// Create it with [`Tasks::get`](crate::Tasks::get).
    ///
    /// ```compile_fail
    /// let _ = tour::Tasks::new().get(1).send_if_none_match(String::from("etag"));
    /// ```
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Get {
        pub(super) __client: super::Tasks,
        pub(super) id: <() as __Types>::__Input2,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `tasks.delete`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`tasks::Result`](crate::tasks::Result)<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
    ///
    /// Create it with [`Tasks::delete`](crate::Tasks::delete).
    ///
    /// ```compile_fail
    /// let _ = tour::tasks::Delete::send_if_none_match;
    /// ```
    ///
    /// ```compile_fail
    /// let _ = tour::tasks::Delete::__prepare_if_none_match;
    /// ```
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Delete {
        pub(super) __client: super::Tasks,
        pub(super) id: <() as __Types>::__Input3,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }
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
    /// `refused: status 429, in riot.matches.ids, after 3 tries`.
    pub type Error = ::concord::Error<<() as __Types>::__Problem>;

    /// The result of a [`Riot`](crate::Riot) call, with [`Error`].
    pub type Result<T> = ::std::result::Result<T, Error>;

    /// The retry policy of [`Riot`](crate::Riot): a [`concord::Retry`] whose rules read the API problem, as [`Error`] does.
    ///
    /// Give it to `with_retry` on the client or on a call: `riot::Retry::standard()`.
    pub type Retry = ::concord::Retry<<() as __Types>::__Problem>;

    /// Calls on [`riot::Matches`](crate::riot::Matches).
    pub mod matches {
        /// The call `riot.matches.ids`. Nothing is sent until a page is asked for.
        ///
        /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
        ///
        /// Create it with [`riot::Matches::ids`](crate::riot::Matches::ids).
        ///
        /// ```compile_fail
        /// # async fn c8_8_a_paged_call_cannot_be_awaited() {
        /// let _ = tour::Riot::new("key").matches(tour::Europe).ids(tour::Puuid::from("p")).await;
        /// # }
        /// ```
        #[must_use = ".send() gives one page; .pages() gives them all"]
        #[derive(Clone)]
        pub struct Ids {
            pub(in super::super) __scope: super::Matches,
            pub(in super::super) puuid: <() as super::__Types>::__Input6,
            pub(in super::super) start: <() as super::__Types>::__Input7,
            pub(in super::super) count: <() as super::__Types>::__Input8,
            pub(in super::super) queue: <() as super::__Types>::__Input9,
            pub(in super::super) start_time: <() as super::__Types>::__Input10,
            pub(in super::super) __options:
                ::concord::__private::Options<<() as super::__Types>::__Problem>,
        }

        /// The call `riot.matches.get`. Nothing is sent until it is awaited.
        ///
        /// `.await` gives [`riot::Result`](crate::riot::Result)<[`Match`](crate::Match)>.
        ///
        /// Create it with [`riot::Matches::get`](crate::riot::Matches::get).
        #[must_use = "a call does nothing until sent or awaited"]
        #[derive(Clone)]
        pub struct Get {
            pub(in super::super) __scope: super::Matches,
            pub(in super::super) id: <() as super::__Types>::__Input11,
            pub(in super::super) __options:
                ::concord::__private::Options<<() as super::__Types>::__Problem>,
        }
    }

    /// Calls on [`riot::Account`](crate::riot::Account).
    pub mod account {
        /// The call `riot.account.by_riot_id`. Nothing is sent until it is awaited.
        ///
        /// `.await` gives [`riot::Result`](crate::riot::Result)<[`Option`]<[`tour::Account`](crate::Account)>>.
        ///
        /// Create it with [`riot::Account::by_riot_id`](crate::riot::Account::by_riot_id).
        #[must_use = "a call does nothing until sent or awaited"]
        #[derive(Clone)]
        pub struct ByRiotId {
            pub(in super::super) __scope: super::Account,
            pub(in super::super) game_name: <() as super::__Types>::__Input2,
            pub(in super::super) tag_line: <() as super::__Types>::__Input3,
            pub(in super::super) __options:
                ::concord::__private::Options<<() as super::__Types>::__Problem>,
        }

        /// The call `riot.account.me`. Nothing is sent until it is awaited.
        ///
        /// `.await` gives [`riot::Result`](crate::riot::Result)<[`tour::Account`](crate::Account)>.
        ///
        /// Create it with [`riot::Account::me`](crate::riot::Account::me).
        #[must_use = "a call does nothing until sent or awaited"]
        #[derive(Clone)]
        pub struct Me {
            pub(in super::super) __scope: super::Account,
            pub(in super::super) access_token: <() as super::__Types>::__Input4,
            pub(in super::super) __options:
                ::concord::__private::Options<<() as super::__Types>::__Problem>,
        }
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

    /// Exchange client credentials using a form body.
    /// Create it with [`Files::grant`](crate::Files::grant).
    ///
    /// ```compile_fail
    /// let _ = tour::files::Grant::send_if_none_match;
    /// ```
    ///
    /// ```compile_fail
    /// let _ = tour::files::Grant::__prepare_if_none_match;
    /// ```
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Grant {
        pub(super) __client: super::Files,
        pub(super) grant_type: <() as __Types>::__Input8,
        pub(super) client_id: <() as __Types>::__Input9,
        pub(super) client_secret: <() as __Types>::__Input10,
        pub(super) __options: ::concord::__private::Options<()>,
    }

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
    /// `refused: status 429, in files.download, after 3 tries`.
    pub type Error = ::concord::Error;

    /// The result of a [`Files`](crate::Files) call, with [`Error`].
    pub type Result<T> = ::std::result::Result<T, Error>;

    /// The retry policy of [`Files`](crate::Files): a [`concord::Retry`]. The API has no problem type: a rule's `tried.problem()` gives nothing.
    ///
    /// Give it to `with_retry` on the client or on a call: `files::Retry::standard()`.
    pub type Retry = ::concord::Retry;

    /// The call `files.exists`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`files::Result`](crate::files::Result)<[`bool`]>.
    ///
    /// Create it with [`Files::exists`](crate::Files::exists).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Exists {
        pub(super) __client: super::Files,
        pub(super) id: <() as __Types>::__Input1,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `files.readme`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`files::Result`](crate::files::Result)<[`String`]>.
    ///
    /// Create it with [`Files::readme`](crate::Files::readme).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Readme {
        pub(super) __client: super::Files,
        pub(super) repo: <() as __Types>::__Input2,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `files.download`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`files::Result`](crate::files::Result)<[`concord::ByteStream`]>.
    ///
    /// Create it with [`Files::download`](crate::Files::download).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Download {
        pub(super) __client: super::Files,
        pub(super) id: <() as __Types>::__Input3,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `files.upload`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`files::Result`](crate::files::Result)<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
    ///
    /// Create it with [`Files::upload`](crate::Files::upload).
    ///
    /// ```compile_fail
    /// let _ = tour::Files::new("key").upload("a", concord::Body::new("", "text/plain")).send_if_none_match("etag");
    /// ```
    ///
    /// ```compile_fail
    /// let _ = tour::files::Upload::__prepare_if_none_match;
    /// ```
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Upload {
        pub(super) __client: super::Files,
        pub(super) name: <() as __Types>::__Input4,
        pub(super) file: <() as __Types>::__Input5,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `files.search`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`files::Result`](crate::files::Result)<[`Page`](crate::Page)>.
    ///
    /// Create it with [`Files::search`](crate::Files::search).
    ///
    /// ```compile_fail
    /// let _ = tour::files::Search::send_if_none_match;
    /// ```
    ///
    /// ```compile_fail
    /// let _ = tour::files::Search::__prepare_if_none_match;
    /// ```
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Search {
        pub(super) __client: super::Files,
        pub(super) text: <() as __Types>::__Input6,
        pub(super) cursor: <() as __Types>::__Input7,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }
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
    /// `refused: status 429, in feed.list, after 3 tries`.
    pub type Error = ::concord::Error;

    /// The result of a [`Feed`](crate::Feed) call, with [`Error`].
    pub type Result<T> = ::std::result::Result<T, Error>;

    /// The retry policy of [`Feed`](crate::Feed): a [`concord::Retry`]. The API has no problem type: a rule's `tried.problem()` gives nothing.
    ///
    /// Give it to `with_retry` on the client or on a call: `feed::Retry::standard()`.
    pub type Retry = ::concord::Retry;

    /// The call `feed.list`. Nothing is sent until a page is asked for.
    ///
    /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
    ///
    /// Create it with [`Feed::list`](crate::Feed::list).
    ///
    /// ```compile_fail
    /// # async fn c8_8_a_paged_call_cannot_be_awaited() {
    /// let _ = tour::Feed::new().list().await;
    /// # }
    /// ```
    #[must_use = ".send() gives one page; .pages() gives them all"]
    #[derive(Clone)]
    pub struct List {
        pub(super) __client: super::Feed,
        pub(super) starting_after: <() as __Types>::__Input0,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `feed.by_cursor`. Nothing is sent until a page is asked for.
    ///
    /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
    ///
    /// Create it with [`Feed::by_cursor`](crate::Feed::by_cursor).
    ///
    /// ```compile_fail
    /// # async fn c8_8_a_paged_call_cannot_be_awaited() {
    /// let _ = tour::Feed::new().by_cursor().await;
    /// # }
    /// ```
    #[must_use = ".send() gives one page; .pages() gives them all"]
    #[derive(Clone)]
    pub struct ByCursor {
        pub(super) __client: super::Feed,
        pub(super) starting_after: <() as __Types>::__Input1,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `feed.linked`. Nothing is sent until a page is asked for.
    ///
    /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
    ///
    /// Create it with [`Feed::linked`](crate::Feed::linked).
    ///
    /// ```compile_fail
    /// # async fn c8_8_a_paged_call_cannot_be_awaited() {
    /// let _ = tour::Feed::new().linked().await;
    /// # }
    /// ```
    #[must_use = ".send() gives one page; .pages() gives them all"]
    #[derive(Clone)]
    pub struct Linked {
        pub(super) __client: super::Feed,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }
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
    /// `refused: status 429, in shapes.again, after 3 tries`.
    pub type Error = ::concord::Error<<() as __Types>::__Problem>;

    /// The result of a [`Shapes`](crate::Shapes) call, with [`Error`].
    pub type Result<T> = ::std::result::Result<T, Error>;

    /// The retry policy of [`Shapes`](crate::Shapes): a [`concord::Retry`] whose rules read the API problem, as [`Error`] does.
    ///
    /// Give it to `with_retry` on the client or on a call: `shapes::Retry::standard()`.
    pub type Retry = ::concord::Retry<<() as __Types>::__Problem>;

    /// The call `shapes.again`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// Create it with [`Shapes::again`](crate::Shapes::again).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Again {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input0,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.replace`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// Create it with [`Shapes::replace`](crate::Shapes::replace).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Replace {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input1,
        pub(super) item: <() as __Types>::__Input2,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.order`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// Create it with [`Shapes::order`](crate::Shapes::order).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Order {
        pub(super) __client: super::Shapes,
        pub(super) trace: <() as __Types>::__Input3,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.upload`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// Create it with [`Shapes::upload`](crate::Shapes::upload).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Upload {
        pub(super) __client: super::Shapes,
        pub(super) body: <() as __Types>::__Input4,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.outcome`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Result`](::std::result::Result)<[`Item`](crate::Item), [`Conflict`](crate::Conflict)>>.
    ///
    /// Create it with [`Shapes::outcome`](crate::Shapes::outcome).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Outcome {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input5,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.ping`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
    ///
    /// Create it with [`Shapes::ping`](crate::Shapes::ping).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Ping {
        pub(super) __client: super::Shapes,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.present`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`bool`]>.
    ///
    /// Create it with [`Shapes::present`](crate::Shapes::present).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Present {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input6,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.create`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Receipt`](crate::Receipt)>.
    ///
    /// Create it with [`Shapes::create`](crate::Shapes::create).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Create {
        pub(super) __client: super::Shapes,
        pub(super) item: <() as __Types>::__Input7,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.hosted`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// Create it with [`Shapes::hosted`](crate::Shapes::hosted).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Hosted {
        pub(super) __client: super::Shapes,
        pub(super) tenant: <() as __Types>::__Input8,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.search`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Vec`]<[`Item`](crate::Item)>>.
    ///
    /// Create it with [`Shapes::search`](crate::Shapes::search).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Search {
        pub(super) __client: super::Shapes,
        pub(super) ratio: <() as __Types>::__Input9,
        pub(super) filter: <() as __Types>::__Input10,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.measured`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// Create it with [`Shapes::measured`](crate::Shapes::measured).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Measured {
        pub(super) __client: super::Shapes,
        pub(super) item: <() as __Types>::__Input11,
        pub(super) length: <() as __Types>::__Input12,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.parsed`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// Create it with [`Shapes::parsed`](crate::Shapes::parsed).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Parsed {
        pub(super) __client: super::Shapes,
        pub(super) raw: <() as __Types>::__Input13,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.counts`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Tally`](crate::Tally)>.
    ///
    /// Create it with [`Shapes::counts`](crate::Shapes::counts).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Counts {
        pub(super) __client: super::Shapes,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.strict`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// Create it with [`Shapes::strict`](crate::Shapes::strict).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Strict {
        pub(super) __client: super::Shapes,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.long_message`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// Create it with [`Shapes::long_message`](crate::Shapes::long_message).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct LongMessage {
        pub(super) __client: super::Shapes,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.noted`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// Create it with [`Shapes::noted`](crate::Shapes::noted).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Noted {
        pub(super) __client: super::Shapes,
        pub(super) trace: <() as __Types>::__Input19,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.titled`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// Create it with [`Shapes::titled`](crate::Shapes::titled).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Titled {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input20,
        pub(super) title: <() as __Types>::__Input21,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.held`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Result`](::std::result::Result)<[`Item`](crate::Item), [`Conflict`](crate::Conflict)>>.
    ///
    /// Create it with [`Shapes::held`](crate::Shapes::held).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Held {
        pub(super) __client: super::Shapes,
        pub(super) body: <() as __Types>::__Input22,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

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

    /// The call `shapes.refusal`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
    ///
    /// Create it with [`Shapes::refusal`](crate::Shapes::refusal).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Refusal {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input23,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.guarded`. Nothing is sent until a page is asked for.
    ///
    /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
    ///
    /// Create it with [`Shapes::guarded`](crate::Shapes::guarded).
    #[must_use = ".send() gives one page; .pages() gives them all"]
    #[derive(Clone)]
    pub struct Guarded {
        pub(super) __client: super::Shapes,
        pub(super) path_key: <() as __Types>::__Input24,
        pub(super) query_key: <() as __Types>::__Input25,
        pub(super) header_key: <() as __Types>::__Input26,
        pub(super) cursor: <() as __Types>::__Input27,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.headed`. Nothing is sent until a page is asked for.
    ///
    /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
    ///
    /// Create it with [`Shapes::headed`](crate::Shapes::headed).
    #[must_use = ".send() gives one page; .pages() gives them all"]
    #[derive(Clone)]
    pub struct Headed {
        pub(super) __client: super::Shapes,
        pub(super) cursor: <() as __Types>::__Input28,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.posted`. Nothing is sent until a page is asked for.
    ///
    /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
    ///
    /// Create it with [`Shapes::posted`](crate::Shapes::posted).
    #[must_use = ".send() gives one page; .pages() gives them all"]
    #[derive(Clone)]
    pub struct Posted {
        pub(super) __client: super::Shapes,
        pub(super) token: <() as __Types>::__Input29,
        pub(super) cursor: <() as __Types>::__Input30,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.streamed`. Nothing is sent until a page is asked for.
    ///
    /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
    ///
    /// Create it with [`Shapes::streamed`](crate::Shapes::streamed).
    #[must_use = ".send() gives one page; .pages() gives them all"]
    #[derive(Clone)]
    pub struct Streamed {
        pub(super) __client: super::Shapes,
        pub(super) file: <() as __Types>::__Input31,
        pub(super) cursor: <() as __Types>::__Input32,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.renewed`. Nothing is sent until a page is asked for.
    ///
    /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
    ///
    /// Create it with [`Shapes::renewed`](crate::Shapes::renewed).
    #[must_use = ".send() gives one page; .pages() gives them all"]
    #[derive(Clone)]
    pub struct Renewed {
        pub(super) __client: super::Shapes,
        pub(super) cursor: <() as __Types>::__Input33,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.keyed_host`. Nothing is sent until a page is asked for.
    ///
    /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
    ///
    /// Create it with [`Shapes::keyed_host`](crate::Shapes::keyed_host).
    #[must_use = ".send() gives one page; .pages() gives them all"]
    #[derive(Clone)]
    pub struct KeyedHost {
        pub(super) __client: super::Shapes,
        pub(super) host_key: <() as __Types>::__Input34,
        pub(super) cursor: <() as __Types>::__Input35,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.roaming`. Nothing is sent until a page is asked for.
    ///
    /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
    ///
    /// Create it with [`Shapes::roaming`](crate::Shapes::roaming).
    #[must_use = ".send() gives one page; .pages() gives them all"]
    #[derive(Clone)]
    pub struct Roaming {
        pub(super) __client: super::Shapes,
        pub(super) tenant: <() as __Types>::__Input36,
        pub(super) cursor: <() as __Types>::__Input37,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.offset`. Nothing is sent until a page is asked for.
    ///
    /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
    ///
    /// Create it with [`Shapes::offset`](crate::Shapes::offset).
    #[must_use = ".send() gives one page; .pages() gives them all"]
    #[derive(Clone)]
    pub struct Offset {
        pub(super) __client: super::Shapes,
        pub(super) start: <() as __Types>::__Input38,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.numbered`. Nothing is sent until a page is asked for.
    ///
    /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
    ///
    /// Create it with [`Shapes::numbered`](crate::Shapes::numbered).
    #[must_use = ".send() gives one page; .pages() gives them all"]
    #[derive(Clone)]
    pub struct Numbered {
        pub(super) __client: super::Shapes,
        pub(super) page: <() as __Types>::__Input39,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.addressed`. Nothing is sent until a page is asked for.
    ///
    /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
    ///
    /// Create it with [`Shapes::addressed`](crate::Shapes::addressed).
    #[must_use = ".send() gives one page; .pages() gives them all"]
    #[derive(Clone)]
    pub struct Addressed {
        pub(super) __client: super::Shapes,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }


    /// The call `shapes.remove`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
    ///
    /// Create it with [`Shapes::remove`](crate::Shapes::remove).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Remove {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input40,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.settled`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Result`](::std::result::Result)<[`Item`](crate::Item), [`Conflict`](crate::Conflict)>>.
    ///
    /// Create it with [`Shapes::settled`](crate::Shapes::settled).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Settled {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input41,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// The call `shapes.part`. Nothing is sent until it is awaited.
    ///
    /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Option`]<[`concord::ByteStream`]>>.
    ///
    /// Create it with [`Shapes::part`](crate::Shapes::part).
    #[must_use = "a call does nothing until sent or awaited"]
    #[derive(Clone)]
    pub struct Part {
        pub(super) __client: super::Shapes,
        pub(super) id: <() as __Types>::__Input42,
        pub(super) __options: ::concord::__private::Options<<() as __Types>::__Problem>,
    }

    /// Calls on [`shapes::Wrapped`](crate::shapes::Wrapped).
    pub mod wrapped {
        /// The call `shapes.wrapped.get`. Nothing is sent until it is awaited.
        ///
        /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Item`](crate::Item)>.
        ///
        /// Create it with [`shapes::Wrapped::get`](crate::shapes::Wrapped::get).
        #[must_use = "a call does nothing until sent or awaited"]
        #[derive(Clone)]
        pub struct Get {
            pub(in super::super) __scope: super::Wrapped,
            pub(in super::super) id: <() as super::__Types>::__Input14,
            pub(in super::super) __options:
                ::concord::__private::Options<<() as super::__Types>::__Problem>,
        }

        /// The call `shapes.wrapped.raw`. Nothing is sent until it is awaited.
        ///
        /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Envelope`](crate::Envelope)<[`Item`](crate::Item)>>.
        ///
        /// Create it with [`shapes::Wrapped::raw`](crate::shapes::Wrapped::raw).
        #[must_use = "a call does nothing until sent or awaited"]
        #[derive(Clone)]
        pub struct Raw {
            pub(in super::super) __scope: super::Wrapped,
            pub(in super::super) id: <() as super::__Types>::__Input15,
            pub(in super::super) __options:
                ::concord::__private::Options<<() as super::__Types>::__Problem>,
        }

        /// The call `shapes.wrapped.find`. Nothing is sent until it is awaited.
        ///
        /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`Option`]<[`Item`](crate::Item)>>.
        ///
        /// Create it with [`shapes::Wrapped::find`](crate::shapes::Wrapped::find).
        #[must_use = "a call does nothing until sent or awaited"]
        #[derive(Clone)]
        pub struct Find {
            pub(in super::super) __scope: super::Wrapped,
            pub(in super::super) id: <() as super::__Types>::__Input16,
            pub(in super::super) __options:
                ::concord::__private::Options<<() as super::__Types>::__Problem>,
        }

        /// The call `shapes.wrapped.touch`. Nothing is sent until it is awaited.
        ///
        /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`()`](https://doc.rust-lang.org/std/primitive.unit.html)>.
        ///
        /// Create it with [`shapes::Wrapped::touch`](crate::shapes::Wrapped::touch).
        #[must_use = "a call does nothing until sent or awaited"]
        #[derive(Clone)]
        pub struct Touch {
            pub(in super::super) __scope: super::Wrapped,
            pub(in super::super) id: <() as super::__Types>::__Input17,
            pub(in super::super) __options:
                ::concord::__private::Options<<() as super::__Types>::__Problem>,
        }

        /// The call `shapes.wrapped.holds`. Nothing is sent until it is awaited.
        ///
        /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`bool`]>.
        ///
        /// Create it with [`shapes::Wrapped::holds`](crate::shapes::Wrapped::holds).
        #[must_use = "a call does nothing until sent or awaited"]
        #[derive(Clone)]
        pub struct Holds {
            pub(in super::super) __scope: super::Wrapped,
            pub(in super::super) id: <() as super::__Types>::__Input18,
            pub(in super::super) __options:
                ::concord::__private::Options<<() as super::__Types>::__Problem>,
        }
    }

    /// Calls on [`shapes::Sealed`](crate::shapes::Sealed).
    pub mod sealed {
        /// The call `shapes.sealed.file`. Nothing is sent until it is awaited.
        ///
        /// `.await` gives [`shapes::Result`](crate::shapes::Result)<[`concord::ByteStream`]>.
        ///
        /// Create it with [`shapes::Sealed::file`](crate::shapes::Sealed::file).
        #[must_use = "a call does nothing until sent or awaited"]
        #[derive(Clone)]
        pub struct File {
            pub(in super::super) __scope: super::Sealed,
            pub(in super::super) id: <() as super::__Types>::__Input43,
            pub(in super::super) __options:
                ::concord::__private::Options<<() as super::__Types>::__Problem>,
        }
    }
}
