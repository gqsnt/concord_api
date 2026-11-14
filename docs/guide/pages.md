# Pages

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Retries](retries.md) · next: [Testing a client](testing.md)*

For the author and the user of a client: a list that comes in several answers. The author writes one line on the endpoint; the caller gets `.pages()`.

The split: the description says how the API pages (`paginate by_offset(start);`), a pager (a Rust function from a call and its answer to the next call) makes the next call, and the caller chooses whether to loop: `.send()` for one page, `.pages()` for all.

## One line on the endpoint

How an API pages is a fact of the API, the same for every caller. So it is one line on the endpoint, and the call gets `.pages()`:

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

**A call to an endpoint with pages cannot be awaited directly.** `.send().await` gives one page, and says so; `.pages()` gives them all. A bare `.await` there does not compile (`rustc` says that the call `is not a future`), so nobody takes the first page for the whole list. On every other endpoint `.await` is unchanged.

## A pager is a function

**The next page is the same call with an input changed.** That is what a caller would write by hand, and it is what a pager is: a function from the call and its answer to the next call.

```rust
GET list(limit?: u32, starting_after?: String) -> List<Customer> {
    query { limit; starting_after; }
    paginate move |call, page| page.value().next_cursor().map(|cursor| call.starting_after(cursor));
}
```

`call` is the call that got this page. `page` is its answer: `page.value()` is the value after the arrow, and `page.headers()` is beside it. The function gives `Some(next call)`, or `None` at the end. A pager written in place is a `move` closure: what `paginate` gets must own what it holds, and without the word `rustc` says to add it.

## The shipped pagers

