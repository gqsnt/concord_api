//! What every pager can be combined with: an end that the page states, a page size that the
//! API applies, a change to other inputs, a second pager for the other direction, and a list
//! that is paged only when the caller asks.

use concord_core::error::Fail;
use concord_core::page::{Call, IntoItems, IntoNext, Slot};
use concord_core::Answer;

mod sealed {
    use concord_core::page::{Call, IntoNext};
    use concord_core::Answer;

    /// Implemented for every pager, and for nothing else.
    pub trait Sealed<C> {}

    impl<C, P, N> Sealed<C> for P
    where
        C: Call,
        P: Fn(C, &Answer<C::Value>) -> N,
        N: IntoNext<C>,
    {
    }
}

/// A pager, with what it can be combined with. Every pager implements it: the six shipped
/// pagers, the pagers that these methods return, and a function of your own with the signature
/// of a pager (`Fn(C, &Answer<C::Value>) -> N`, where `N` is `Option<C>` or
/// `Result<Option<C>, Fail<C::Problem>>`). It cannot be implemented for anything else.
///
/// Each method returns a new pager that asks the pager it was called on, and decides something
/// before or after it. The methods combine in any order: each one wraps what is written before
/// it, so the last one written decides first. None of them is known to the macro: bring the trait
/// into scope beside the description, with `use concord::page::Pager;`.
///
/// A closure passed to these methods reads the page without naming its type, because the input
/// that the pager moves (its slot) tells Rust the call. `by_link()` and `by_next_url(…)` move no
/// input, so there the type is written: `by_link().more(|page: &Batch| page.has_more)`. A
/// closure that uses an input of the endpoint (a slot) is a `move` closure.
///
/// | Written | The next call | Ends when | Fails when |
/// |---|---|---|---|
/// | `.more(\|page\| page.has_more)` | The one of the pager. | The function returns `false`. | It returns `true`, and the pager finds no next call. |
/// | `.total(start, \|page\| page.total)` | The one of the pager. | `start` plus the items of the page reaches the total. | It does not, and the pager finds no next call. |
/// | `.page_size(count, 20)` | The one of the pager. | The page holds fewer items than `count`, or than 20 when `count` was not set; or when the pager ends. | When the pager fails. |
/// | `.also(\|call, page\| …)` | The one of the pager, changed by the function. | When the pager ends. | When the pager fails. |
/// | `.when_set(before, other)` | The one of `other` when the call set `before`; else the one of the pager. | When the pager that decides ends. | When the pager that decides fails. |
/// | `.if_set(page)` | The one of the pager. | The call did not set `page`; or when the pager ends. | When the pager fails. |
///
/// **A page that says there is more is never the end.** With `more` and `total`, a page that
/// states that more items follow, and gives nothing to continue with, ends the pages with a
/// Mismatch (`the page says that more items follow, and gives no next page`) instead of
/// looking complete. A cursor that comes back unchanged needs no check of its own: the request
/// built from it is the request that was just sent, and [`Pages`](concord_core::Pages) already
/// ends with a Mismatch before sending it again.
///
/// **What the caller receives** is what [`Pages`](concord_core::Pages) does for every pager.
/// The page that arrived is returned first; the decision about it comes with the following
/// `try_next()`:
///
/// | The pager decided | `.pages()` | `.pages().items()` |
/// |---|---|---|
/// | A next call | `Ok(Some(page))`, then the next page | Every item of the page, then the items of the next one |
/// | The end | `Ok(Some(page))`, then `Ok(None)` | Every item of the page, then `Ok(None)` |
/// | A Mismatch (`more`, `total`, an offset that cannot grow) | `Ok(Some(page))`, then `Err(mismatch)`, then `Ok(None)` | Every item of the page, then `Err(mismatch)`, then `Ok(None)` |
/// | `Fail::from(problem)` in a pager of your own | `Ok(Some(page))`, then the Refused error, then `Ok(None)` | Every item of the page, then the error, then `Ok(None)` |
/// | A next call that repeats the request just sent, or goes to another origin | `Ok(Some(page))`, then the Mismatch or the Build error, then `Ok(None)` | The same, after every item of the page |
///
/// A program that writes descriptions from an OpenAPI document needs no logic of its own: each
/// argument is a field of the page, read with a closure (`|page| page.has_more`), or an input of
/// the endpoint, named as it is.
///
/// ```
/// use concord::page::{IntoItems, Pager};
/// use concord::{by_cursor, Json};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// pub struct List<T> { pub data: Vec<T>, pub has_more: bool }
/// #[derive(Deserialize)]
/// pub struct Customer { pub id: String }
///
/// impl<T: Send> IntoItems for List<T> {
///     type Item = T;
///     fn into_items(self) -> Vec<T> { self.data }
///     fn count(&self) -> usize { self.data.len() }
/// }
///
/// concord::api! {
///     pub client Shop {
///         base "https://shop.example.test";
///         read Json;
///         /// Forward after the last customer; back before the first, when the caller went back.
///         GET customers(limit?: u32, starting_after?: String, ending_before?: String) -> List<Customer> {
///             path "v1/customers";
///             query { limit; starting_after; ending_before; }
///             paginate by_cursor(starting_after, |page| page.data.last().map(|c| c.id.clone()))
///                 .when_set(ending_before, by_cursor(ending_before, |page| page.data.first().map(|c| c.id.clone())))
///                 .more(|page| page.has_more);
///         }
///     }
/// }
///
/// async fn every_id(shop: &Shop) -> Result<Vec<String>, concord::Error> {
///     let mut ids = Vec::new();
///     let mut customers = shop.customers().limit(100).pages().items();
///     while let Some(customer) = customers.try_next().await? {
///         ids.push(customer.id);
///     }
///     Ok(ids)
/// }
/// ```
pub trait Pager<C: Call>: sealed::Sealed<C> + Sized + Send + 'static {
    /// `paginate by_cursor(after, |page| page.next.clone()).more(|page| page.has_more);`: the page
    /// states whether more items follow. When `more` returns `false`, this page is the last, and
    /// the pager is not asked. When it returns `true`, the pager must find the next call: if it
    /// finds none, the pages end with a Mismatch, after this page. An error of the pager is
    /// returned as it is. `more` returns a `bool` or an `Option<bool>`: `None` says that this page
    /// states nothing, and the pager decides as if `more` were not there.
    ///
    /// For a flag (`has_more`, `is_truncated`, `hasNextPage`), and for a page number that the
    /// page states with the number of pages: `.more(|page| page.page < page.pages)`.
    ///
    /// ```
    /// use concord::page::{IntoItems, Pager};
    /// use concord::{by_cursor, Json};
    /// use serde::Deserialize;
    ///
    /// /// S3's `ListObjectsV2`: `IsTruncated`, and the token of the next page.
    /// #[derive(Deserialize)]
    /// #[serde(rename_all = "PascalCase")]
    /// pub struct Objects {
    ///     pub is_truncated: Option<bool>,
    ///     pub next_continuation_token: Option<String>,
    ///     #[serde(default)]
    ///     pub contents: Vec<Object>,
    /// }
    /// #[derive(Deserialize)]
    /// #[serde(rename_all = "PascalCase")]
    /// pub struct Object { pub key: String }
    ///
    /// concord::api! {
    ///     pub client Bucket {
    ///         base "https://bucket.example.test";
    ///         read Json;
    ///         GET objects(continuation_token?: String) -> Objects {
    ///             query { "list-type" = "2"; "continuation-token" = continuation_token; }
    ///             paginate by_cursor(continuation_token, |page| page.next_continuation_token.clone())
    ///                 .more(|page| page.is_truncated == Some(true));
    ///         }
    ///     }
    /// }
    ///
    /// async fn keys(bucket: &Bucket) -> Result<Vec<String>, concord::Error> {
    ///     let mut keys = Vec::new();
    ///     let mut pages = bucket.objects().pages();
    ///     while let Some(page) = pages.try_next().await? {
    ///         keys.extend(page.contents.into_iter().map(|object| object.key));
    ///     }
    ///     Ok(keys)
    /// }
    /// ```
    // Keep the public pager signature's explicit call, answer and problem types.
    // PANIC-SAFETY: The returned closure keeps the call, answer and problem types without boxing; no panic lint is relaxed.
    #[allow(clippy::type_complexity)]
    fn more<F, M>(
        self,
        more: F,
    ) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
    where
        F: Fn(&C::Value) -> M + Send + 'static,
        M: Into<Option<bool>>;

    /// `paginate by_offset(start).total(start, |page| page.total);`: the page states how many
    /// items the list holds. `n` is the number of items of the page ([`IntoItems::count`]) and
    /// `at` the `start` of the call that fetched it (0 when it was not set). When `at + n`
    /// reaches the total, this page is the last, and the pager is not asked. When it does not,
    /// the pager must find the next call, as with [`more`](Pager::more): an empty page before
    /// the total is a Mismatch, and so is a total that is not a number of items (a negative one)
    /// and an `at + n` that cannot be counted.
    ///
    /// `start` is the offset input, the same that the pager moves.
    ///
    /// ```
    /// use concord::page::{IntoItems, Pager};
    /// use concord::{by_offset, Json};
    /// use serde::Deserialize;
    ///
    /// /// A leaderboard that states the number of players.
    /// #[derive(Deserialize)]
    /// #[serde(rename_all = "camelCase")]
    /// pub struct Leaderboard { pub total_players: u32, pub players: Vec<Player> }
    /// #[derive(Deserialize)]
    /// pub struct Player { pub name: String }
    ///
    /// impl IntoItems for Leaderboard {
    ///     type Item = Player;
    ///     fn into_items(self) -> Vec<Player> { self.players }
    ///     fn count(&self) -> usize { self.players.len() }
    /// }
    ///
    /// concord::api! {
    ///     pub client Ladder {
    ///         base "https://ladder.example.test";
    ///         read Json;
    ///         GET leaderboard(size?: u32, start_index?: u32) -> Leaderboard {
    ///             query { size; "startIndex" = start_index; }
    ///             paginate by_offset(start_index).total(start_index, |page| page.total_players);
    ///         }
    ///     }
    /// }
    ///
    /// async fn names(ladder: &Ladder) -> Result<Vec<String>, concord::Error> {
    ///     let mut names = Vec::new();
    ///     let mut players = ladder.leaderboard().size(200).pages().items();
    ///     while let Some(player) = players.try_next().await? {
    ///         names.push(player.name);
    ///     }
    ///     Ok(names)
    /// }
    /// ```
    // Keep the public pager signature's explicit call, answer and problem types.
    // PANIC-SAFETY: The returned closure keeps the call, answer and problem types without boxing; no panic lint is relaxed.
    #[allow(clippy::type_complexity)]
    fn total<S, F, T>(
        self,
        start: S,
        total: F,
    ) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
    where
        S: Slot<C>,
        S::Value: Copy + Default + TryInto<u64>,
        F: Fn(&C::Value) -> T + Send + 'static,
        T: TryInto<u64>,
        C::Value: IntoItems;

    /// `paginate by_offset(start).page_size(count, 20);`: the API sends `count` items on each page
    /// but the last, and 20 when the caller set no `count`. A page that holds fewer items is the
    /// last, and the pager is not asked; for a full page, the pager decides. A `count` or a
    /// `default` that is not a number of items (a negative one) ends nothing.
    ///
    /// `by_offset(start).page_size(count, 20)` is `by_offset_limit(start, count)` with the
    /// API's default: the caller who sets no `count` saves the empty request at the end. Use it
    /// only for an API that promises full pages until the last one.
    ///
    /// ```
    /// use concord::{by_offset, Json};
    /// use concord::page::Pager;
    ///
    /// concord::api! {
    ///     pub client Matches {
    ///         base "https://matches.example.test";
    ///         read Json;
    ///         /// At most `count` ids (20 when not set), from `start`.
    ///         GET ids(puuid: String, start?: u32, count?: u32) -> Vec<String> {
    ///             path "by-puuid/{puuid}/ids";
    ///             query { start; count; }
    ///             paginate by_offset(start).page_size(count, 20);
    ///         }
    ///     }
    /// }
    ///
    /// async fn every_id(matches: &Matches, puuid: &str) -> Result<Vec<String>, concord::Error> {
    ///     let mut ids = Vec::new();
    ///     let mut pages = matches.ids(puuid).pages().items();
    ///     while let Some(id) = pages.try_next().await? {
    ///         ids.push(id);
    ///     }
    ///     Ok(ids)
    /// }
    /// ```
    // Keep the public pager signature's explicit call, answer and problem types.
    // PANIC-SAFETY: The returned closure keeps the call, answer and problem types without boxing; no panic lint is relaxed.
    #[allow(clippy::type_complexity)]
    fn page_size<K>(
        self,
        count: K,
        default: K::Value,
    ) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
    where
        K: Slot<C>,
        K::Value: Copy + TryInto<usize> + Send + 'static,
        C::Value: IntoItems;

    /// `paginate by_cursor(token, |page| page.next.clone()).also(|call, page| …);`: the next call
    /// that the pager found, changed by your function before it is sent. It receives that call
    /// and the page that arrived, and returns the call to send. It is not run at the end, nor
    /// after an error.
    ///
    /// Use it for a second input that changes with the first: a marker that comes in a pair, an
    /// input that the API refuses next to a cursor (set to `None` with its setter), or a path that
    /// changes after the first page. Every other input of the call is kept, as by every pager.
    ///
    /// ```
    /// use concord::{by_cursor, Json};
    /// use concord::page::Pager;
    /// use serde::Deserialize;
    ///
    /// /// Kubernetes: `metadata.continue`, empty on the last page.
    /// #[derive(Deserialize)]
    /// pub struct PodList { pub metadata: ListMeta, pub items: Vec<serde_json::Value> }
    /// #[derive(Deserialize)]
    /// pub struct ListMeta { #[serde(rename = "continue", default)] pub next: String }
    ///
    /// concord::api! {
    ///     pub client Cluster {
    ///         base "https://cluster.example.test";
    ///         read Json;
    ///         /// A continued list may not name a resource version: the API refuses both together.
    ///         GET pods(limit?: i64, continue_token?: String, resource_version?: String) -> PodList {
    ///             path "api/v1/pods";
    ///             query { limit; "continue" = continue_token; "resourceVersion" = resource_version; }
    ///             paginate by_cursor(continue_token, |page| Some(page.metadata.next.clone()).filter(|t| !t.is_empty()))
    ///                 .also(|call, _| call.resource_version(None));
    ///         }
    ///     }
    /// }
    ///
    /// async fn count(cluster: &Cluster) -> Result<usize, concord::Error> {
    ///     let mut pages = cluster.pods().limit(500).resource_version("0").pages();
    ///     let mut count = 0;
    ///     while let Some(page) = pages.try_next().await? {
    ///         count += page.items.len();
    ///     }
    ///     Ok(count)
    /// }
    /// ```
    // Keep the public pager signature's explicit call, answer and problem types.
    // PANIC-SAFETY: The returned closure keeps the call, answer and problem types without boxing; no panic lint is relaxed.
    #[allow(clippy::type_complexity)]
    fn also<F>(
        self,
        also: F,
    ) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
    where
        F: Fn(C, &C::Value) -> C + Send + 'static;

    /// `paginate forward.when_set(before, backward);`: when the call that fetched the page set
    /// `before`, `backward` decides the next call; otherwise this pager does. For an API that
    /// pages in both directions, with one input for each (`starting_after` and `ending_before`,
    /// `after_id` and `before_id`): the caller chooses the direction by the input it sets, and
    /// the pages keep it.
    ///
    /// ```
    /// use concord::page::{IntoItems, Pager};
    /// use concord::{by_cursor, Json};
    /// use serde::Deserialize;
    ///
    /// /// A page that names its first and its last id.
    /// #[derive(Deserialize)]
    /// pub struct Batches {
    ///     pub data: Vec<serde_json::Value>,
    ///     pub has_more: bool,
    ///     pub first_id: Option<String>,
    ///     pub last_id: Option<String>,
    /// }
    ///
    /// concord::api! {
    ///     pub client Batching {
    ///         base "https://batches.example.test";
    ///         read Json;
    ///         GET batches(limit?: u32, after_id?: String, before_id?: String) -> Batches {
    ///             path "v1/batches";
    ///             query { limit; after_id; before_id; }
    ///             paginate by_cursor(after_id, |page| page.last_id.clone())
    ///                 .when_set(before_id, by_cursor(before_id, |page| page.first_id.clone()))
    ///                 .more(|page| page.has_more);
    ///         }
    ///     }
    /// }
    ///
    /// async fn older_than(batching: &Batching, id: &str) -> Result<usize, concord::Error> {
    ///     let mut pages = batching.batches().before_id(id).pages();
    ///     let mut count = 0;
    ///     while let Some(page) = pages.try_next().await? {
    ///         count += page.data.len();
    ///     }
    ///     Ok(count)
    /// }
    /// ```
    // Keep the public pager signature's explicit call, answer and problem types.
    // PANIC-SAFETY: The returned closure keeps the call, answer and problem types without boxing; no panic lint is relaxed.
    #[allow(clippy::type_complexity)]
    fn when_set<S, P>(
        self,
        input: S,
        other: P,
    ) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
    where
        S: Slot<C>,
        P: Pager<C>;

    /// `paginate by_page(page, 1).if_set(page);`: when the call that fetched the page did not set
    /// `input`, this page is the last, and the pager is not asked. For an API that pages only when
    /// asked to, and otherwise sends the whole list in one response.
    ///
    /// ```
    /// use concord::page::{IntoItems, Pager};
    /// use concord::{by_page, Json};
    /// use serde::Deserialize;
    ///
    /// /// The whole list without `page`; with it, 100 contacts and the number of pages.
    /// #[derive(Deserialize)]
    /// pub struct Contacts {
    ///     #[serde(rename = "Contacts", default)]
    ///     pub contacts: Vec<serde_json::Value>,
    ///     pub pagination: Option<Pagination>,
    /// }
    /// #[derive(Deserialize)]
    /// #[serde(rename_all = "camelCase")]
    /// pub struct Pagination { pub page: u32, pub page_count: u32 }
    ///
    /// impl IntoItems for Contacts {
    ///     type Item = serde_json::Value;
    ///     fn into_items(self) -> Vec<Self::Item> { self.contacts }
    ///     fn count(&self) -> usize { self.contacts.len() }
    /// }
    ///
    /// concord::api! {
    ///     pub client Books {
    ///         base "https://books.example.test";
    ///         read Json;
    ///         GET contacts(page?: u32) -> Contacts {
    ///             path "api.xro/2.0/Contacts";
    ///             query { page; }
    ///             paginate by_page(page, 1)
    ///                 .more(|list| list.pagination.as_ref().map(|p| p.page < p.page_count))
    ///                 .if_set(page);
    ///         }
    ///     }
    /// }
    ///
    /// async fn every_contact(books: &Books) -> Result<usize, concord::Error> {
    ///     let mut contacts = books.contacts().page(1).pages().items();
    ///     let mut count = 0;
    ///     while contacts.try_next().await?.is_some() {
    ///         count += 1;
    ///     }
    ///     Ok(count)
    /// }
    /// ```
    // Keep the public pager signature's explicit call, answer and problem types.
    // PANIC-SAFETY: The returned closure keeps the call, answer and problem types without boxing; no panic lint is relaxed.
    #[allow(clippy::type_complexity)]
    fn if_set<S>(
        self,
        input: S,
    ) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
    where
        S: Slot<C>;
}

