# Pages

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Rate limits](limits.md) · next: [Testing a client](testing.md)*

This page covers lists that arrive over several responses. The author writes one line on the endpoint; the caller gets `.pages()`.

The work is split three ways. The description says how the API paginates (`paginate by_offset(start);`). A pager builds the next call from the call that fetched a page and that page. The caller chooses whether to loop: `.send()` for one page, `.pages()` for all of them.

## Pagination is one line on the endpoint

How an API paginates is a fact about the API, the same for every caller. So it is one line on the endpoint, and the call gets `.pages()`:

```rust
let mut pages = asia.ids(&puuid).count(100).pages();
while let Some(page) = pages.try_next().await? {
    for id in page { println!("{id}"); }
}

// or, when only the items matter
let mut ids = asia.ids(&puuid).count(100).pages().items();
while let Some(id) = ids.try_next().await? { println!("{id}"); }

// one page, on purpose
let first = asia.ids(&puuid).count(100).send().await?;
```

**A call to a paginated endpoint cannot be awaited directly.** `.send().await` returns one page, and says so in the code; `.pages()` returns all of them. A bare `.await` there does not compile (`rustc` says that the call `is not a future`), so nobody mistakes the first page for the whole list. On every other endpoint, `.await` works as usual.

## The line to write

Most APIs page in one of five ways, and each is one line. The shop below has one endpoint for each:

```rust
use concord::page::{by_offset_total, IntoItems};
use concord::{by_cursor, by_link, by_next_url, by_page_bounds, Json};

concord::api! {
    pub client Shop {
        base "https://shop.example.test";
        read Json;

        /// A cursor, and a flag that says whether more items follow.
        GET customers(limit?: u32, starting_after?: String) -> Batch<Customer> {
            query { limit; starting_after; }
            paginate by_cursor(starting_after, |page| page.last_id.clone())
                .has_more(|page| page.has_more);
        }
        /// An offset, and the total number of items in the list.
        GET orders(offset?: u32, limit?: u32) -> Counted<Order> {
            query { offset; limit; }
            paginate by_offset_total(offset, |page| page.total);
        }
        /// A page number, and the number of pages.
        GET products(page?: u32) -> Numbered<Product> {
            query { page; }
            paginate by_page_bounds(page, |list| Some((list.page, list.pages)));
        }
        /// The URL of the next page, in a `Link` header.
        GET reviews(per_page?: u32) -> Vec<Review> {
            query { per_page; }
            paginate by_link();
        }
        /// The URL of the next page, in the body.
        GET invoices() -> Linked<Invoice> {
            paginate by_next_url(|page: &Linked<Invoice>| page.next.clone());
        }
    }
}
```

The types are your own, beside the macro:

```rust
#[derive(Deserialize)]
pub struct Batch<T> { pub data: Vec<T>, pub has_more: bool, pub last_id: Option<String> }
#[derive(Deserialize)]
pub struct Counted<T> { pub items: Vec<T>, pub total: u64 }
#[derive(Deserialize)]
pub struct Numbered<T> { pub rows: Vec<T>, pub page: u32, pub pages: u32 }
#[derive(Deserialize)]
pub struct Linked<T> { pub results: Vec<T>, pub next: Option<String> }

impl<T: Send> IntoItems for Batch<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> { self.data }
    fn count(&self) -> usize { self.data.len() }
}
// The same for `Counted` (`items`).
```

Use `by_page` to advance the requested number and find the end by counting items, or `by_page_bounds` to follow the server's reported current and last page numbers without counting items.

Two more shapes are common. An API that states nothing about the rest of the list, and promises full pages until the last one, ends on a short page: `paginate by_offset(start).stop_on_short_page(count, 20);`, where `20` is the server's default page size ([a short page](#more-items-but-nothing-to-continue-with-is-an-error)). An API that pages only when the caller asks for a page, and otherwise sends the whole list in one response, adds `.only_when_set(page)`.

