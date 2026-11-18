//! `Paging`: what the shipped pagers return, with what the page states, a short-page rule, a
//! gate and edits, decided in one fixed order.

use concord_core::error::{mismatch, Fail};
use concord_core::page::{Call, InputId, IntoItems, IntoNext, Pager, Slot};
use concord_core::Answer;

/// The text of the Mismatch when a page says that more items follow and nothing moves.
const MORE_WITHOUT_NEXT: &str = "the page says that more items follow, and gives no next page";
/// The text of the Mismatch when two statements of one page disagree.
const DISAGREE: &str = "the page's statements disagree on whether more items follow";
/// The text of the Mismatch when a total cannot be read as a number of items.
const TOTAL: &str = "the total of the list could not be read";
/// The text of the Mismatch when the call sets two inputs that each choose the move.
const TWO_MOVES: &str = "the call chooses two moves: it sets two inputs that each choose one";
/// The text of the Mismatch when an offset cannot be counted.
const OFFSET: &str = "the offset cannot grow any further";

type Gate<C> = Box<dyn Fn(&C) -> bool + Send>;
/// The input that a slot names on this call, when that input is set: both read together.
type Chooser<C> = Box<dyn Fn(&C) -> Option<InputId> + Send>;
type Statement<C> =
    Box<dyn FnMut(&C, &<C as Call>::Value) -> Result<bool, Fail<<C as Call>::Problem>> + Send>;
type Move<C> = Box<dyn Pager<C>>;
/// A short-page rule reads only a size from the call (a `Copy` value), never the call itself.
type Short<C> = Box<dyn Fn(&C) -> Option<usize> + Send>;
type Edit<C> = Box<dyn FnMut(C, &<C as Call>::Value) -> C + Send>;

