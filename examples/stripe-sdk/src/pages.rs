//! Stripe's cursor pagination: ordinary traversals of every list operation, and the adapter
//! underneath them.
//!
//! Stripe v1 lists page with object ids: `starting_after=<last id>` fetches the page after a
//! page, `ending_before=<first id>` the page before it, and the two are mutually exclusive.
//! `has_more` says whether more objects exist in the requested direction. The next cursor is
//! therefore known only once a page has arrived (a dependent chain): pages of one list are
//! fetched one after another, never in parallel, and no cursor is ever computed from a count.
//!
//! # Ordinary use
//!
//! Every generated list operation has `items()` and `pages()`: the traversal forward from the
//! first page, as `concord_pagination`'s owned [`PageItems`] / [`Pages`] ([`StripeItems`],
//! [`StripePages`]). The end is explicit: `Ok(None)` only after the page with `has_more: false`;
//! a cap ([`Pages::max_pages`], none by default) reached while Stripe has more is
//! `Err(PagesError::Bounded { resume, .. })`; a failed page (a status, a protocol violation, a
//! transport failure) is `Err(PagesError::Provider { error, failed, .. })` with this SDK's
//! [`Error`]. Neither is ever a silent end.
//!
//! ```no_run
//! use std::num::NonZeroUsize;
//! use concord_example_stripe_sdk::{Limit, StripeContext};
//!
//! # async fn run(stripe: StripeContext) -> Result<(), Box<dyn std::error::Error>> {
//! let limit = Limit::new(100)?;
//! // Every invoice, item by item; `?` on each step: a cap or a failure is an error.
//! let mut invoices = stripe.api().invoices().list().set_limit(limit).items()?;
//! while let Some(invoice) = invoices.next().await? {
//!     println!("{} {}", invoice.id, invoice.total);
//! }
//!
//! // Pages with their request metadata, at most 20; or everything, keeping the prefix.
//! let cap = NonZeroUsize::new(20).unwrap();
//! let mut pages = stripe.api().customers().list().pages()?.max_pages(cap);
//! while let Some(page) = pages.next_page().await? {
//!     println!("{} customers (request {:?})", page.items.len(), page.meta.request_id);
//! }
//! let products = stripe.api().products().list().items()?.collect_complete().await?;
//! # let _ = products; Ok(()) }
//! ```
//!
//! A bounded or failed traversal names the page to continue from (`PagesError::resume`); pass
//! it to [`CursorList::pages_from`] over the same list. [`CursorList`] is also the way to page
//! [`Direction::Backward`] or to continue a first page obtained another way
//! ([`CursorList::after_embedded`]).
//!
//! # The adapter
//!
//! [`CursorList`] holds one list request (a generated operation with its filters, page size and
//! expansions — the *template*) and one [`Direction`]. [`CursorList::fetch`] builds the request
//! for a cursor (a fresh clone of the template; nothing is shared between pages), sends it once
//! through [`crate::send()`] and projects the envelope into a `concord_pagination::Page`
//! ([`StripePage`]): the items, the next [`Cursor`] and the page's [`ResponseMeta`]. The ordinary
//! traversals above run it; it is also the page factory of the `concord_pagination` engine
//! (`sequential`, `resume_items`) for item-level checkpoints and engine reports:
//!
//! ```no_run
//! use std::num::NonZeroUsize;
//! use concord_example_stripe_sdk::pages::CursorList;
//! use concord_example_stripe_sdk::{Limit, StripeContext};
//! use concord_pagination::{Budget, Resume, sequential};
//!
//! # async fn run(stripe: StripeContext) -> Result<(), concord_example_stripe_sdk::Error> {
//! let limit = Limit::new(100).expect("1-100");
//! let list = CursorList::forward(stripe.api().invoices().list().set_limit(limit))?;
//! let budget = Budget::new(NonZeroUsize::new(20).unwrap());
//! let mut pages = sequential(Resume::Start, budget, |cursor| list.fetch(cursor));
//! while let Some(page) = pages.next().await {
//!     let page = page?; // a failure ends the chain; `pages.finish()` keeps the restart point
//!     println!("{} invoices (request {:?})", page.items.len(), page.meta.request_id);
//! }
//! let report = pages.finish(); // the engine's `None` also ends a bounded chain: check this
//! # let _ = report; Ok(()) }
//! ```
//!
//! # Protocol rules the projection enforces
//!
//! | Page | Result |
//! |---|---|
//! | `has_more: false` (with or without items) | terminal: `next` is `None`; an empty terminal page is an ordinary empty result |
//! | `has_more: true`, items present | `next` is the last item's id (forward) or the first item's (backward) |
//! | `has_more: true`, `data: []` | [`ProtocolError::EmptyNonTerminalPage`]: no object can name the next page, so this is an error, not an empty success |
//! | an item without a usable `id` | a Concord `Decode` error: every listed model requires its typed id |
//! | `object` other than `"list"` | a Concord `Decode` error |
//! | the next cursor equals the cursor that requested the page | [`ProtocolError::CursorDidNotAdvance`] |
//!
//! # Direction and binding
//!
//! A [`Cursor`] records its direction and a *binding*: a fingerprint of the template's prepared
//! request (method, URL with filters, page size and expansions, `Stripe-Version` and
//! `Stripe-Account`), cursor parameters excluded. [`CursorList::fetch`] refuses, before
//! building any request:
//!
//! - a cursor of the other direction ([`ProtocolError::IncompatibleDirection`]);
//! - a cursor bound to another template — another account, filter or page size
//!   ([`ProtocolError::CursorBindingMismatch`]).
//!
//! [`CursorList::new`] refuses a template that already sets `starting_after` or
//! `ending_before` ([`ProtocolError::TemplateHasCursor`]): the traversal owns those parameters.
//!
//! The fingerprint uses `std`'s `DefaultHasher`: stable within one build of the program, which
//! is what an in-process or same-binary checkpoint needs; it is not a cross-version format.
//!
//! # Ordering and snapshots
//!
//! Items keep Stripe's order (reverse chronological for top-level lists). A backward traversal
//! returns pages of newer objects, each page still newest first. Stripe lists are live: objects
//! created during a traversal can appear or not; a traversal is not a snapshot.

