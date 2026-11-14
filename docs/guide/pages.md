# Pages

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Retries](retries.md) · next: [Testing a client](testing.md)*

This page covers lists that arrive over several responses. The author writes one line on the endpoint; the caller gets `.pages()`.

The work is split three ways. The description says how the API paginates (`paginate by_offset(start);`). A pager, a Rust function from a call and its response to the next call, builds the next call. The caller chooses whether to loop: `.send()` for one page, `.pages()` for all of them.

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

## A pager is a function

**The next page is the same call with one input changed.** That is what a caller would write by hand, and it is exactly what a pager is: a function from the call and its response to the next call.

```rust
GET list(limit?: u32, starting_after?: String) -> List<Customer> {
    query { limit; starting_after; }
    paginate move |call, page| page.value().next_cursor().map(|cursor| call.starting_after(cursor));
}
```

`call` is the call that fetched this page. `page` is its response: `page.value()` is the value of the endpoint's return type, and `page.headers()` sits beside it. The function returns `Some(next call)`, or `None` at the end. A pager written inline is a `move` closure, because what `paginate` receives must own what it captures; if you leave out `move`, `rustc` tells you to add it.

## The shipped pagers

Six functions are shipped for the shapes that most APIs use. They are ordinary items of `concord`, and the macro treats none of them specially. Their names start with `by_` because APIs often name their inputs `offset`, `cursor` and `page`, and an input shadows an item of the same name within its node. (That is Rust's own rule. The item is then written with its path: `body concord::form(form);`.) **In `paginate`, an input refers to its parameter on the call**, not to its value: the function reads it on the call that fetched the page and sets it on the next one.

| Written | The next call | Ends when | Used by |
|---|---|---|---|
| `paginate by_offset(start);` | `start` grows by the number of items received. | A page is empty. | — |
| `paginate by_offset_limit(start, count);` | The same. | A page is empty, or shorter than the `count` that the caller gave. Use this only when the API guarantees it. | Riot |
| `paginate by_page(page, 1);` | `page + 1`. When no `page` was given, the page just fetched was number 1: the second argument. | A page is empty. | Riot |
| `paginate by_cursor(starting_after, List::next_cursor);` | The input takes the value that your function finds in the page. | The function returns `None`. | Stripe, Slack, Notion |
| `paginate by_link();` | The same call, sent to the URL of `Link: <…>; rel="next"`. | There is no such link. A `Link` field that cannot be parsed is a Mismatch, not an end. Only the syntax of a field is checked: its target is parsed the way `reqwest` parses a URL. | GitHub, GitLab |
| `paginate by_next_url(Page::next);` | The same, with a URL that your function reads from the page. | The function returns `None`. | Twilio, OData |

## Shared pagination, written once as a part

**When an API paginates all its lists the same way, the pagination is written once, as a part.** A part is a named group of inputs together with the statements that place them. A node names it at the end of its inputs, and then has those inputs and statements as if it had written them itself ([a part](describing-an-api.md#a-part-the-same-inputs-on-many-endpoints)).

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

**Because a pager works on the whole call, a cursor works wherever its input goes:** in a query, in a header, or in a body (`body json { filter; start_cursor; }` with `paginate by_cursor(start_cursor, Batch::next_cursor);`). Everything else in the request is kept, with nothing more to write.

## A pager of your own

**Your own pager is written like the shipped ones, and it can reject a page.** A page that says "there is more" but gives no cursor is not the end of the list. The pager reports this, and the pages end with an error instead of looking complete. Its failure type is `concord::Fail`.

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

`Slot` stands for one input of the call: `get` reads it on a call, and `set` returns the call with it changed. `Call` adds one method for APIs that hand out URLs: `call.at_url(url)`, the same call with the path and the query of that URL. It never changes the origin: a URL at an origin other than the one the call builds is a Build error, in pages and outside them. A pager that cannot fail returns an `Option<C>`. A pager that fails with the API's problem produces a Refused error, with the status of the page that arrived; one that fails with `concord::mismatch(…)` produces a Mismatch. The fields of a call are not public: a pager reads an input through its slot, or sets it with its setter. The shipped pagers are in [`concord-extras/src/page.rs`](../../concord-extras/src/page.rs): they use these, and `concord::page::fragmentless` (a URL without its fragment), which is public too. For a shorter pager of your own, without failure, see [a pager](extending.md#a-pager).

## What holds for every page

| | |
|---|---|
| A page that arrived is always returned | What the pager decides about a page comes with the following `try_next()`: the next page, the end, or an error. An error about the next page never takes away a page that already arrived. |
| One page per `try_next()` | Each is one call, with the retry policy and both timeouts of the call ([timeouts](calling.md#timeouts)). Its request is built when the page is requested, so a `Secret` replaced between two pages is used, and the call timeout, `timeout`, covers building the request. Nothing is fetched ahead, and nothing is sent before the first `try_next()`. The time that you spend between two pages is not counted, and nothing bounds the whole loop. |
| The end is the author's statement | A cursor never looks at the length of a page: Slack, DynamoDB and Google all send short or empty pages in the middle of a list. |
| Never the same request twice in a row | If the next call would repeat the request just sent, the pages end with a Mismatch. "The same" means the method, the URL without its fragment, the bytes of the body, and the headers, each value in its order. The text of a secret in the URL or in a header is left out of the comparison: in the path, in a query pair, or in a header value. A body is compared by its bytes. So an API that paginates through a header moves forward, and a key that was replaced does not look like progress, even when it is in the path. A header value is left out when it has the `sensitive` flag, which Concord sets for a secret: a value that your `edit_request` marks is left out too (C8.4 of [the contracts](../reference/contracts.md)). A longer cycle, or a list that never ends, is not detected ([scope and limits](../design/scope.md)). |
| A next-page URL | Its path and its query are sent as the server gave them, without the fragment. A relative URL is resolved against the URL of the response that contained it; when there is no response (your own `call.at_url("../next")`), against the URL that the call builds. A query pair from the description is not added again, except one whose value is a `Secret`: every pair with that key already in the URL is removed, and the secret is written once. |
| Another origin | Every page of one `.pages()` goes to the origin of its first request, whatever the pager did. Another origin is a Build error, because the call carries your credentials. |
| After an error | `try_next()` returns the error, and the pages are over: the next `try_next()` returns `None`. To continue later, call the endpoint again from the last page that you hold. |
| A `try_next()` that is dropped | Nothing is lost, and nothing is sent twice: the next `try_next()` continues with the request in flight. |
| `.pages().items()` | The same pages, one item at a time. It requires the return type to implement one trait, `concord::page::IntoItems` ([below](#a-page-type-of-your-own-intoitems)). `Option` and `Result` do not implement it, so no domain error is lost inside an iterator. |
| A body that is a stream | A paginated call whose body is a `Body` that can be read only once (an input that is a stream) fails at its second page with a Build error: the first page consumed the stream, and nothing of the second page is sent. A stream inside a combined body (a `multipart`) behaves the same. An expression that creates a new stream each time it runs gives each page its own. |
| Not provided | "Collect everything", fetching pages in parallel, a total count, the head of each page. A loop is three lines. |

## A page type of your own: `IntoItems`

`.pages().items()` and the counting pagers (`by_offset`, `by_offset_limit`, `by_page`) require `concord::page::IntoItems` on the return type. It is implemented for `Vec<T>`. For your own list type, you implement it:

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

`.items()` gets the items with `into_items`; a counting pager calls `count`. A page and its items are `Send`, so the only bound to write is `T: Send`. Calling `.items()` on a page type without the trait does not compile: `rustc` reports the error in Concord's words, saying that the type is not a list of items.

See [the engine page](../internals/engine.md) for how `Pages` works, and C8.4 to C8.7 of the [contracts](../reference/contracts.md) for the exact rules.
