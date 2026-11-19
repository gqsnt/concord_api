//! Lists that span several responses.
//!
//! A pager decides the next call from the call and its response: any [`Pager`], which every
//! closure `|call, page| …` that returns `Option<C>`, or the same wrapped in a `Result`, already
//! is. The client's author chooses one with `paginate`. `concord`'s `by_offset`,
//! `by_offset_limit`, `by_page`, `by_cursor`, `by_link` and `by_next_url` are six functions that
//! return one, built with [`Call`] and [`Slot`]; your own pager is built in the same way.

/// Borrows a URL reference without its fragment, before resolving or comparing a next page.
/// No URL parsing, decoding or allocation occurs. A percent-encoded `#` remains
/// part of the path or query; only a literal fragment delimiter ends the reference.
///
/// ```
/// # use concord_core as concord;
/// use concord::page::fragmentless;
/// assert_eq!(fragmentless("../items?key=%23value#section"), "../items?key=%23value");
/// assert_eq!(fragmentless("/items"), "/items");
/// ```
pub fn fragmentless(url: &str) -> &str {
    url.split_once('#').map_or(url, |(url, _)| url)
}

use crate::error::{Error, Fail};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A page that contains items. Implement this for your own list type.
///
/// `Option` and `Result` deliberately do not implement it: their domain outcomes
/// must be handled before a page can be flattened. Pages and their items are `Send`.
///
/// ```
/// # use concord_core as concord;
/// struct Batch { rows: Vec<u64> }
/// impl concord::page::IntoItems for Batch {
///     type Item = u64;
///     fn into_items(self) -> Vec<u64> { self.rows }
///     fn count(&self) -> usize { self.rows.len() }
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a list of items",
    note = "implement `concord::page::IntoItems` for your page type"
)]
pub trait IntoItems: Send {
    /// The type of one item.
    type Item: Send;
    /// Takes the items in the order the provider sent them.
    fn into_items(self) -> Vec<Self::Item>;
    /// Returns how many items this page holds.
    fn count(&self) -> usize;
}

impl<T: Send> IntoItems for Vec<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> {
        self
    }
    fn count(&self) -> usize {
        self.len()
    }
}

/// The pages of a call, one call per page. Returned by `.pages()`.
///
/// `T` is the type of a page, `E` the API's own problem type.
///
/// Pass `.pages()` from a generated list call to an application function like this:
///
/// ```
/// # use concord_core as concord;
/// use concord::{Error, Pages};
///
/// async fn collect_ids(mut pages: Pages<Vec<String>>) -> Result<Vec<String>, Error> {
///     let mut ids = Vec::new();
///     while let Some(page) = pages.try_next().await? {
///         ids.extend(page);
///     }
///     Ok(ids)
/// }
/// ```
///
/// It is also a `Stream` of `Result<T, Error<E>>`, for the tools that take one.
/// Each request is prepared once, when its page is requested, using the current secrets;
/// that exact prepared request is then sent. Pages pin the origin of the first finished
/// request, after `edit_request` and before middleware. Each later finished request
/// is checked against that origin before requests are compared; a change of origin,
/// including one caused by a replaced host secret, ends the pages with a Build error.
/// `at_url` is checked separately while each request is built, before `edit_request`,
/// against the origin that the call builds from its base (or `with_base`).
/// A shared one-shot stream body causes a Build error on the second page; a body that the
/// description creates anew for each send gives each page its own stream.
///
/// A repeated request is a Mismatch: the same method, URL, body bytes and headers,
/// leaving out the fragment and every part that a secret filled in: parts of the host or
/// path, query pairs and header values. Sensitive header values are left out too, including
/// those marked by `edit_request`. Body bytes are compared as they are, so when a secret in
/// the body is replaced, the next request differs in its bytes and one repeat goes
/// undetected. A one-shot stream body never counts as a repeat: it has no bytes
/// to compare. Repeatable bodies compare the flattened sequence of bytes, factories
/// and map identities, without opening any source. After an `edit_request` that changes the URL,
/// its secret marks are no longer known and the URL is compared as it is, without
/// its fragment. A changed continuation header advances the page; a replaced key
/// whose marks are known does not.
/// When a next URL already contains a secret query key, every pair with that key
/// is removed before the current secret is added.
#[must_use = "pages are requested with `try_next`"]
pub struct Pages<T, E = ()> {
    // The pager and exchange need only be Send. Mutex preserves Pages' Sync
    // promise; polling uses get_mut under the caller's exclusive borrow.
    source: std::sync::Mutex<Box<dyn PageSource<T, E>>>,
}