/// A pager with what the page states about it. The six shipped pagers return one; so do
/// [`by_offset_total`], and [`Paging::from_fn`] makes one of your own function. It is a [`Pager`], so `paginate` takes it as it is; its methods need no import.
///
/// For each page, a `Paging` decides **in this order, whatever the order in which its methods
/// were written**:
///
/// 0. **The call.** If it sets the inputs of two [`or_when_set`](Paging::or_when_set), the pages
///    end with a Mismatch (`the call chooses two moves: it sets two inputs that each choose one`),
///    whatever the page says: the same mistake does not pass on a list of one page. An input
///    given to two `or_when_set` is one choice, and the first written moves. An input is known
///    by [`Slot::input`], its position among the inputs of the endpoint, never by the type of
///    the slot or where its value lives: a slot of yours that forwards to an input returns that
///    input's `input()`, and two inputs that share storage or take no room are two.
/// 1. **The gate.** If one input given to [`only_when_set`](Paging::only_when_set) is not set on
///    the call that fetched the page, this page is the last. Nothing else is read.
/// 2. **What the page states.** Every statement ([`has_more`](Paging::has_more), and the total of
///    [`by_offset_total`]) is read, once, in the order written, whatever the others say. If one
///    cannot be read (a total that is not a number of items), its failure is the result. Otherwise,
///    if one says "no more" and another "more", the pages end with a Mismatch
///    (`the page's statements disagree on whether more items follow`). If all say "no more", this
///    page is the last. The order of the methods does not matter for statements that read only
///    the page; two statements that share state of their own see each other in the order written.
/// 3. **The move.** The next call is computed: by the pager of [`or_when_set`](Paging::or_when_set)
///    whose input the call set, or else by the pager this value was made from. Its failure (an
///    unreadable `Link`, a pager of your own that fails, two inputs set that each choose the
///    move) is the result, also on a short page. If a statement said "more" and there is no next
///    call, the pages end with a Mismatch
///    (`the page says that more items follow, and gives no next page`).
/// 4. **With no statement at all,** a short-page rule
///    ([`stop_on_short_page`](Paging::stop_on_short_page), and the `count` of
///    [`by_offset_limit`](super::by_offset_limit)) ends the list on a short page, after the move;
///    otherwise the move decides, and no next call is the end. A statement always has authority
///    over the short-page rule.
/// 5. **The edits** of [`map_next`](Paging::map_next) are applied to the next call, in the order
///    they were written. The order written also counts inside two other steps: the statements
///    are read in that order (step 2), and of two `or_when_set` given one input, the first
///    written moves (step 0).
///
/// Then the rules of [`Pages`](concord_core::Pages), for every pager: the next request may not
/// repeat the one just sent (a Mismatch), nor go to another origin (a Build error). The page that
/// arrived is returned first; what was decided about it comes with the following `try_next()`, and
/// after an error the pages are over.
///
/// ```
/// use concord::page::IntoItems;
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
///         /// Forward after the last customer; back before the first, when the caller set `ending_before`.
///         GET customers(limit?: u32, starting_after?: String, ending_before?: String) -> List<Customer> {
///             path "v1/customers";
///             query { limit; starting_after; ending_before; }
///             paginate by_cursor(starting_after, |page| page.data.last().map(|c| c.id.clone()))
///                 .or_when_set(ending_before, by_cursor(ending_before, |page| page.data.first().map(|c| c.id.clone())))
///                 .has_more(|page| page.has_more);
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
///
/// A closure passed to a method reads the page without naming its type when the pager moves an
/// input, whose slot tells Rust the call. `by_link()` and `by_next_url(…)` move no input, so there
/// the type is written: `by_link().has_more(|page: &Batch| page.has_more)`. A closure that uses an
/// input of the endpoint (a slot) is a `move` closure.
///
/// # A `Paging` is not a function
///
/// Before `Paging`, the shipped pagers returned closures. Code that used one as a function, or
/// wrote one where `paginate` reads a closure, stops compiling. Find the message here:
///
/// | The message | What was written | What to write |
/// |---|---|---|
/// | ``expected function, found `Paging<…>` `` (E0618) | `pager(call, page)` | `pager.next_call(call, page)`, with `use concord::page::Pager;`, from a closure that owns the pager: `let mut pager = by_cursor(…);` then `move \|call, page\| pager.next_call(call, page)`; a function that returns that closure returns `impl FnMut(…)`, not `impl Fn(…)` |
/// | ``expected an `Fn(…)` closure, found `Paging<…>` ``, or the same with `FnMut` (E0277), at a `Box<dyn Fn(…)>` or `Box<dyn FnMut(…)>` | a shipped pager in a box of a function type, alone or beside a closure | a box of a pager, given its type in a `let`: `let pager: Box<dyn Pager<C>> = if … { Box::new(by_cursor(…)) } else { Box::new(Paging::from_fn(\|call: C, page: &Answer<T>\| …)) }; pager`; the closure names its parameters' types |
/// | the same E0277, at a bound of a function of yours (`impl FnMut(C, &Answer<T>) -> Option<C>`) | a shipped pager passed to that function | a parameter of type `impl Pager<C>` (and a return of `impl Pager<C>`), called with [`Pager::next_call`]; or [`into_fn`](Paging::into_fn), with the function's types changed to `Result<Option<C>, Fail<C::Problem>>` |
/// | `` `paginate` reads this expression as a closure, and its value is `Paging<…>` `` | a shipped pager anywhere but a call or a path (alone, or in a plain block, `if` or `match`): from a macro of yours, after a `return` or a `break`, in a `loop` or a labelled block, under `#[cfg]`, behind `*` or `.0` | `.into_fn()` written after the pager: `loop { break by_cursor(…).into_fn(); }`, `my_pager!(…).into_fn()`; or the pager as a call or a path. `Paging::from_pager(…)` does not help there: the expression inside it is still one that `paginate` reads as a closure |
#[must_use = "a pager does nothing until `paginate` takes it"]
pub struct Paging<C: Call> {
    // The lock is only ever reached through `&mut self` (`get_mut`): it costs nothing, and it
    // makes `Paging` `Sync` without asking `Sync` of the functions it holds.
    parts: std::sync::Mutex<Parts<C>>,
}

/// The parts of a `Paging`, in the order in which they decide.
struct Parts<C: Call> {
    /// 1. The gates: each input must be set on the call that fetched the page.
    only_when_set: Vec<Gate<C>>,
    /// 2. The statements of the page: `true` for "more".
    statements: Vec<Statement<C>>,
    /// 3. The moves chosen by an input, then the move otherwise.
    or_when_set: Vec<(Chooser<C>, Move<C>)>,
    moves: Move<C>,
    /// 4. The short-page rules, read only when no statement spoke: each gives a size, and the
    ///    page is counted once, after the move.
    stop_on_short_page: Vec<Short<C>>,
    count: Option<fn(&C::Value) -> usize>,
    /// 5. The edits of the next call, in the order written.
    map_next: Vec<Edit<C>>,
}