use std::future::Future;
use std::hash::{DefaultHasher, Hash, Hasher};

use concord::Prepared;
use concord::reqwest::Url;
use concord_pagination::{Page, PageItems, Pages, Resume};

use crate::expandable::{List, Object};
use crate::send::{Error, ProtocolError, ResponseMeta, send};
use crate::{
    BalanceTransaction, Customer, Invoice, InvoiceLineItem, Price, Product,
    StripeBalanceTransactionsList, StripeCustomersList, StripeInvoicesLines, StripeInvoicesList,
    StripePricesList, StripeProductsList, StripeSubscriptionsList, Subscription,
};

/// A generated list operation the adapter can page: it sets the two cursor parameters and
/// prepares the request. Implemented for every list operation of the declaration.
pub trait ListRequest: Clone {
    /// The listed object.
    type Item: Object + serde::de::DeserializeOwned;

    /// Sets (`Some`) or clears (`None`) `starting_after`.
    fn with_starting_after(self, cursor: Option<<Self::Item as Object>::Id>) -> Self;

    /// Sets (`Some`) or clears (`None`) `ending_before`.
    fn with_ending_before(self, cursor: Option<<Self::Item as Object>::Id>) -> Self;

    /// Prepares the request (no I/O).
    ///
    /// # Errors
    ///
    /// Concord's `Build` error.
    fn prepare_list(self) -> Result<Prepared<List<Self::Item>>, concord::Error>;
}

