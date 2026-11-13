//! A list that comes in several answers.
//!
//! A pager is a function from the call and its answer to the next call: any
//! `Fn(C, &Answer<C::Value>) -> Option<C>`, or the same with a `Result` around it. The
//! client's author chooses one with `paginate`. `by_offset`, `by_offset_limit`, `by_page`, `by_cursor`, `by_link` and `by_next_url` of
//! `concord` are six functions that give one, written with [`Call`] and [`Slot`]; one of
//! yours is written in the same way.

/// Borrow a URL reference without its fragment, before resolving or comparing a next page.
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
    note = "implement concord::page::IntoItems for your page type"
)]
pub trait IntoItems: Send {
    /// The type of one item.
    type Item: Send;
    /// Take the items in provider order.
    fn into_items(self) -> Vec<Self::Item>;
    /// How many items this page holds.
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

/// The pages of a call, one call for each. From `.pages()`.
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
/// Each request is prepared once when its page is asked for, using current secrets;
/// that exact preparation is then sent. Pages pin the origin of the first finished
/// request, after `edit_request` and before middleware. Each later finished request
/// is checked against that origin before request comparison; a change of origin,
/// including through a replaced host secret, ends the pages with a Build error.
/// `at_url` is checked separately while building each request, before `edit_request`,
/// against the origin the call builds from its base (or `with_base`).
/// A shared one-shot stream body gives a Build error on the second page; a body the
/// description makes anew for each send gives each page its own stream.
///
/// A repeated request gives a Mismatch: the same method, URL, body bytes and headers,
/// leaving out the fragment and every piece that a secret wrote: host/path pieces,
/// query pairs and header values. Sensitive header values are omitted too, including
/// those marked by `edit_request`. Body bytes are compared as they are, so a replaced
/// body secret looks like a move once. A streamed body is never the same request:
/// it has no bytes to compare. After an `edit_request` that changes the URL,
/// its secret marks are no longer known and the URL is compared as it is, without
/// its fragment. A changed continuation header advances the page; a replaced key
/// with known marks does not.
/// When a next URL already contains a secret query key, every pair of that key
/// is removed before the current secret is written.
#[must_use = "pages are asked for with `try_next`"]
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
    /// Every item of every page, in order: `call.pages().items()`.
    /// Each page is consumed through [`IntoItems::into_items`].
    ///
    /// A page that is an `Option` or a `Result` cannot silently lose its outcome.
    /// The diagnostic says "is not a list of items" and asks for
    /// `concord::page::IntoItems` to be implemented for the page type:
    /// (Rustdoc's `compile_fail` checks rejection, but cannot match diagnostic text.)
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
    /// The next page, or `None` after the last one. Nothing is sent before the first call.
    ///
    /// Cancel safe: a `try_next()` that is dropped loses nothing and sends nothing twice;
    /// the exchange in flight lives in this value, and the next `try_next()` goes on with it.
    /// After an error the pages are over: the next `try_next()` gives `Ok(None)`, and the
    /// stream ends the same way. Trying again is the job of the retry policy of the call; to
    /// go on later, call the endpoint again from the last page that you hold.
    ///
    /// # Errors
    ///
    /// The error of the call for that page, or a Mismatch when the pager asks for
    /// the request that was just sent.
    ///
    /// `Fail::from(problem)` in a pager becomes a Refused error with that problem;
    /// `concord::mismatch(…)` becomes a Mismatch. Both are returned by the `try_next()`
    /// after the page that arrived.
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

/// The items of every page, in order. From `.pages().items()`, for a page type with [`IntoItems`].
///
/// `I` is the type of an item, `E` the API's own problem type.
#[must_use = "items are asked for with `try_next`"]
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
    /// The next item, or `None` after the last one. Cancel safe, like [`Pages::try_next`],
    /// and over after an error in the same way.
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
/// Every generated call has it. It is not meant to be written by hand.
pub trait Call: Clone + Send + Sync + 'static {
    /// The type that sending the call gives: one page.
    type Value: Send + 'static;
    /// The API's own problem type.
    type Problem: Send + Sync + 'static;

    /// The same call, sent to this URL in place of its usual URL.
    ///
    /// `at_url` gives a path and a query, never an origin.
    /// A URL at another origin than the one that the call builds from its base
    /// (or `with_base`) is a Build error, in pages and outside them. This check runs
    /// on its own when the request is built, before `edit_request`.
    /// When your pager returns this call, Pages records the URL of the answer
    /// your pager saw. A relative URL is resolved against that answer's URL;
    /// for your own `call.at_url("../next")`, it resolves against the URL the call builds.
    /// Resolution happens before the origin check and before your `edit_request` edits.
    ///
    /// The author's headers are still written. Original query pairs are not added
    /// again, except those holding a `Secret`. A fragment is dropped before comparison.
    fn at_url(self, url: &str) -> Self;
}

/// The place of one input on a call.
///
/// In a `paginate` statement, an input of the endpoint names its place, not its value:
/// `paginate by_offset_limit(start, count);` hands two places to `by_offset_limit`, which reads them on the
/// call that was answered and sets one on the next call.
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
    note = "in paginate, a pager is given inputs: paginate by_offset(start);"
)]
pub trait Slot<C>: Copy + Send + Sync + 'static {
    /// The declared value type of the input, without the optional-input wrapper.
    type Value;

    /// The value on this call. `None` when the input is optional and was not given.
    fn get<'a>(&self, call: &'a C) -> Option<&'a Self::Value>;

    /// The call with this input set.
    fn set(&self, call: C, value: Self::Value) -> C;
}

/// What a pager may give: `Some(next call)`, `None` at the end, or the same inside a
/// `Result` whose error is a [`Fail`] (a page that cannot be believed is an error, not the end).
///
/// `Fail::from(problem)` in a pager becomes a Refused error with that problem;
/// `concord::mismatch(…)` becomes a Mismatch. Both are returned by the `try_next()`
/// after the page that arrived.
#[diagnostic::on_unimplemented(
    message = "a pager gives the next call: Some(call), None, or a Result of that"
)]
pub trait IntoNext<C: Call>: sealed::Sealed {
    /// The next call, the end, or the failure.
    fn into_next(self) -> Result<Option<C>, Fail<C::Problem>>;
}

mod sealed {
    pub trait Sealed {}
    impl<C> Sealed for Option<C> {}
    impl<C, F> Sealed for Result<Option<C>, F> {}
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