impl<C: Call> Paging<C> {
    fn parts(&mut self) -> &mut Parts<C> {
        self.parts
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<C: Call> std::fmt::Debug for Paging<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Paging").finish_non_exhaustive()
    }
}

// What each shipped pager returns is `Send` and `Sync`, as their closures were.
const _: () = {
    fn send_and_sync<T: Send + Sync>() {}
    // The assertion is a function that type-checks and is never called.
    // PANIC-SAFETY: Only an unused-code lint is relaxed, for a compile-time assertion; no panic lint is relaxed.
    #[allow(dead_code)]
    fn paging<C: Call>() {
        send_and_sync::<Paging<C>>();
    }
};

impl<C: Call> Paging<C> {
    /// Makes a `Paging` that moves as `pager` does: any [`Pager`], such as a type of your own that
    /// wraps a shipped pager. For a closure, [`from_fn`](Paging::from_fn) infers its parameters.
    ///
    /// ```
    /// use concord::page::{Call, Pager, Paging};
    /// use concord::{by_cursor, Answer, Fail, Json};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// pub struct Search { pub timed_out: bool, pub next: Option<String> }
    ///
    /// /// Refuses a page that timed out, then asks the pager inside.
    /// pub struct Complete<P>(pub P);
    /// impl<C: Call<Value = Search>, P: Pager<C>> Pager<C> for Complete<P> {
    ///     fn next_call(&mut self, call: C, page: &Answer<Search>) -> Result<Option<C>, Fail<C::Problem>> {
    ///         if page.value().timed_out {
    ///             return Err(concord::mismatch("the search timed out"));
    ///         }
    ///         self.0.next_call(call, page)
    ///     }
    /// }
    ///
    /// concord::api! {
    ///     pub client Index {
    ///         base "https://index.example.test";
    ///         read Json;
    ///         GET search(after?: String) -> Search {
    ///             query { after; }
    ///             paginate Paging::from_pager(Complete(by_cursor(after, |page| page.next.clone())))
    ///                 .has_more(|page| page.next.is_some());
    ///         }
    ///     }
    /// }
    /// ```
    pub fn from_pager<P: Pager<C>>(pager: P) -> Paging<C> {
        Paging {
            parts: std::sync::Mutex::new(Parts {
                only_when_set: Vec::new(),
                statements: Vec::new(),
                or_when_set: Vec::new(),
                moves: Box::new(pager),
                stop_on_short_page: Vec::new(),
                count: None,
                map_next: Vec::new(),
            }),
        }
    }

    /// Makes a `Paging` of a function of your own, from the call and the page to the next call:
    /// `Option<C>`, or the same in a `Result` whose error is a [`Fail`]. Unlike
    /// [`from_pager`](Paging::from_pager), it takes the closure's shape from its bound, so the closure's
    /// parameters need no lifetime. Written in place, the closure names the type of the page
    /// (`page: &Answer<Changes>`): Rust learns the call only after the whole expression. A
    /// closure with no method after it needs nothing of this: `paginate |call, page| …;` as
    /// today.
    ///
    /// ```
    /// use concord::page::{Paging, Slot};
    /// use concord::{Answer, Json};
    /// use serde::Deserialize;
    ///
    /// /// Drive's changes: the next token, or the token to start from next time.
    /// #[derive(Deserialize)]
    /// #[serde(rename_all = "camelCase")]
    /// pub struct Changes { pub next_page_token: Option<String>, pub new_start_page_token: Option<String> }
    ///
    /// concord::api! {
    ///     pub client Drive {
    ///         base "https://drive.example.test";
    ///         read Json;
    ///         GET changes(page_token: String) -> Changes {
    ///             query { "pageToken" = page_token; }
    ///             paginate Paging::from_fn(move |call, page: &Answer<Changes>| {
    ///                 page.value().next_page_token.clone().map(|token| page_token.set(call, token))
    ///             })
    ///             .has_more(|page| page.new_start_page_token.is_none());
    ///         }
    ///     }
    /// }
    /// ```
    pub fn from_fn<F, N>(pager: F) -> Paging<C>
    where
        F: FnMut(C, &Answer<C::Value>) -> N + Send + 'static,
        N: IntoNext<C>,
    {
        Paging::from_pager(pager)
    }