macro_rules! list_request {
    ($($operation:ty => $item:ty),* $(,)?) => {
        $(impl ListRequest for $operation {
            type Item = $item;
            fn with_starting_after(self, cursor: Option<<$item as Object>::Id>) -> Self {
                self.maybe_starting_after(cursor)
            }
            fn with_ending_before(self, cursor: Option<<$item as Object>::Id>) -> Self {
                self.maybe_ending_before(cursor)
            }
            fn prepare_list(self) -> Result<Prepared<List<$item>>, concord::Error> {
                self.prepare()
            }
        }

        impl $operation {
            /// Every object of this list, forward from its first page, one at a time: see
            /// [`CursorList::items`].
            ///
            /// # Errors
            ///
            /// As [`CursorList::new`]; nothing is sent before the first `next()`.
            pub fn items(self) -> Result<StripeItems<$item>, Error> {
                Ok(CursorList::forward(self)?.items())
            }

            /// Every page of this list, forward from its first page: see
            /// [`CursorList::pages`].
            ///
            /// # Errors
            ///
            /// As [`CursorList::new`]; nothing is sent before the first `next_page()`.
            pub fn pages(self) -> Result<StripePages<$item>, Error> {
                Ok(CursorList::forward(self)?.pages())
            }
        })*
    };
}

list_request!(
    StripeCustomersList => Customer,
    StripeSubscriptionsList => Subscription,
    StripeInvoicesList => Invoice,
    StripeInvoicesLines => InvoiceLineItem,
    StripeProductsList => Product,
    StripePricesList => Price,
    StripeBalanceTransactionsList => BalanceTransaction,
);

/// The direction of a traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    /// `starting_after=<last id>`: older objects, page after page.
    Forward,
    /// `ending_before=<first id>`: newer objects, page after page.
    Backward,
}

/// Where a traversal continues: an object id, its direction and the binding of the list it came
/// from. `Debug` shows the direction only.
#[derive(Clone, PartialEq, Eq, concord::Debug)]
pub struct Cursor<I> {
    direction: Direction,
    #[sensitive]
    id: I,
    #[sensitive]
    binding: u64,
}

impl<I> Cursor<I> {
    /// The direction.
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// The object id the next request continues from. Deliberately named: a cursor is opaque
    /// list position data.
    pub fn id(&self) -> &I {
        &self.id
    }

    /// The fingerprint of the list it belongs to.
    pub fn binding(&self) -> u64 {
        self.binding
    }

    /// The parts, for an application that stores checkpoints itself.
    pub fn into_parts(self) -> (Direction, I, u64) {
        (self.direction, self.id, self.binding)
    }

    /// A cursor from stored parts. [`CursorList::fetch`] still checks its direction and binding.
    pub fn from_parts(direction: Direction, id: I, binding: u64) -> Self {
        Self {
            direction,
            id,
            binding,
        }
    }
}

/// One projected page: `concord_pagination`'s page with Stripe cursors and response metadata.
pub type StripePage<T> = Page<T, Cursor<<T as Object>::Id>, ResponseMeta>;

/// The ordinary page traversal of a list ([`CursorList::pages`]): owned, nameable, its end
/// explicit (see the [module documentation](self)).
pub type StripePages<T> = Pages<T, Cursor<<T as Object>::Id>, ResponseMeta, Error>;

/// The ordinary item traversal of a list ([`CursorList::items`]).
pub type StripeItems<T> = PageItems<T, Cursor<<T as Object>::Id>, Error>;

/// One list request and a direction. See the [module documentation](self).
#[derive(Clone, concord::Debug)]
#[sensitive]
pub struct CursorList<R: ListRequest> {
    template: R,
    #[public]
    direction: Direction,
    binding: u64,
}