pub(crate) trait PageSource<T, E>: Send {
    fn poll_page(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<T, Error<E>>>>;
}

impl<T, E> Pages<T, E> {
    pub(crate) fn new(source: impl PageSource<T, E> + 'static) -> Self {
        Self {
            source: std::sync::Mutex::new(Box::new(source)),
        }
    }

    fn poll_page(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<T, Error<E>>>> {
        self.source
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .poll_page(cx)
    }
}

impl<T: Send, E: Send> Pages<T, E> {
    /// Returns every item of every page, in order: `call.pages().items()`.
    /// Each page is consumed through [`IntoItems::into_items`].
    ///
    /// A page that is an `Option` or a `Result` cannot silently lose its outcome.
    /// The diagnostic says "is not a list of items" and asks for
    /// `concord::page::IntoItems` to be implemented for the page type.
    /// (Rustdoc's `compile_fail` checks that the code is rejected, but cannot match the
    /// diagnostic text.)
    ///
    /// ```compile_fail
    /// # use concord_core as concord;
    /// use concord::Pages;
    /// type Page = Option<Vec<u32>>;
    /// trait Call { fn pages(self) -> Pages<Page>; }
    /// fn check(call: impl Call) {
    ///     let _ = call.pages().items();
    /// }
    /// ```
    ///
    /// ```compile_fail
    /// # use concord_core as concord;
    /// use concord::Pages;
    /// type Page = Result<Vec<u32>, String>;
    /// trait Call { fn pages(self) -> Pages<Page>; }
    /// fn check(call: impl Call) {
    ///     let _ = call.pages().items();
    /// }
    /// ```
    pub fn items<I>(self) -> Items<I, E>
    where
        T: IntoItems<Item = I>,
        T: 'static,
        E: 'static,
        I: Send + 'static,
    {
        Items {
            pages: Pages::new(Flatten {
                pages: self,
                items: Vec::new().into_iter(),
            }),
        }
    }
}

impl<T: Send, E: Send> Pages<T, E> {
    /// Returns the next page, or `None` after the last one. Nothing is sent before the first
    /// call.
    ///
    /// Cancel safe: a `try_next()` that is dropped loses nothing and sends nothing twice;
    /// the exchange in flight lives in this value, and the next `try_next()` continues it.
    /// After an error the pages are over: the next `try_next()` returns `Ok(None)`, and the
    /// stream ends the same way. Retrying is the job of the call's retry policy; to
    /// continue later, call the endpoint again from the last page that you hold.
    ///
    /// # Errors
    ///
    /// The error of the call for that page, or a Mismatch when the pager returns the
    /// request that was just sent.
    ///
    /// `Fail::from(problem)` in a pager becomes a Refused error with that problem;
    /// `concord::mismatch(…)` becomes a Mismatch. Both are returned by the `try_next()`
    /// after the page that arrived.
    /// [`Fail::from_read`] in a pager that decodes a part of the page itself becomes the error
    /// that a reader gives for that page: on a 2xx, the Mismatch that names the location of the
    /// fault. When a
    /// pager fails, the caller receives, in this order: the page that arrived, as
    /// `Ok(Some(page))`; the error, from the next `try_next()`; then `Ok(None)` from every
    /// later one, and nothing more is sent.
    // The public signature explicitly promises a Send future.
    // PANIC-SAFETY: The explicit returned future states the hidden/public Send contract; no panic lint is relaxed.
    #[allow(clippy::manual_async_fn)]
    pub fn try_next(&mut self) -> impl Future<Output = Result<Option<T>, Error<E>>> + Send + '_ {
        async {
            std::future::poll_fn(|cx| self.poll_page(cx))
                .await
                .transpose()
        }
    }
}

impl<T, E> std::fmt::Debug for Pages<T, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pages").finish_non_exhaustive()
    }
}

impl<T: Send, E: Send> futures_core::Stream for Pages<T, E> {
    type Item = Result<T, Error<E>>;

    /// The same rule as [`try_next`](Pages::try_next): after the first error, the pages are over.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().poll_page(cx)
    }
}