    /// Turns this pager into a closure, for code that takes a function and not a [`Pager`]: a
    /// function of yours whose parameter is `impl FnMut(C, &Answer<C::Value>) -> …`, or one boxed
    /// function type shared with closures. Where you write the parameter yourself, prefer
    /// `impl Pager<C>` and call [`Pager::next_call`]: it keeps the pager's name in your signature.
    ///
    /// **It ends a decision.** Attach every rule before the conversion: `by_cursor(…)
    /// .has_more(…).into_fn()`. A converted value wrapped again with [`Paging::from_fn`] starts a
    /// separate decision, whose rules come first and know nothing of the inner ones: an outer
    /// "no more" ends the list without reading the inner statements, and an outer short-page rule
    /// can end the list although an inner statement says that more items follow.
    ///
    /// ```
    /// use concord::page::{Call, Paging};
    /// use concord::{by_cursor, Answer, Fail, Json};
    ///
    /// #[derive(serde::Deserialize)]
    /// pub struct Page { pub next: Option<String> }
    ///
    /// /// A function of the author's that wants a function.
    /// fn logged<C: Call>(
    ///     mut next: impl FnMut(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static,
    /// ) -> impl FnMut(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static {
    ///     move |call, page| next(call, page)
    /// }
    ///
    /// concord::api! {
    ///     pub client Feed {
    ///         base "https://feed.example.test";
    ///         read Json;
    ///         GET items(after?: String) -> Page {
    ///             query { after; }
    ///             paginate logged(by_cursor(after, |page| page.next.clone()).into_fn());
    ///         }
    ///     }
    /// }
    /// ```
    // Keep the public pager signature's explicit call, answer and problem types.
    // PANIC-SAFETY: The closure keeps the call, answer and problem types without boxing; no panic lint is relaxed.
    #[allow(clippy::type_complexity)]
    pub fn into_fn(
        mut self,
    ) -> impl FnMut(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + Sync + 'static
    {
        move |call, page| self.next_call(call, page)
    }

    /// `.has_more(|page| page.has_more)`: the page states whether more items follow. `false` ends
    /// the list; `true` requires a next call (step 3 of [`Paging`]). The function returns a
    /// `bool`: when the field may be absent, say what absence means in it
    /// (`page.is_truncated.unwrap_or(false)`). A page number stated with the number of pages is a
    /// statement too: `.has_more(|page| page.page < page.pages)`.
    ///
    /// ```
    /// use concord::{by_cursor, Json};
    /// use serde::Deserialize;
    ///
    /// /// S3's `ListObjectsV2`.
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
    ///                 .has_more(|page| page.is_truncated.unwrap_or(false));
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
    pub fn has_more<F>(mut self, mut more: F) -> Paging<C>
    where
        F: FnMut(&C::Value) -> bool + Send + 'static,
    {
        self.parts()
            .statements
            .push(Box::new(move |_, value| Ok(more(value))));
        self
    }

    /// `.stop_on_short_page(count, 20)`: for an API whose pages state nothing, a page that holds
    /// fewer items than `count`, or than `default` when the caller set no `count`, is the last.
    /// A `count` or a `default` that is not a number of items (a negative one) stops nothing. When
    /// the page states something ([`has_more`](Paging::has_more), a total), the statement decides
    /// and this rule is not read (step 4 of [`Paging`]).
    ///
    /// `default` is your copy of the server's default page size. **If the server lowers it, the
    /// list ends early, in silence**: the one place where Concord ends a list on your word alone.
    /// If it raises it, one empty request more ends the list. Use it only for an API that
    /// promises full pages until the last one.
    ///
    /// ```
    /// use concord::{by_offset, Json};
    ///
    /// concord::api! {
    ///     pub client Matches {
    ///         base "https://matches.example.test";
    ///         read Json;
    ///         /// At most `count` ids (20 when not set), from `start`.
    ///         GET ids(puuid: String, start?: u32, count?: u32) -> Vec<String> {
    ///             path "by-puuid/{puuid}/ids";
    ///             query { start; count; }
    ///             paginate by_offset(start).stop_on_short_page(count, 20);
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
    pub fn stop_on_short_page<K>(self, count: K, default: K::Value) -> Paging<C>
    where
        K: Slot<C>,
        K::Value: Copy + TryInto<usize> + Send + 'static,
        C::Value: IntoItems,
    {
        self.short_page_rule(move |call| {
            count.get(call).copied().unwrap_or(default).try_into().ok()
        })
    }

    /// `.stop_on_short_page_of(100)`: as [`stop_on_short_page`](Paging::stop_on_short_page), for
    /// an API whose page size is fixed and has no input: a page that holds fewer than `size`
    /// items is the last, when the page states nothing. If the API changes the size, the list
    /// ends early, in silence.
    ///
    /// ```
    /// use concord::{by_cursor, Json};
    /// use concord::page::IntoItems;
    /// use serde::Deserialize;
    ///
    /// /// Journals, always 100 at a time, after the number given in `offset`.
    /// #[derive(Deserialize)]
    /// pub struct Journals { #[serde(rename = "Journals", default)] pub journals: Vec<Journal> }
    /// #[derive(Deserialize)]
    /// pub struct Journal { #[serde(rename = "JournalNumber")] pub number: i64 }
    ///
    /// impl IntoItems for Journals {
    ///     type Item = Journal;
    ///     fn into_items(self) -> Vec<Journal> { self.journals }
    ///     fn count(&self) -> usize { self.journals.len() }
    /// }
    ///
    /// concord::api! {
    ///     pub client Books {
    ///         base "https://books.example.test";
    ///         read Json;
    ///         GET journals(offset?: i64) -> Journals {
    ///             path "api.xro/2.0/Journals";
    ///             query { offset; }
    ///             paginate by_cursor(offset, |page| Some(page.journals.last()?.number))
    ///                 .stop_on_short_page_of(100);
    ///         }
    ///     }
    /// }
    /// ```
    pub fn stop_on_short_page_of(self, size: usize) -> Paging<C>
    where
        C::Value: IntoItems,
    {
        self.short_page_rule(move |_| Some(size))
    }

    /// `.only_when_set(page)`: when the call that fetched the page did not set `input`, this page
    /// is the last, and nothing else is read (step 1 of [`Paging`]). For an API that pages only
    /// when asked, and otherwise sends the whole list in one response.
    ///
    /// ```
    /// use concord::page::IntoItems;
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
    ///                 .has_more(|list| list.pagination.as_ref().is_some_and(|p| p.page < p.page_count))
    ///                 .only_when_set(page);
    ///         }
    ///     }
    /// }
    /// ```
    pub fn only_when_set<S: Slot<C>>(mut self, input: S) -> Paging<C> {
        self.parts()
            .only_when_set
            .push(Box::new(move |call| input.get(call).is_some()));
        self
    }

    /// `.or_when_set(before, other)`: when the call that fetched the page set `input`, `other`
    /// moves instead (step 3 of [`Paging`]). For an API that pages in both directions, with one
    /// input for each (`starting_after` and `ending_before`): the caller chooses the direction by
    /// the input it sets. The statements, the short-page rules, the gates and the edits of this
    /// value apply to both moves. The call that fetched a page is checked at each decision,
    /// before the gates: one that sets the inputs of two `or_when_set` ends the pages with a
    /// Mismatch after that page, whatever the page says. That is the first page when the caller
    /// set both inputs, and can be a later page when a move itself sets a second input. An input
    /// given to two `or_when_set` (directly, or through a slot of yours that forwards to it) is
    /// one choice, and the first written moves. An `other` that is a `Paging` decides by the same
    /// order on its own, inside the move: the rules of this value come first.
    ///
    /// ```
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
    ///                 .or_when_set(before_id, by_cursor(before_id, |page| page.first_id.clone()))
    ///                 .has_more(|page| page.has_more);
    ///         }
    ///     }
    /// }
    /// ```
    pub fn or_when_set<S: Slot<C>, P: Pager<C>>(mut self, input: S, other: P) -> Paging<C> {
        // The input is known by its identity in the endpoint (`Slot::input`), not by the type of
        // `S` or where its value lives: a slot of yours that forwards to it is the same input,
        // and one slot type that can name two inputs names two. The identity is read when the
        // decision is made, with the value: a slot names one input at a time.
        self.parts().or_when_set.push((
            Box::new(move |call| input.get(call).map(|_| input.input())),
            Box::new(other),
        ));
        self
    }

    /// `.map_next(|next, page| …)`: changes the next call before it is sent: a second marker
    /// that comes in a pair, an input that the API refuses beside a cursor (cleared with its
    /// setter), a path that changes after the first page. It runs only when there is a next call,
    /// after every decision, in the order the edits were written (step 5 of [`Paging`]). Every
    /// other input of the call is kept.
    ///
    /// ```
    /// use concord::{by_cursor, Json};
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
    ///                 .map_next(|next, _| next.resource_version(None));
    ///         }
    ///     }
    /// }
    /// ```
    pub fn map_next<F>(mut self, edit: F) -> Paging<C>
    where
        F: FnMut(C, &C::Value) -> C + Send + 'static,
    {
        self.parts().map_next.push(Box::new(edit));
        self
    }
}

impl<C: Call> Paging<C> {
    /// Adds a short-page rule: the size below which a page is short, read from the call.
    pub(super) fn short_page_rule<F>(mut self, size: F) -> Paging<C>
    where
        F: Fn(&C) -> Option<usize> + Send + 'static,
        C::Value: IntoItems,
    {
        let parts = self.parts();
        parts.stop_on_short_page.push(Box::new(size));
        parts.count = Some(<C::Value as IntoItems>::count);
        self
    }
}

impl<C: Call> Pager<C> for Box<Paging<C>> {
    fn next_call(
        &mut self,
        call: C,
        page: &Answer<C::Value>,
    ) -> Result<Option<C>, Fail<C::Problem>> {
        (**self).next_call(call, page)
    }
}

impl<C: Call> Pager<C> for Paging<C> {
    fn next_call(
        &mut self,
        call: C,
        page: &Answer<C::Value>,
    ) -> Result<Option<C>, Fail<C::Problem>> {
        let parts = self.parts();
        // 0. The call: two inputs set that each choose a move are a mistake, whatever the page
        // says. One input given to two `or_when_set` is one choice: the first written.
        let mut chosen: Option<(usize, InputId)> = None;
        for (index, (chooses, _)) in parts.or_when_set.iter().enumerate() {
            if let Some(input) = chooses(&call) {
                match chosen {
                    None => chosen = Some((index, input)),
                    Some((_, first)) if first != input => return Err(mismatch(TWO_MOVES)),
                    Some(_) => {}
                }
            }
        }
        // 1. The gates.
        if !parts.only_when_set.iter().all(|set| set(&call)) {
            return Ok(None);
        }
        // 2. What the page states: every statement is read, once, in the order written; the
        // first that cannot be read is the result, then a disagreement.
        let read: Vec<_> = parts
            .statements
            .iter_mut()
            .map(|statement| statement(&call, page.value()))
            .collect();
        let mut said = Vec::with_capacity(read.len());
        for more in read {
            said.push(more?);
        }
        let mut stated = None;
        for more in said {
            if stated.is_some_and(|before| before != more) {
                return Err(mismatch(DISAGREE));
            }
            stated = Some(more);
        }
        if stated == Some(false) {
            return Ok(None);
        }
        // 4 reads only the sizes from the call before the move, which takes the call.
        let size = match stated {
            None => parts
                .stop_on_short_page
                .iter()
                .filter_map(|size| size(&call))
                .max(),
            Some(_) => None,
        };
        // 3. The move: its failure is the result, also on a short page.
        let alternative = chosen.and_then(|(index, _)| parts.or_when_set.get_mut(index));
        let next = match alternative {
            Some((_, other)) => other.next_call(call, page)?,
            None => parts.moves.next_call(call, page)?,
        };
        // 4. With no statement, a short page ends the list: the page is counted after the move.
        if let (Some(size), Some(count)) = (size, parts.count) {
            if count(page.value()) < size {
                return Ok(None);
            }
        }
        let Some(mut next) = next else {
            return match stated {
                Some(true) => Err(mismatch(MORE_WITHOUT_NEXT)),
                _ => Ok(None),
            };
        };
        // 5. The edits, in the order written.
        for edit in &mut parts.map_next {
            next = edit(next, page.value());
        }
        Ok(Some(next))
    }
}

/// `paginate by_offset_total(start, |page| page.total);`: like [`by_offset`](super::by_offset),
/// and the page states how many items the list holds. `n` is the number of items of the page
/// ([`IntoItems::count`]) and `at` the `start` of the call that fetched it (0 when not set). When
/// `at + n` reaches the total, this page is the last; below it, the move must give a next call,
/// and an empty page is the Mismatch of step 3 of [`Paging`], unless a pager chosen by
/// [`or_when_set`](Paging::or_when_set), such as a cursor, still gives one. A total or an offset that cannot be read as an
/// unsigned number, and an `at + n` that overflows, are Mismatches. The offset is named once: it is
/// the input that moves and the one that is counted.
///
/// ```
/// use concord::page::{by_offset_total, IntoItems};
/// use concord::Json;
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
///             paginate by_offset_total(start_index, |page| page.total_players);
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
pub fn by_offset_total<C, S, F, T>(start: S, mut total: F) -> Paging<C>
where
    C: Call,
    S: Slot<C>,
    S::Value: Copy + TryFrom<u64> + TryInto<u64>,
    F: FnMut(&C::Value) -> T + Send + 'static,
    T: TryInto<u64>,
    C::Value: IntoItems,
{
    let mut paging = super::by_offset(start);
    paging.parts().statements.push(Box::new(move |call, value| {
        // An offset that is not set is zero.
        let at: u64 = match start.get(call) {
            Some(at) => (*at).try_into().map_err(|_| mismatch(super::UNREADABLE))?,
            None => 0,
        };
        let count = u64::try_from(value.count()).map_err(|_| mismatch(OFFSET))?;
        let reached = at.checked_add(count).ok_or_else(|| mismatch(OFFSET))?;
        let total: u64 = total(value).try_into().map_err(|_| mismatch(TOTAL))?;
        Ok(reached < total)
    }));
    paging
}

#[cfg(test)]
mod tests {
    use super::*;
    use concord_core::http::{HeaderMap, StatusCode};
    use concord_core::Headers;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::sync::Arc;