// The bodies are not written yet: each closure holds what it will use, so that its
// signature, its captures and its `Send + 'static` bound are the ones the trait promises.
// PANIC-SAFETY: A design of the surface only; every body is replaced before the surface ships.
#[allow(clippy::unimplemented)]
impl<C, P, N> Pager<C> for P
where
    C: Call,
    P: Fn(C, &Answer<C::Value>) -> N + Send + 'static,
    N: IntoNext<C>,
{
    fn more<F, M>(
        self,
        more: F,
    ) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
    where
        F: Fn(&C::Value) -> M + Send + 'static,
        M: Into<Option<bool>>,
    {
        move |_call, _answer| {
            let _ = (&self, &more);
            unimplemented!()
        }
    }

    fn total<S, F, T>(
        self,
        start: S,
        total: F,
    ) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
    where
        S: Slot<C>,
        S::Value: Copy + Default + TryInto<u64>,
        F: Fn(&C::Value) -> T + Send + 'static,
        T: TryInto<u64>,
        C::Value: IntoItems,
    {
        move |_call, _answer| {
            let _ = (&self, &start, &total);
            unimplemented!()
        }
    }

    fn page_size<K>(
        self,
        count: K,
        default: K::Value,
    ) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
    where
        K: Slot<C>,
        K::Value: Copy + TryInto<usize> + Send + 'static,
        C::Value: IntoItems,
    {
        move |_call, _answer| {
            let _ = (&self, &count, &default);
            unimplemented!()
        }
    }

    fn also<F>(
        self,
        also: F,
    ) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
    where
        F: Fn(C, &C::Value) -> C + Send + 'static,
    {
        move |_call, _answer| {
            let _ = (&self, &also);
            unimplemented!()
        }
    }

    fn when_set<S, Q>(
        self,
        input: S,
        other: Q,
    ) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
    where
        S: Slot<C>,
        Q: Pager<C>,
    {
        move |_call, _answer| {
            let _ = (&self, &input, &other);
            unimplemented!()
        }
    }

    fn if_set<S>(
        self,
        input: S,
    ) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
    where
        S: Slot<C>,
    {
        move |_call, _answer| {
            let _ = (&self, &input);
            unimplemented!()
        }
    }
}