/// The items of every page, in order. Returned by `.pages().items()`, for a page type that
/// implements [`IntoItems`].
///
/// `I` is the type of an item, `E` the API's own problem type.
#[must_use = "items are fetched with `try_next`"]
pub struct Items<I, E = ()> {
    pages: Pages<I, E>,
}

struct Flatten<T: IntoItems, E> {
    pages: Pages<T, E>,
    items: std::vec::IntoIter<T::Item>,
}

impl<T: IntoItems, E: Send> PageSource<T::Item, E> for Flatten<T, E> {
    fn poll_page(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<T::Item, Error<E>>>> {
        loop {
            if let Some(item) = self.items.next() {
                return Poll::Ready(Some(Ok(item)));
            }
            match self.pages.poll_page(cx) {
                Poll::Ready(Some(Ok(page))) => self.items = page.into_items().into_iter(),
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(Err(error))),
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl<I: Send, E: Send> Items<I, E> {
    /// Returns the next item, or `None` after the last one. Cancel safe, like
    /// [`Pages::try_next`], and finished after an error in the same way.
    ///
    /// # Errors
    ///
    /// The same as [`Pages::try_next`].
    // The public signature explicitly promises a Send future.
    // PANIC-SAFETY: The explicit returned future states the hidden/public Send contract; no panic lint is relaxed.
    #[allow(clippy::manual_async_fn)]
    pub fn try_next(&mut self) -> impl Future<Output = Result<Option<I>, Error<E>>> + Send + '_ {
        self.pages.try_next()
    }
}

impl<I, E> std::fmt::Debug for Items<I, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Items").finish_non_exhaustive()
    }
}

impl<I: Send, E: Send> futures_core::Stream for Items<I, E> {
    type Item = Result<I, Error<E>>;

    /// The same rule as [`try_next`](Items::try_next): after the first error, the items are over.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().pages.poll_page(cx)
    }
}

/// What a pager may do with a call, besides the methods the client's author provides.
///
/// Every generated call implements it. It is not meant to be implemented by hand.
pub trait Call: Clone + Send + Sync + 'static {
    /// The type that sending the call returns: one page.
    type Value: Send + 'static;
    /// The API's own problem type.
    type Problem: Send + Sync + 'static;

    /// Returns the same call, sent to this URL instead of its usual URL.
    ///
    /// `at_url` sets a path and a query, never an origin.
    /// A URL at another origin than the one that the call builds from its base
    /// (or `with_base`) is a Build error, in pages and outside them. This check runs
    /// on its own when the request is built, before `edit_request`.
    /// When your pager returns this call, Pages records the URL of the response
    /// your pager saw. A relative URL is resolved against that response's URL;
    /// for your own `call.at_url("../next")`, it resolves against the URL the call builds.
    /// Resolution happens before the origin check and before your `edit_request` edits.
    ///
    /// The author's headers are still sent. Original query pairs are not added
    /// again, except those holding a `Secret`. A fragment is dropped before comparison.
    fn at_url(self, url: &str) -> Self;
}

/// A call that supplies its endpoint's name, as every generated call does.
///
/// The name is the one reported by errors and transport information for this call.
///
/// ```
/// # use concord_core as concord;
/// use concord::page::NamedCall;
///
/// fn endpoint<C: NamedCall>(_: &C) -> &'static str {
///     C::ENDPOINT
/// }
/// ```
pub trait NamedCall: Call {
    /// The client, scopes and endpoint joined with dots, such as `files.objects.get`.
    const ENDPOINT: &'static str;
}