    #[derive(Clone, Debug)]
    struct Probe;
    impl Call for Probe {
        type Value = Vec<u32>;
        type Problem = u8;
        fn at_url(self, _: &str) -> Probe {
            self
        }
    }

    /// Two statements that both fail: the first written is the result, in either order, and both
    /// are read once. No public construction has two that can fail, so this is a private test.
    #[test]
    fn c8_6_of_two_failing_statements_the_first_written_is_the_result() {
        for reverse in [false, true] {
            let mut pager = Paging::from_fn(|call: Probe, _: &Answer<Vec<u32>>| Some(call));
            let seen = Arc::new(AtomicUsize::new(0));
            for problem in [!reverse, reverse] {
                let visits = seen.clone();
                pager.parts().statements.push(Box::new(move |_, _| {
                    visits.fetch_add(1, SeqCst);
                    if problem {
                        Err(Fail::from(7))
                    } else {
                        Err(mismatch("the second failure"))
                    }
                }));
            }
            let page = Answer::new(StatusCode::OK, Headers::from(HeaderMap::new()), vec![1]);
            let failure = pager.next_call(Probe, &page).unwrap_err();
            let first = if reverse {
                "Fail(mismatch)"
            } else {
                "Fail(problem)"
            };
            assert_eq!(format!("{failure:?}"), first);
            assert_eq!(seen.load(SeqCst), 2);
        }
    }

    #[test]
    fn c8_6_a_statement_that_fails_after_a_disagreement_is_the_result() {
        let visits: Vec<Arc<AtomicUsize>> = (0..3).map(|_| Arc::new(AtomicUsize::new(0))).collect();
        let (yes, no, failing) = (visits[0].clone(), visits[1].clone(), visits[2].clone());
        let mut pager = Paging::from_fn(|call: Probe, _: &Answer<Vec<u32>>| Some(call))
            .has_more(move |_| {
                yes.fetch_add(1, SeqCst);
                true
            })
            .has_more(move |_| {
                no.fetch_add(1, SeqCst);
                false
            });
        // a statement that cannot be read, written after the two that disagree
        pager.parts().statements.push(Box::new(move |_, _| {
            failing.fetch_add(1, SeqCst);
            Err(Fail::from(7))
        }));
        let page = Answer::new(StatusCode::OK, Headers::from(HeaderMap::new()), vec![1]);
        let error = pager.next_call(Probe, &page).unwrap_err();
        assert_eq!(format!("{error:?}"), "Fail(problem)");
        assert!(visits.iter().all(|visits| visits.load(SeqCst) == 1));
    }
}