impl<R: ListRequest> CursorList<R> {
    /// Pages `template` in `direction`.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::TemplateHasCursor`] when the template already sets a cursor parameter;
    /// Concord's `Build` error when the template cannot be prepared.
    pub fn new(template: R, direction: Direction) -> Result<Self, Error> {
        let prepared = template.clone().prepare_list()?;
        let url = prepared.request().url();
        if has_cursor(url) {
            return Err(Error::protocol_error(
                ProtocolError::TemplateHasCursor,
                None,
            ));
        }
        let binding = fingerprint(&prepared);
        Ok(Self {
            template,
            direction,
            binding,
        })
    }

    /// [`new`](Self::new) with [`Direction::Forward`].
    ///
    /// # Errors
    ///
    /// As [`new`](Self::new).
    pub fn forward(template: R) -> Result<Self, Error> {
        Self::new(template, Direction::Forward)
    }

    /// The direction.
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// The template's fingerprint.
    pub fn binding(&self) -> u64 {
        self.binding
    }

    /// A cursor continuing after `id` in this list's direction — for a first page obtained
    /// another way, such as an invoice's embedded `lines` list, whose last id continues
    /// `GET /v1/invoices/{id}/lines`. The caller asserts that `id` belongs to this list.
    pub fn continue_from(&self, id: <R::Item as Object>::Id) -> Cursor<<R::Item as Object>::Id> {
        Cursor {
            direction: self.direction,
            id,
            binding: self.binding,
        }
    }

    /// The cursor continuing a first page obtained another way — an invoice's embedded `lines`
    /// list continues `GET /v1/invoices/{id}/lines` — under the same rules as a fetched page:
    /// `None` when `has_more` is false, the edge item's id otherwise.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::EmptyNonTerminalPage`] for `has_more: true` with no item.
    pub fn after_embedded(
        &self,
        list: &List<R::Item>,
    ) -> Result<Option<Cursor<<R::Item as Object>::Id>>, Error> {
        if !list.has_more {
            return Ok(None);
        }
        let edge = match self.direction {
            Direction::Forward => list.data.last(),
            Direction::Backward => list.data.first(),
        };
        match edge {
            Some(item) => Ok(Some(self.continue_from(item.id().clone()))),
            None => Err(Error::protocol_error(
                ProtocolError::EmptyNonTerminalPage,
                None,
            )),
        }
    }

    /// The request for the page at `cursor` (`None`: the first page).
    ///
    /// # Errors
    ///
    /// [`ProtocolError::IncompatibleDirection`] or [`ProtocolError::CursorBindingMismatch`] for a
    /// cursor that does not belong to this traversal. Nothing is built or sent.
    pub fn request(&self, cursor: Option<&Cursor<<R::Item as Object>::Id>>) -> Result<R, Error> {
        let template = self.template.clone();
        let Some(cursor) = cursor else {
            return Ok(template);
        };
        if cursor.direction != self.direction {
            return Err(Error::protocol_error(
                ProtocolError::IncompatibleDirection,
                None,
            ));
        }
        if cursor.binding != self.binding {
            return Err(Error::protocol_error(
                ProtocolError::CursorBindingMismatch,
                None,
            ));
        }
        Ok(match self.direction {
            Direction::Forward => template.with_starting_after(Some(cursor.id.clone())),
            Direction::Backward => template.with_ending_before(Some(cursor.id.clone())),
        })
    }

    /// Fetches the page at `cursor` (`None`: the first page) and projects it. The returned
    /// future owns everything it needs (it borrows nothing from `self`), so a traversal can call
    /// this from an `FnMut` page factory. The request is built and checked before the future is
    /// returned; nothing is sent until it is polled.
    pub fn fetch(
        &self,
        cursor: Option<Cursor<<R::Item as Object>::Id>>,
    ) -> impl Future<Output = Result<StripePage<R::Item>, Error>> + use<R> {
        let prepared = self
            .request(cursor.as_ref())
            .and_then(|request| request.prepare_list().map_err(Error::from));
        let direction = self.direction;
        let binding = self.binding;
        async move {
            let response = send(prepared?).await?;
            project(
                response.value,
                direction,
                binding,
                cursor.as_ref(),
                response.meta,
            )
        }
    }
}