**In `paginate`, an input refers to its slot on the call**, not to its value: the pager reads it on the call that fetched the page, and sets it on the next one. So `starting_after` above is the input itself, and `by_cursor` gives it a new value on each next call; every other input of the call is kept as the caller set it. The shipped pagers' names start with `by_` because APIs often name their inputs `offset`, `cursor` and `page`, and an input shadows an item of the same name within its node. (That is Rust's own rule. The item is then written with its path: `body concord::form(form);`.)

**A closure given to a pager or to one of its methods reads the page as the endpoint's return type**, with no type to write, whenever the pager moves an input: the slot tells Rust which call it is. `by_link()` and `by_next_url(…)` move no input, so there the closure names the type of the page: `by_next_url(|page: &Linked<Invoice>| page.next.clone())`, `by_link().has_more(|page: &Batch<Review>| page.has_more)`. Without it, `rustc` reports ``type annotations needed for `&_` ``. A function of the page works everywhere: a method of the list type (`by_next_url(Linked::next_url)`, for a `fn next_url(&self) -> Option<String>`), or a method of a trait of your own that every list type of the API implements, so that the same line fits each list endpoint.

Seven of the shipped pagers are at the root of `concord` and in `concord::page`; `by_offset_total`, and `Paging`, the type that they all return, are in `concord::page`. They are ordinary items, and the macro treats none of them specially.

## The shipped pagers

Eight functions say how the next call is made. Each returns a `Paging`:

| Written | The next call | The list ends |
|---|---|---|
| `by_cursor(after, f)` | `after` takes the value that `f` finds in the page. | When `f` returns `None`. The length of a page is never looked at: Slack, DynamoDB and Google send short or empty pages in the middle of a list. |
| `by_offset(start)` | `start` grows by the number of items that arrived (an offset that is not set counts as 0). | On an empty page. |
| `by_offset_limit(start, count)` | The same, and the same `count`. | On an empty page, or on a page shorter than the `count` that the caller set. Use it only for an API that promises full pages until the last one. |
| `by_offset_total(start, f)` | The same as `by_offset`. `f` reads the total number of items from the page. | When `start` plus the items of this page reaches the total. Below it, the move must give a next call: an empty page there is an error ([below](#more-items-but-nothing-to-continue-with-is-an-error)), unless a pager chosen by `or_when_set`, such as a cursor, still gives one. |
| `by_page(page, first)` | `page + 1`. With no `page` set, the page just fetched was number `first`. | On an empty page. |
| `by_page_bounds(page, f)` | The reported current page number plus one, even if the server returned a different number from the one requested. `f` returns `Some((current, last))` in the type of `page`, or `None`. `last` is a page number, so a zero-based list of five pages has `last = 4`. No item count is needed. | When the bounds are missing or the current number has reached or passed the last. |
| `by_link()` | The same call, sent to the URL of `Link: <…>; rel="next"` (RFC 8288). | When the response has no such link. A `Link` field that cannot be parsed is a Mismatch, not the end. Only the syntax of a field is checked: its target is parsed the way `reqwest` parses a URL. |
| `by_next_url(f)` | The same call, sent to the URL that `f` reads from the page. | When `f` returns `None`. |

Seven methods of `Paging` add what the author knows about the API. Each returns the `Paging`, so they chain, in any order, and none needs an import:

| Method | What it means |
|---|---|
| `.has_more(\|page\| …)` | The page states whether more items follow: `false` ends the list, and `true` requires a next call. The function returns a `bool`; when the field can be absent, say in it what absence means (`page.is_truncated.unwrap_or(false)`). |
| `.stop_on_short_page(count, default)` | For a page that states nothing: a page with fewer items than the `count` that the caller set, or than `default` when the caller set none, is the last. |
| `.stop_on_short_page_of(size)` | The same, for an API whose page size is fixed and has no input. |
| `.only_when_set(input)` | When the call that fetched a page did not set `input`, that page is the last, and nothing else is read: a caller who does not set it gets one page. |
| `.or_when_set(input, other)` | When the caller set `input`, the pager `other` moves instead: for an API that pages in both directions, with one input for each (`starting_after` and `ending_before`). After each page, a call that sets two distinct inputs given to `or_when_set` is a Mismatch; of two `or_when_set` given one input, the first written moves. When `other` is a `Paging` with methods of its own, it decides by the same order on its own, inside the move: this value's statements, gate and short-page rule come first, so an outer "no more" ends the list without reading the inner statements, and an inner "no more" under an outer "more" leaves no next call (a Mismatch). |
| `.map_next(\|next, page\| …)` | Changes the next call before it is sent: a second marker that comes in a pair, or an input that the API refuses beside a cursor, cleared with its setter (`next.resource_version(None)`). |
| `.try_map_next(\|next, page\| …)` | Changes the next call or returns a `Fail`. The edit receives the endpoint's decoded page value. Use `mismatch("…")` when a value needed to edit the call is missing. If an edit fails, later edits are skipped. The caller gets the page, then the error. |

A function of your own becomes a `Paging` with `Paging::from_fn(…)`, and any pager with `Paging::from_pager(…)` ([a pager of your own](#a-pager-of-your-own)). `.into_fn()` turns a `Paging` back into a closure ([where a pager that is not a closure can be written](#where-a-pager-that-is-not-a-closure-can-be-written)).

## The order in which a page is decided

For each page, a `Paging` decides in this order, **whatever the order in which its methods were written**:

0. **The call.** The call that fetched the page is checked at each decision, before the gate. If it sets the inputs of two `or_when_set`, the pages end with a Mismatch after that page, whatever the page says: after the first page when the caller set both inputs, after a later page when a move itself sets a second input.
1. **The gate.** If one input given to `only_when_set` is not set on the call that fetched the page, this page is the last. Nothing else is read.
2. **What the page states.** Every statement (`has_more`, the total of `by_offset_total`, or the bounds of `by_page_bounds`) is read, once, in the order written, whatever the others say. If one cannot be read (a total that is not a number of items), its failure is the result. Otherwise, if one says "no more" and another "more", the pages end with a Mismatch. If all say "no more", this page is the last.
3. **The move.** The next call is computed: by the pager of the `or_when_set` whose input the call set, or else by the pager that the `Paging` was made from. Its failure (a `Link` field that cannot be parsed, a pager of your own that fails) is the result, also on a short page. A cursor chosen by `or_when_set` goes on past an empty page below a total, when it finds a next call. If a statement said "more" and there is no next call, the pages end with a Mismatch.
4. **With no statement at all,** a short-page rule (`stop_on_short_page`, `stop_on_short_page_of`, and the `count` of `by_offset_limit`) ends the list on a short page, after the move; otherwise the move decides, and no next call is the end. A statement always has authority over a short-page rule.
5. **The edits** of `map_next` and `try_map_next` run as one list, in the order written, only when there is a next call. The first failure skips the remaining edits.

So `.has_more(…).only_when_set(page)` and `.only_when_set(page).has_more(…)` are the same pager, and so is any other order of the methods: a program that writes descriptions may emit them in the order of its template. **The order written still matters inside three steps:** the edits of both methods run one after the other, as written; of two `or_when_set` given one input, the first written moves; and the statements are read in the order written, which two statements that share state of their own can see. The promise of order covers statements that read only the page.

Then the rules that hold for every pager, written or shipped: the next request may not repeat the one just sent (a Mismatch; only the one just sent: a list that alternates between two requests is not seen). The two requests are compared as they were built, after `edit_request` and before any middleware, with what a secret wrote in them left out; an `edit_request` that changes the URL loses what says where a secret was written in it, and the URL is then compared as it is. Nor may it go to another origin (a Build error) ([what holds for every page](#what-holds-for-every-page)).

## More items, but nothing to continue with, is an error

**A page that says that more items follow, and gives no next page, ends the list with an error, never in silence.** `has_more` returns `true` and the cursor is missing; the offset is below the total and the page is empty: the caller receives the page that arrived, then a Mismatch whose `reason()` is `the page says that more items follow, and gives no next page`. Two statements that disagree end it the same way (`the page's statements disagree on whether more items follow`). Without a statement, there is nothing to contradict: the move decides, and no next call is the end.

**`stop_on_short_page` is the one place where Concord ends a list on your word alone.** It is for an API whose pages state nothing, and that promises full pages until the last one: a page with fewer items than the size asked for is then the last, and Concord does not ask for one more empty page. `default` is your copy of the server's default page size, for a caller who sets no `count`. **If the server lowers that default, the list ends early, in silence:** a page of 10, where you wrote 20, looks short. If it raises it, the rule no longer ends anything early, and the list ends as the pager alone ends it: for `by_offset`, with one empty request more. A caller who sets `count` does not depend on the default, but a server that sends fewer items than the `count` asked for (its maximum is lower) ends the list there as well. When the page states something (`has_more`, a total, or bounds), the statement decides, and the short-page rule is not read.

## Shared pagination, written once as a part

**When an API paginates all its lists the same way, the pagination is written once, as a part.** A part is a named group of inputs together with the statements that place them. A node names it at the end of its inputs, and then has those inputs and statements as if it had written them itself ([a part](describing-an-api.md#a-part-the-same-inputs-on-many-endpoints)).

```rust
pub client Stripe(key: Secret) {
    …
    part page(limit?: u32, starting_after?: String) {
        query { limit; starting_after; }
        paginate by_cursor(starting_after, |page| page.data.last().map(|object| object.id.clone()))
            .has_more(|page| page.has_more);
    }

    scope customers {
        path "v1/customers";
        GET list(email?: String, ..page) -> List<Customer> { query { email; } }
    }
}
```

```rust
let mut pages = stripe.customers().list().email("a@b.example").limit(50).pages();
```

**Because a pager works on the whole call, a cursor works wherever its input goes:** in a query, in a header, or in a body (`body json { filter; start_cursor; }` with `paginate by_cursor(start_cursor, Results::next_cursor);`). Everything else in the request is kept, with nothing more to write.

## A pager of your own

**A pager is anything that implements `concord::page::Pager<C>`**: it takes the call that fetched a page and that page, and returns the next call, `None` at the end, or a failure. Every closure `|call, page| …` that returns `Option<C>`, or `Result<Option<C>, Fail<C::Problem>>`, is one, as in 5.0.0-rc.1:

```rust
paginate move |call, page| page.value().next_cursor().map(|cursor| call.starting_after(cursor));
```

`call` is the call that fetched this page. `page` is its response: `page.value()` is the value of the endpoint's return type, and `page.headers()` sits beside it. A pager written inline is a `move` closure, because what `paginate` receives must own what it captures; if you leave out `move`, `rustc` tells you to add it.

**A pager of your own can call a shipped one**, and do what no method says: here, refuse a page that timed out, which would otherwise skip documents. Make the shipped pager once, in a `let mut`, and call its `next_call` from the closure, with `concord::page::Pager` in scope:

```rust
use concord::page::Pager;
use concord::{by_cursor, Json};

concord::api! {
    pub client Index {
        base "https://index.example.test";
        read Json;

        GET search(q: String, after?: String) -> Hits {
            query { q; after; }
            paginate {
                let mut pager = by_cursor(after, |page| page.next.clone());
                move |call, page| {
                    if page.value().timed_out {
                        return Err(concord::mismatch("a page timed out"));
                    }
                    pager.next_call(call, page)
                }
            };
        }
    }
}
```

The `let` runs once, at `.pages()`, so the pager inside keeps its state for the whole walk through the list. A function that returns such a closure returns `impl FnMut(…)`, not `impl Fn(…)`: `next_call` takes the pager by `&mut`.

**A function of your own that should take the methods of a `Paging`** is wrapped with `Paging::from_fn`. Written in place, its closure names the type of the page (`page: &Answer<Changes>`): Rust learns the call only after the whole expression.

```rust
use concord::page::{Paging, Slot};
use concord::{Answer, Json};

concord::api! {
    pub client Drive {
        base "https://drive.example.test";
        read Json;

        /// The next token, until the page gives the token to start from next time.
        GET changes(page_token: String) -> Changes {
            query { "pageToken" = page_token; }
            paginate Paging::from_fn(move |call, page: &Answer<Changes>| {
                page.value().next_page_token.clone().map(|token| page_token.set(call, token))
            })
            .has_more(|page| page.new_start_page_token.is_none());
        }
    }
}
```

`Slot` stands for one input of the call: `get` reads it on a call, `set` returns the call with it changed, and `input` says which input of the endpoint it is (an `InputId`, its position among the inputs). `get` returns `None` when the input is optional and the call does not set it. A slot of your own that wraps another returns the `input()` of the slot it wraps. The fields of a call are not public: a pager reads an input through its slot, or sets it with its setter. `concord::page::Call` adds one method for APIs that hand out URLs: `call.at_url(url)`, the same call with the path and the query of that URL. It never changes the origin: a URL at an origin other than the one the call builds is a Build error, in pages and outside them. A URL on another host is a call of another description ([a URL that the API returned](extending.md#a-url-that-the-api-returned)).

A pager that cannot fail returns an `Option<C>`. A pager that fails ends the pages with one of three errors: with the API's problem, a Refused error that carries that problem and the status of the page that arrived; with `concord::mismatch(…)`, a Mismatch; with `Fail::from_read(…)`, an error that keeps the decoder's diagnostics, a Mismatch on a `2xx` and a Refused error on another status, and a generic Mismatch when the `ReadError` is not a decode failure (a body or a stream that broke) ([C5.8](../reference/contracts.md)).

**A type of your own** implements `Pager<C>` with its one method, `next_call(&mut self, call, page)`, to keep state or to wrap any other pager; `Paging::from_pager(MyPager(…))` then gives it the methods. A box of your own type is not a pager by itself: write `Box<dyn Pager<C>>`, given its type in a `let`, or implement `Pager<C>` for your box ([the rustdoc of `Pager`](../../concord-core/src/page.rs)). The shipped pagers are in [`concord-extras/src/page.rs`](../../concord-extras/src/page.rs) and [`concord-extras/src/page/paging.rs`](../../concord-extras/src/page/paging.rs): they use these, and `concord::page::fragmentless` (a URL without its fragment), which is public too. For a shorter pager of your own, without failure, see [a pager](extending.md#a-pager).

## Where a pager that is not a closure can be written

`paginate` reads its expression in one of two ways. **A closure is read as in earlier releases:** Rust infers the types of `call` and `page` from the endpoint. **Any other pager, such as a `Paging`, is taken as it is**, but only where the macro can be sure, before Rust has looked at a single type, that nothing in the expression needs that inference. It is sure when the expression is:

- a call or a method call: `by_cursor(after, …).has_more(…)`, `my_pager(after)`, `Box::new(by_link())`;
- a path to a value: `next_page`;
- a plain block whose last expression is one of these (`{ let pager = by_link(); pager }`), or an `if`/`else` or a `match` whose every branch is:

```rust
use concord::{by_cursor, by_page, Json};

concord::api! {
    /// Version 1 of the API numbers its pages; version 2 hands out a cursor.
    pub client Catalog(legacy: bool) {
        base "https://catalog.example.test";
        read Json;

        GET items(page?: u32, after?: String) -> Batch<Item> {
            query { page; after; }
            paginate if *client.legacy { by_page(page, 1) } else { by_cursor(after, |batch| batch.last_id.clone()) };
        }
    }
}
```

and nowhere in it is there a macro, an attribute (`#[cfg]` included), an item declared inside it (a `fn`, a `struct`), a `return`, a `?`, a loop, a label, or a `break` with a value. A closure passed as an argument, such as the one given to `has_more`, does not count, and neither do the `return`, `?` and loops of an `async` block: they are its own. A closure is welcome anywhere, but a macro inside it does count (`|page| page.next.as_ref().map(|s| format!("{s}"))`): write that function as a named one, or convert the pager. Even a call or a path may need a type written, as Rust needs it anyway: a generic constant (`Bank::<_>::FP`).

**Everything else is read as a closure**, as in 5.0.0-rc.1, so a description that uses only closures of its own compiles as before, whatever its shape, unless a closure returns a type of its own through an implementation of `IntoNext`, which this release refuses ([the changelog](../../CHANGELOG.md)). The rules that hold for every page apply to it as to any pager. A `Paging` in such an expression (after a `return` or a `break`, in a `loop`, under `#[cfg]`, out of a macro of yours, behind `*` or `.0`) is refused with Concord's own message:

```text
error[E0277]: `paginate` reads this expression as a closure, and its value is `Paging<shop::Call<…>>`, which is not a closure
note: if the value is a pager, write `.into_fn()` after a `Paging<…>` (`by_cursor(…).into_fn()`) or `Paging::from_pager(…).into_fn()` around a pager of yours; or write the pager as a call or a path, alone or at each tail of a plain block, `if` or `match`
note: if it is not, it is neither a closure nor a pager, and `paginate` cannot use it
```

What to write, in order of preference:

- **Move the choice into a function** of yours that returns the `Paging`, and call it: `paginate choose(after, before);`. A call is always taken as it is, and the function's return type, `Paging<C>`, keeps the methods for the caller of the function.
- **Write `.into_fn()` after the pager**, where the expression must stay as it is: `loop { break by_cursor(…).into_fn(); }`, `my_pager!(after).into_fn()`. A macro of yours that itself writes `return` converts inside its own expansion: a suffix on its invocation comes too late. A pager type of your own in such an expression is converted the same way: `Paging::from_pager(my_pager!()).into_fn()`. `Paging::from_pager(…)` alone does not help there: the expression inside it is still one that `paginate` reads as a closure.

**`.into_fn()` consumes a `Paging` and returns a closure**, which decides each page by the rules of that `Paging`, and only those. Attach `has_more`, `stop_on_short_page` and the other methods before it: `by_cursor(…).has_more(…).into_fn()`. When that closure is wrapped again with `Paging::from_fn`, the outer `Paging` is a separate decision: its rules come first, and it sees the closure only as a move, whose rules it does not know: an outer "no more" ends the list without reading the inner statements, and an outer short-page rule can end the list although an inner statement says that more items follow.

A value that is neither a closure nor a pager gets the same message, or, where the expression is a call or a path, ``error[E0277]: `u32` is not a pager for this endpoint``, with the note ``a pager is a closure `|call, page| …` that returns the next call, or a value such as `by_cursor(…)` ``.

## Upgrading from 5.0.0-rc.1

**A shipped pager is no longer a function.** In 5.0.0-rc.1, `by_offset`, `by_offset_limit`, `by_page`, `by_cursor`, `by_link` and `by_next_url` returned closures; they now return a `Paging<C>`, which is a pager and not a function. In `paginate`, written as a call, nothing changes. Code that used one as a function stops compiling, with one of these messages:

| The message | What was written | What to write |
|---|---|---|
| ``error[E0618]: expected function, found `Paging<…>` `` | A shipped pager called: `pager(call, page)` | `pager.next_call(call, page)`, with `use concord::page::Pager;`, on a `let mut pager = by_cursor(…);` that the closure owns ([a pager of your own](#a-pager-of-your-own)). A function that returns that closure returns `impl FnMut(…)`, not `impl Fn(…)`. |
| ``error[E0277]: expected an `Fn(C, &Answer<…>)` closure, found `Paging<C>` `` at the return type of a function of yours | A function that returned a shipped pager as `impl Fn(C, &Answer<T>) -> Option<C>` | Return `Paging<C>` (the caller can then add methods) or `impl Pager<C>`. `paginate my_pager(…);` takes either. |
| ``error[E0277]: expected an `FnMut(…)` closure, found `Paging<…>` `` (or `Fn`) at a parameter of a function of yours | A shipped pager passed to a function that takes `impl FnMut(C, &Answer<T>) -> Option<C>` | A parameter of type `impl Pager<C>`, called with `next_call`; or `.into_fn()` on the pager, with the function's types changed to `Result<Option<C>, Fail<C::Problem>>`, which is what `.into_fn()` returns. |
| The same E0277, at a `Box<dyn Fn(…)>` or `Box<dyn FnMut(…)>` | A shipped pager in a box of a function type, alone or beside a closure | A box of a pager, given its type in a `let`: `let pager: Box<dyn Pager<C>> = if … { Box::new(by_cursor(…)) } else { Box::new(Paging::from_fn(\|call: C, page: &Answer<T>\| …)) }; pager`. The closure names the types of its parameters. |
| ``error[E0277]: `paginate` reads this expression as a closure, and its value is `Paging<…>`, which is not a closure`` | A shipped pager in an expression that `paginate` reads as a closure, also one whose callback holds a macro (`by_cursor(after, \|page\| … format!(…) …)`) | [Where a pager that is not a closure can be written](#where-a-pager-that-is-not-a-closure-can-be-written): a named function for the callback, or `.into_fn()` after the pager. |
| ``error[E0277]: expected an `FnMut(…)` closure, found `Paging<…>` ``, in `paginate` itself | A shipped pager in a branch or an arm beside a closure, also in an arm under a `#[cfg]` that is off (`match … { #[cfg(any())] _ => \|call, page\| …, _ => by_cursor(…) }`): a closure at a value position keeps `rustc`'s own message | `.into_fn()` after the shipped pager, or a closure in every branch. |
| ``error[E0277]: expected an `Fn(…)` closure, found `Paging<…>` `` at an `Arc<dyn Fn(…)>` | One shipped pager shared by every walk: `let shared: Arc<dyn Fn(…)> = Arc::new(by_link());` | A pager keeps the state of one walk: make a new one for each, with a function: `fn make() -> Paging<C> { by_link() }` and `paginate make();`. |

**Two limits of `use concord::page::*;`.** `Pager` is implemented for every closure, so a glob import brings its method `next_call` into scope wherever a closure is. If a trait of yours also has a method named `next_call`, implemented for closures, two things can happen. On a closure, the call is ambiguous: ``error[E0034]: multiple applicable items in scope``, with `rustc`'s own repair. On a box of a closure, `rustc` finds Concord's method first and calls it **without a word**, and your pager behaves differently. In both cases, call your method by its trait, as Rust does for any two traits with one method name: `LocalMove::next_call(&mut f, call, page)` for a closure `f`, and `LocalMove::next_call(&mut *f, call, page)` for a box `f: Box<dyn …>`, whose trait is implemented for what it holds. Where `Pager` is not in scope (import only the names that you use, such as `use concord::page::Paging;`), neither can happen.

## What the caller receives

| How the list ends | What `try_next()` returns |
|---|---|
| The pager returns no next call: a `None`, a statement that says "no more", a closed gate, a short page | The last page, as `Ok(Some(page))`; then `Ok(None)`. |
| The pager fails with `concord::mismatch(…)`, or a shipped pager finds the page wrong (it says more and gives no next page, its statements disagree, a total or a `Link` field cannot be read, an offset or a page number cannot grow, the call sets the inputs of two `or_when_set`) | The page that arrived, as `Ok(Some(page))`; then a Mismatch, printed `mismatch: status 200, the next page cannot be requested, in shop.customers`, whose `reason()` says why ([the messages](../reference/messages.md#2-when-a-request-is-built-or-answered)); then `Ok(None)`. |
| The pager fails with the API's problem | The page that arrived; then a Refused error with that problem and the status of the page (`refused: status 200, in shop.customers`); then `Ok(None)`. |
| The pager fails with `Fail::from_read(…)` | The page that arrived; then the error that a reader gives for that page ([C5.8](../reference/contracts.md)); then `Ok(None)`. |
| The next request is the request just sent | The page that arrived; then a Mismatch, `the next page cannot be requested`, with nothing sent; then `Ok(None)`. |
| The next request goes to another origin | The page that arrived; then a Build error, `the URL leaves the origin of the call`, with nothing sent; then `Ok(None)`. |
| The next request fails (a status with no arm, a timeout, a body that does not decode) | Every page before it; then that call's error; then `Ok(None)`. |

**An error about the next page never takes away a page that already arrived,** and after an error nothing more is sent. To continue later, call the endpoint again from the last page that you hold.

## What holds for every page

| | |
|---|---|
| One page per `try_next()` | Each is one call, with the retry policy and both timeouts of the call ([timeouts](calling.md#timeouts)). Its request is built when the page is requested, so a `Secret` replaced between two pages is used, and the call timeout, `timeout`, covers building the request. Nothing is fetched ahead, and nothing is sent before the first `try_next()`. The pager runs once for each page, before that page is handed to the caller. The time that you spend between two pages is not counted, and nothing bounds the whole loop. |
| The end is the author's statement | What ends a list is what the description says: no next call, a statement of the page, or a short-page rule that the author wrote. A cursor never looks at the length of a page. |
| Never the same request twice in a row | If the next call would repeat the request just sent, the pages end with a Mismatch. "The same" means the method, the URL without its fragment, the bytes of the body, and the headers, each value in its order. The text of a secret in the URL or in a header is left out of the comparison: in the path, in a query pair, or in a header value. A body is compared by its bytes, and a `Body::repeatable` by its factory and map identities and buffered parts, without opening a source for the comparison. So an API that paginates through a header moves forward, and a key that was replaced does not look like progress, even when it is in the path. A header value is left out when it has the `sensitive` flag, which Concord sets for a secret: a value that your `edit_request` marks is left out too (C8.4 of [the contracts](../reference/contracts.md)). The comparison is of the requests as Concord builds them: a transport that follows redirects or rewrites the URL itself (`concord::Http` does neither) can hide a repeat, for as long as it does so: the same page can then arrive again and again. A longer cycle, or a list that never ends, is not detected ([scope and limits](../design/scope.md)). |
| A next-page URL | Its path and its query are sent as the server gave them, without the fragment. An empty query is a query: `/slice?` is sent as `/slice?`, not as `/slice`, and `?` on the path just requested is not that request. A relative URL is resolved against the URL of the response that contained it; when there is no response (your own `call.at_url("../next")`), against the URL that the call builds. A query pair from the description is not added again, except one whose value is a `Secret`: every pair with that key already in the URL is removed, and the secret is written once. |
| Another origin | Every page of one `.pages()` goes to the origin of its first request, whatever the pager did. Another origin is a Build error, because the call carries your credentials. |
| A `try_next()` that is dropped | Nothing is lost, and the drop sends nothing again: the next `try_next()` continues with the request in flight. |
| `.pages().items()` | The same pages, one item at a time. It requires the return type to implement one trait, `concord::page::IntoItems` ([below](#a-page-type-of-your-own-intoitems)). `Option` and `Result` do not implement it, so no domain error is lost inside an iterator. |
| A body that is a stream | A paginated call whose body is a `Body` that can be read only once (an input made by `Body::stream`) fails at its second page with a Build error: the first page consumed the stream, and nothing of the second page is sent. A one-shot stream inside a combined body (a `multipart`) behaves the same. An expression that creates a new stream each time it runs gives each page its own, and a `Body::repeatable` is opened again for each page ([a body that is opened again](bodies.md#a-body-that-is-opened-again-for-each-attempt)). |
| Not provided | "Collect everything", fetching pages in parallel, a total count for the caller, the head of each page. A loop is three lines. |

## A page type of your own: `IntoItems`

`.pages().items()`, the counting pagers (`by_offset`, `by_offset_limit`, `by_offset_total`, `by_page`) and the short-page rules require `concord::page::IntoItems` on the return type. It is implemented for `Vec<T>`. For your own list type, you implement it:

```rust
// in `concord::page`
pub trait IntoItems: Send {
    type Item: Send;
    fn into_items(self) -> Vec<Self::Item>;     // the items, in the order of the provider
    fn count(&self) -> usize;                   // how many this page holds
}

// yours, beside the macro
#[derive(Deserialize)]
pub struct List<T> { pub data: Vec<T>, pub has_more: bool }

impl<T: Send> concord::page::IntoItems for List<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> { self.data }
    fn count(&self) -> usize { self.data.len() }
}
```

`.items()` gets the items with `into_items`; a counting pager and a short-page rule call `count`. A page and its items are `Send`, so the only bound to write is `T: Send`. Calling `.items()` on a page type without the trait does not compile: `rustc` reports the error in Concord's words, saying that the type is not a list of items.

See [the engine page](../internals/engine.md) for how `Pages` works, and C8.4 to C8.7 of the [contracts](../reference/contracts.md) for the exact rules.