/// The slot of one input on a call.
///
/// In a `paginate` statement, an input of the endpoint names its slot, not its value:
/// `paginate by_offset_limit(start, count);` hands two slots to `by_offset_limit`, which reads them on the
/// call whose response arrived and sets one on the next call.
///
/// ```
/// # use concord_core as concord;
/// use concord::page::{Call, IntoItems, Slot};
/// use concord::Answer;
///
/// /// The next page starts one page further: `paginate by_page(page);`.
/// pub fn by_page<C: Call, S: Slot<C, Value = u32>>(page: S) -> impl Fn(C, &Answer<C::Value>) -> Option<C>
/// where
///     C::Value: IntoItems,
/// {
///     move |call, answer| {
///         if answer.value().count() == 0 {
///             return None;
///         }
///         let at = page.get(&call).copied().unwrap_or(1);
///         Some(page.set(call, at + 1))
///     }
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not an input of this endpoint",
    note = "in `paginate`, pass the endpoint's inputs to the pager: `paginate by_offset(start);`"
)]
pub trait Slot<C>: Copy + Send + Sync + 'static {
    /// The declared value type of the input, without the optional-input wrapper.
    type Value;

    /// Returns the value on this call: `None` when the input is optional and was not set.
    fn get<'a>(&self, call: &'a C) -> Option<&'a Self::Value>;

    /// Returns the call with this input set.
    fn set(&self, call: C, value: Self::Value) -> C;

    /// Which input of the endpoint this slot names. A slot of yours that forwards to another
    /// returns the identity of the input it forwards to (`self.0.input()`), whatever it reads
    /// or where its value lives.
    ///
    /// A slot names one input for the whole of a decision: for as long as a pager decides on a
    /// page, `get` and `input` answer for the same input. A pager calls them one after the
    /// other when it decides, and reads them again at the next page; it never reads one when it
    /// is built and the other later. So a slot of yours that can change the input it names (by
    /// a switch that another thread turns, say) changes it between two decisions, never during
    /// one. Otherwise a pager may read the value of one input and the identity of another, and
    /// report two moves for a call that sets one input, or miss a call that sets two.
    fn input(&self) -> InputId;
}

/// Identifies the input of an endpoint that a [`Slot`] names, by its position among the inputs
/// of the endpoint: the number that the generated code gives it. Two slots of one call `C` name one input when
/// their `InputId`s are equal; the call's type is the endpoint, so the number alone is compared.
///
/// `or_when_set` knows an input by it: where the value of an input lives, and its type, are not
/// which input it is (two inputs may share storage, or take no room at all).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InputId(usize);

impl InputId {
    /// The input at `position` among the inputs of the endpoint, in the order the generated code
    /// numbers them. A slot of your own over a call type of your own numbers its inputs itself.
    pub const fn new(position: usize) -> InputId {
        InputId(position)
    }
}

/// What a pager may return: `Some(next call)`, `None` at the end, or the same inside a
/// `Result` whose error is a [`Fail`] (a page that cannot be trusted is an error, not the end).
///
/// `Fail::from(problem)` in a pager becomes a Refused error with that problem;
/// `concord::mismatch(…)` becomes a Mismatch. Both are returned by the `try_next()`
/// after the page that arrived.
#[diagnostic::on_unimplemented(
    message = "a pager returns the next call: `Some(call)`, `None`, or a `Result` of either"
)]
///
/// Only these shapes implement it, each for its own call: another crate cannot add one.
pub trait IntoNext<C: Call>: sealed::Sealed<C> {
    /// Returns the next call, the end, or the failure.
    fn into_next(self) -> Result<Option<C>, Fail<C::Problem>>;
}

// The seal names the call and the exact error type, so an implementation outside
// this crate cannot exist, and a later conversion of pager errors cannot overlap one.
mod sealed {
    use super::{Call, Fail};
    pub trait Sealed<C> {}
    impl<C: Call> Sealed<C> for Option<C> {}
    impl<C: Call> Sealed<C> for Result<Option<C>, Fail<C::Problem>> {}
}

impl<C: Call> IntoNext<C> for Option<C> {
    fn into_next(self) -> Result<Option<C>, Fail<C::Problem>> {
        Ok(self)
    }
}

impl<C: Call> IntoNext<C> for Result<Option<C>, Fail<C::Problem>> {
    fn into_next(self) -> Result<Option<C>, Fail<C::Problem>> {
        self
    }
}