impl<R> CursorList<R>
where
    R: ListRequest + Send + 'static,
    R::Item: Send + 'static,
    <R::Item as Object>::Id: Send + 'static,
{
    /// The pages of this list from its first page: a [`Pages`] traversal over
    /// [`fetch`](Self::fetch), one page at a time, without a cap unless you set one
    /// ([`Pages::max_pages`]). `Ok(None)` only after the page with `has_more: false`; a cap
    /// reached while Stripe has more, or a failed page, is an error that names the page to
    /// continue from. Nothing is sent until the first `next_page()`.
    pub fn pages(&self) -> StripePages<R::Item> {
        self.pages_from(Resume::Start)
    }

    /// [`pages`](Self::pages) from `from`: a cursor of this list (a bounded or failed
    /// traversal's `PagesError::resume`, [`continue_from`](Self::continue_from),
    /// [`after_embedded`](Self::after_embedded)), or `Resume::Start`. A cursor of another list
    /// or direction is refused by the first page's request, before anything is sent. Add `concord-pagination` as a direct dependency and use `concord_pagination::Resume`. `Resume::Start` repeats the original cursor-free list request in this list's direction; `Resume::At(key)` takes this list's `Cursor<<R::Item as Object>::Id>` from a returned page's `next`, `continue_from` or `after_embedded`.
    pub fn pages_from(
        &self,
        from: Resume<Cursor<<R::Item as Object>::Id>>,
    ) -> StripePages<R::Item> {
        let list = self.clone();
        Pages::new(from, move |cursor| list.fetch(cursor))
    }

    /// The objects of this list from its first page, one at a time ([`PageItems`] over
    /// [`pages`](Self::pages)); the same end rules.
    pub fn items(&self) -> StripeItems<R::Item> {
        self.pages().items()
    }
}

/// Projects one list envelope into a page: the items, the next cursor (if `has_more`) and the
/// metadata. See the [module documentation](self) for the rules.
///
/// # Errors
///
/// [`ProtocolError::EmptyNonTerminalPage`] and [`ProtocolError::CursorDidNotAdvance`].
pub fn project<T: Object>(
    list: List<T>,
    direction: Direction,
    binding: u64,
    requested: Option<&Cursor<T::Id>>,
    meta: ResponseMeta,
) -> Result<StripePage<T>, Error> {
    let next = if list.has_more {
        let edge = match direction {
            Direction::Forward => list.data.last(),
            Direction::Backward => list.data.first(),
        };
        let Some(edge) = edge else {
            return Err(Error::protocol_error(
                ProtocolError::EmptyNonTerminalPage,
                Some(meta),
            ));
        };
        let id = edge.id().clone();
        if requested.is_some_and(|cursor| cursor.id == id) {
            return Err(Error::protocol_error(
                ProtocolError::CursorDidNotAdvance,
                Some(meta),
            ));
        }
        Some(Cursor {
            direction,
            id,
            binding,
        })
    } else {
        None
    };
    Ok(Page {
        items: list.data,
        next,
        meta,
    })
}

fn has_cursor(url: &Url) -> bool {
    url.query_pairs()
        .any(|(key, _)| key == "starting_after" || key == "ending_before")
}

fn fingerprint<T>(prepared: &Prepared<T>) -> u64 {
    let request = prepared.request();
    let mut hasher = DefaultHasher::new();
    request.method().as_str().hash(&mut hasher);
    let url = request.url();
    url.path().hash(&mut hasher);
    for (key, value) in url.query_pairs() {
        if key != "starting_after" && key != "ending_before" {
            key.hash(&mut hasher);
            value.hash(&mut hasher);
        }
    }
    for name in ["stripe-account", "stripe-version"] {
        name.hash(&mut hasher);
        for value in request.headers().get_all(name) {
            value.as_bytes().hash(&mut hasher);
        }
    }
    hasher.finish()
}