Six functions are shipped for the shapes that most APIs have. They are ordinary items of `concord`, and the macro knows none of them. Their names start with `by_`, because APIs name their inputs `offset`, `cursor` and `page`, and an input hides an item of its name in the lines of its node. (That is Rust's own rule. The item is then written with its path: `body concord::form(form);`.) **In `paginate`, an input is the input itself, as a place**, not its value: the function reads it on the call that was answered and sets it on the next one.

| Written | The next call | Ends when | Like |
|---|---|---|---|
| `paginate by_offset(start);` | `start` grows by the number of items that came. | A page is empty. | — |
| `paginate by_offset_limit(start, count);` | The same. | A page is empty, or shorter than the `count` that the caller gave. Write this only when the API promises it. | Riot |
| `paginate by_page(page, 1);` | `page + 1`. With no `page` given, the page that was answered was number 1: the second argument. | A page is empty. | Riot |
| `paginate by_cursor(starting_after, List::next_cursor);` | The input takes what your function finds in the page. | The function gives `None`. | Stripe, Slack, Notion |
| `paginate by_link();` | The same call, sent to the URL of `Link: <…>; rel="next"`. | There is no such link. A `Link` field that cannot be read is a Mismatch, not an end. Only the shape of a field is checked: its target is read as `reqwest` reads a URL. | GitHub, GitLab |
| `paginate by_next_url(Page::next);` | The same, with a URL that your function reads in the page. | The function gives `None`. | Twilio, OData |

## One way to page, written once

**An API pages all its lists in one way, so it is written once: a part.** A part is a named group of inputs with the statements that place them. A node names it at the end of its inputs, and then has those inputs and those statements as if it had written them ([a part](describing-an-api.md#a-part-the-same-inputs-on-many-endpoints)).

```rust
pub client Stripe(key: Secret) {
    …
    part page(limit?: u32, starting_after?: String) {
        query { limit; starting_after; }
        paginate by_cursor(starting_after, List::next_cursor);
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

**Because the unit is the call, a cursor works wherever the input goes:** in a query, in a header, or in a body (`body json { filter; start_cursor; }` with `paginate by_cursor(start_cursor, Batch::next_cursor);`). Everything else of the request is kept without a word.

## A pager of your own

**A pager of your own is written like the shipped ones, and it can refuse a page.** A page that says "there is more" and gives no cursor is not the end of the list. The pager says so, and the pages end with an error instead of looking complete. Its failure has a type to name: `concord::Fail`.

```rust
use concord::page::{Call, Slot};
use concord::{Answer, Fail};

/// `paginate more(start_cursor);`: the next cursor, as long as the page says that there is more.
pub fn more<C, S, T>(cursor: S) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
where
    C: Call<Value = Batch<T>>,
    S: Slot<C, Value = String>,
{
    move |call, answer| match (&answer.value().next_cursor, answer.value().has_more) {
        (Some(next), true) => Ok(Some(cursor.set(call, next.clone()))),
        (None, true)       => Err(concord::mismatch("has_more without a cursor")),
        _                  => Ok(None),
    }
}
```

`Slot` is the place of one input: `get` reads it on a call, `set` gives the call with it changed. `Call` adds one method for an API that hands out addresses: `call.at_url(url)`, the same call with the path and the query of that URL. It never gives an origin: a URL at another origin than the one that the call builds is a Build error, in pages and outside them. A pager that cannot fail gives an `Option<C>`. A pager that fails with the API's problem gives a Refused error, with the status of the page that came; one that fails with `concord::mismatch(…)` gives a Mismatch. The fields of a call are not public: a pager reads an input through its place, or sets it with its method. The shipped pagers are in [`concord-extras/src/page.rs`](../../concord-extras/src/page.rs): they use these, and `concord::page::fragmentless` (a URL without its fragment), which is public too. A shorter pager of your own, with no failure: [a pager](extending.md#a-pager).

## What holds for every page

| | |
|---|---|
| A page that arrived is always given | What the pager says about a page comes with the following `try_next()`: the next page, the end, or an error. An error about the next page never takes away a page that arrived. |
| One page for each `try_next()` | Each is one call, with the retry policy and the two clocks of the call ([time](calling.md#time)). Its request is built when the page is asked for: a `Secret` that was replaced between two pages is used, and `timeout` covers the building. Nothing is fetched ahead, and nothing is sent before the first one. The time that you spend between two pages is not counted, and nothing bounds the whole loop. |
| The end is the author's statement | A cursor never looks at the length of a page: Slack, DynamoDB and Google all send short or empty pages in the middle of a list. |
| Never the same request twice in a row | If the next call would be the request that was just sent, the pages end with a Mismatch. "The same" is the method, the URL without its fragment, the bytes of the body, and the headers, each value in its order. What a secret wrote in the URL or in a header is left out: a piece of a path, a pair of the query, a value of a header. A body is compared as its bytes. So an API that pages by a header moves, and a key that was replaced does not look like a move, also when it stands in the path. What marks a header value is the flag `sensitive`, which Concord sets for a secret: a value that your `edit_request` marks is left out too (C8.4 of [the contracts](../reference/contracts.md)). A longer cycle, or a list that never ends, is not seen ([scope and limits](../design/scope.md)). |
| A next URL | Its path and its query are sent as the server gave them, without the fragment. A relative one is resolved against the URL of the answer that gave it; with no answer (your own `call.at_url("../next")`), against the URL that the call builds. A query pair of the description is not added again, except one whose value is a `Secret`: every pair of that key that the URL already holds is taken out, and the secret is written once. |
| Another origin | Every page of one `.pages()` goes to the origin of its first request, whatever the pager did. Another origin is a Build error: the call carries your credentials. |
| After an error | `try_next()` gives the error, and the pages are over: the next `try_next()` gives `None`. To go on later, call the endpoint again from the last page that you hold. |
| `try_next()` that is dropped | Nothing is lost, and nothing is sent twice: the next `try_next()` goes on with the request in flight. |
| `.pages().items()` | The same pages, one item at a time. It asks one thing of the type after the arrow: `concord::page::IntoItems` ([below](#a-page-type-of-your-own-intoitems)). `Option` and `Result` do not have it, so no error of the domain is lost in an iterator. |
| A body that is a stream | A paged call whose body is one `Body` that can be read once (an input that is a stream) fails at its second page, with a Build error: the first page took the stream, and nothing of the second is sent. A stream inside a body made of bodies (a `multipart`) is the same. An expression that makes a new stream each time it runs gives each page its own. |
| Not there | "Collect everything", pages in parallel, a total count, the head of each page. A loop is three lines. |

## A page type of your own: `IntoItems`

`.pages().items()` and the counting pagers (`by_offset`, `by_offset_limit`, `by_page`) ask `concord::page::IntoItems` of the type after the arrow. It is written for `Vec<T>`. For a list type of your own, you write it:

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

`.items()` takes the items with `into_items`; a counting pager asks `count`. A page and its items are `Send`, so the one bound to write is `T: Send`. A page type without the trait gets Concord's words when `.items()` is asked of it: it is not a list of items.

How `Pages` works is on [the engine page](../internals/engine.md). The exact lines are C8.4 to C8.7 of the [contracts](../reference/contracts.md).