/// What `paginate` takes: it receives the call that fetched a page and that page, and returns
/// the next call, the end (`None`), or a failure.
///
/// Every closure that returns `Option<C>`, or `Result<Option<C>, Fail<C::Problem>>`, is a pager,
/// and so is the `Paging` that `concord`'s shipped pagers return. A type of your own implements
/// it to keep state or to wrap another pager, which it can then call:
///
/// ```
/// # use concord_core as concord;
/// use concord::page::{Call, Pager};
/// use concord::{Answer, Fail};
///
/// /// Stops after `left` pages, whatever the pager inside says.
/// pub struct AtMost<P> { pub pager: P, pub left: u32 }
///
/// impl<C: Call, P: Pager<C>> Pager<C> for AtMost<P> {
///     fn next_call(&mut self, call: C, page: &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> {
///         self.left = self.left.saturating_sub(1);
///         if self.left == 0 {
///             return Ok(None);
///         }
///         self.pager.next_call(call, page)
///     }
/// }
/// ```
///
/// A pager is called once for each page, before that page is handed to the caller. It lives as
/// long as the `Pages` that `.pages()` returned, so its state is that of one walk through the list.
///
/// **A box of your own pager.** `Box<dyn Pager<C>>` is a pager, and so is a box of a closure; a
/// box of a type of yours is not, and `paginate Box::new(MyPager)` says
/// ``expected an `FnMut(…)` closure, found `MyPager` ``. Write either a box of the trait, given
/// its type in a `let`: `paginate { let pager: Box<dyn Pager<_>> = Box::new(MyPager); pager };`,
/// or the implementation for your box, beside your type:
///
/// ```
/// # use concord_core as concord;
/// # use concord::page::{Call, Pager};
/// # use concord::{Answer, Fail};
/// # pub struct MyPager;
/// # impl<C: Call> Pager<C> for MyPager {
/// #     fn next_call(&mut self, _: C, _: &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> { Ok(None) }
/// # }
/// impl<C: Call> Pager<C> for Box<MyPager> {
///     fn next_call(&mut self, call: C, page: &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> {
///         (**self).next_call(call, page)
///     }
/// }
/// ```
///
/// One implementation for every `Box<P>` cannot be written: Rust refuses it beside the one for
/// closures, since a box of a closure is a closure (E0119).
///
/// **A method of your own named `next_call`.** `Pager` is implemented for every closure, so with
/// `use concord::page::*;` its `next_call` is in scope wherever a closure is. If a trait of yours
/// also has a method named `next_call`, call yours by its trait, as Rust does for any two traits
/// with one method name: `LocalMove::next_call(&mut f, call, page)`, or `&mut *f` for a box
/// `f: Box<dyn …>`. Rust may report the
/// ambiguity (E0034, "multiple applicable items in scope"), or, on a box of a closure, quietly
/// call Concord's method, which Rust finds first; so qualify the call in both cases.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a pager for this endpoint",
    note = "a pager is a closure `|call, page| …` that returns the next call, or a value such as `by_cursor(…)`"
)]
pub trait Pager<C: Call>: Send + 'static {
    /// Returns the next call, `None` when this page is the last, or the failure that ends the
    /// pages after this page.
    ///
    /// # Errors
    ///
    /// `Fail::from(problem)` becomes a Refused error with that problem; `concord::mismatch(…)`
    /// a Mismatch. Both are returned by the `try_next()` after the page that arrived.
    fn next_call(
        &mut self,
        call: C,
        page: &crate::Answer<C::Value>,
    ) -> Result<Option<C>, Fail<C::Problem>>;
}

impl<C, F, N> Pager<C> for F
where
    C: Call,
    F: FnMut(C, &crate::Answer<C::Value>) -> N + Send + 'static,
    N: IntoNext<C>,
{
    fn next_call(
        &mut self,
        call: C,
        page: &crate::Answer<C::Value>,
    ) -> Result<Option<C>, Fail<C::Problem>> {
        self(call, page).into_next()
    }
}

/// A boxed pager of any type: `Box<dyn Pager<C>>`. A box of a closure is a pager because the
/// box is a closure too, and `concord`'s `Paging` has its own impl for `Box<Paging<C>>`. (One impl
/// for every `Box<P>` would overlap the impl for closures, which Rust refuses.)
impl<C: Call> Pager<C> for Box<dyn Pager<C>> {
    fn next_call(
        &mut self,
        call: C,
        page: &crate::Answer<C::Value>,
    ) -> Result<Option<C>, Fail<C::Problem>> {
        (**self).next_call(call, page)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn pager_failure_docs_agree_on_kind_problem_and_delivery() {
        let wording =
            "`Fail::from(problem)` in a pager becomes a Refused error with that problem; \
            `concord::mismatch(…)` becomes a Mismatch. Both are returned by the `try_next()` \
            after the page that arrived.";
        fn docs(source: &str) -> String {
            source
                .lines()
                .filter_map(|line| line.trim().strip_prefix("///"))
                .map(str::trim)
                .collect::<Vec<_>>()
                .join(" ")
        }
        assert_eq!(docs(include_str!("page.rs")).matches(wording).count(), 2);
        assert_eq!(docs(include_str!("error.rs")).matches(wording).count(), 1);
    }
}
