# 6. Behaviour: who you are, retry, pages

*Concord V5 design · chapter 6 of 9 · [index](../README.md) · previous: [5. The response](5-response.md) · next: [7. Promises](7-promises.md)*

Three things that an API client must do and that are not "where a value goes". Each one is a Rust value with one job. The description only says the fact of the API behind it.

## 6.1 The split

| | The fact of the API, in the description | The behaviour, a Rust value | The choice of the application |
|---|---|---|---|
| **Who you are** | Where the credential goes: `headers { "x-riot-token" = key; }`. What signs each request: `middleware Signed::new(key.clone());` | A `Secret`. A middleware. | The credential itself, given as an input |
| **Retry** | This `POST` is safe to send twice: `idempotent;` | A `Retry` value | Whether to try again, and how hard: `with_retry(…)` |
| **Pages** | This endpoint has pages, and how the next one is asked: `paginate by_offset(start);` | A function from a call and its answer to the next call | To loop or not: `.send()` for one page, `.pages()` for all |

The description never holds a policy. Its author writes what is true of the API for every caller. How often to try again is different for a command-line tool and for a night job, so it is set where the client is built.

## 6.2 Who you are

**A credential that is a value is placed like any value.** A key, a bearer token, a user and a password: each is an input, and the description says where it goes, in the words it already has.

| The API wants | In the description |
|---|---|
| A key in a header | `headers { "x-riot-token" = key; }` |
| A bearer token | `headers { "authorization" = "Bearer {token}"; }` |
| HTTP Basic | `headers { "authorization" = basic(user, password); }` |
| A key in the query | `query { "api_key" = key; }` |
| A secret in the body (a token endpoint) | `body form { grant_type; client_id; client_secret; }`: a `Secret` may be an entry of a body, and is written as its text (section 5.8). |
| A client certificate | Nothing. It is a fact of the connection: set it on the HTTP client, `concord::Http::with(…)` (below). |
| A signature, a token that is renewed | `middleware Signed::new(key.clone());` (below). |

**What makes it a credential is its type.** The input is a `concord::Secret`, and a value that holds a `Secret` is secret wherever it is placed:

| | |
|---|---|
| In a header | The header is marked sensitive: `reqwest` and `http` do not print its value. |
| In a query or a path | It is then part of the URL. Concord never prints a URL. A `reqwest::Response` that you take with `send_raw()` shows its URL in `{:?}`. |
| To a next URL | A pair of the query whose value is a `Secret` goes with the call to a next URL too (`by_link()`, `by_next_url(…)`): a server does not write your key in its links. |
| In an error, a log, `{:?}` | Never. A `Secret` prints as `Secret(…)` and has no `Display`. |
| In a template | `"Bearer {token}"` with a secret in a hole is a secret as a whole. |
| Under another `base` | A secret does not follow a change of `base`. An entry, or a piece of a `path`, that holds a `Secret` is bound to the `base` in reach where it is written: the one of its node, or the nearest above. Only where none is in reach (a key on the client, above every `base`) is it bound to the first one below, on each branch: such a key follows to all of them. Under any other `base` further down, the description does not compile until that node writes the entry again (`override "authorization" = "Bearer {client.token}";`) or removes it. A `path` has no such word: it is moved, and written under each `base` that needs it. A caller's `with_base` moves everything: it is the caller's own word ([rules](rules.md), rule 16). |

**The credential is an input.** When every call needs it, it is an input of the client: `new` asks for it. When one endpoint needs another one (the player's own token in `me`, chapter 3), it is an input of that endpoint, which writes its own entry and removes the one above: `headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }`.

| | |
|---|---|
| A key that changes | A `Secret` can be replaced: `key.replace("the new key")`. Every client and handle that holds a clone of it sends the new one in each request that is built from then on. A request is built once for a call, so a try that waited sends the key that the call started with. |
| Basic | `basic(user, password)` gives a `Secret`: `Basic` and the pair in base64 (RFC 7617), written as UTF-8. |
| A value that cannot be a header | A line break in a token: a Build error, and nothing is sent. |
| `request()` | It shows the request with these values in it: they are part of what the description builds. |

There is no `auth` statement and no list of schemes: a scheme would do nothing more than place a value, and `headers` and `query` already do that.

**A signature, a token that is renewed, a quota: a middleware.** They are not a value: they are a process that runs on each request as it leaves. The description names it on the client, with `middleware`, and the author writes it beside the macro ([rules](rules.md), rule 26).

```rust
pub client Vault(key: Secret) {
    base "https://api.vault.example";
    read Json;
    middleware Signed::new(key.clone());        // every request is signed as it leaves

    GET balance() -> Balance { path "balance"; }
}

// plain Rust, beside the macro
pub struct Signed { key: Secret }

impl concord::Middleware for Signed {
    async fn handle(&self, mut request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        let text = format!("{} {}", request.method(), request.url().path());
        let signature = sign(&self.key, &text).map_err(|e| TransportError::blocked().with_source(e))?;
        request.headers_mut().insert("x-signature", signature);
        next.send(request, info).await          // on to the next middleware, then to what really sends
    }
}
```

```rust
let vault = Vault::new(key);                                   // it signs: there is no other way to make one
let test  = Vault::new("k").with_transport(concord::fake(…));  // the fake gets the signed request
```

| | |
|---|---|
| Where it stands | On the client, any number of times. |
| **The order** | A request goes first through the middlewares that a caller added, then through the ones of the description, in the order written, then to the transport. The answer comes back the same way. So the author's are the last before the wire, and **the author writes the one that signs last**: nothing changes a request after it was signed. A transport must not change it either. |
| When it is made | Once, in `new`. Every handle and every call of that client shares it, so it can hold what must be shared: a quota, a token that is renewed. Its expression sees the required inputs of the client, as references, and nothing else: the macro refuses an optional input that is named there. `new` cannot fail, so the expression has no `?`: what can fail is done in `handle`. |
| What a caller wants to read from it | A quota that the middleware keeps comes in as a required input of the client, and the caller keeps a clone. |
| When it runs | For **each** try and each page, on the request as it leaves: a request that is sent again is signed again. |
| What it gets | The request, a `TryInfo`, and `next`. `info.number()` is the number of the try. `info.endpoint()` is the name of the endpoint, and it is a promise: the names of the tree joined with `.` (`riot.matches.ids`), the same text as in a printed error. `next.send(request, info)` sends on. It may be called more than once, or not at all. |
| A middleware that waits | It is not a silent server: the clock for silence watches the transport and a body while it is read, not the middlewares (section 6.5). A quota that holds a request back, or a token that is fetched, does not meet it. Only `timeout` bounds a middleware. With no `timeout` it may wait for ever: that is its author's affair. |
| When it cannot sign | It gives `TransportError::blocked()`, with its cause as the source, and does not call `next`. The tries end there, and the result is that blocked error. Your cause is in `e.source()`, and is never printed. |
| Nothing takes it away | Not a node below, not a caller. An API whose public endpoints must not be signed has a middleware that looks at the request, or at `info.endpoint()`. |
| What it changes | Its author's affair. The checks of pages (the origin, "the same request") and of retry (the method) look at the request as it was built, before any middleware. A middleware that sends a request elsewhere is like `.edit_request(…)`: allowed, and its author's word. |
| A caller's own | `with_middleware(…)` on a client: a log, a limit of their own. It comes before every middleware of the description, so it cannot change a request after it was signed. |
| `request()` | `request()` and `into_request()` give the request before any middleware: a signature belongs to one try. |

**A token that your code renews** is the same shape, and it calls `next.send` twice. Concord has no rule of its own for a `401`.

```rust
impl concord::Middleware for Renewed {
    async fn handle(&self, mut request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        let again = request.try_clone();                      // nothing, for a body that is a stream
        self.write_token(&mut request);
        let answer = next.send(request, info.clone()).await?;
        match again {
            Some(mut copy) if answer.status() == 401 => {
                self.renew().await?;                          // yours: ask for a new token, and keep it
                self.write_token(&mut copy);
                next.send(copy, info).await
            }
            _ => Ok(answer),
        }
    }
}
```

Written before the signer (`middleware Renewed::new(…); middleware Signed::new(…);`), it is around it: the second send is signed too. The one that signs is written last.

**Two things, with one job each.**

| | Its job | How it is set |
|---|---|---|
| A middleware | What a request goes through on the way: a signature, a renewed token, a quota, a log. | `middleware m;` in the description; `with_middleware(…)` by a caller. |
| A transport | What really sends: `concord::Http`, a `reqwest::Client` of yours, a fake. What it does is counted by the two clocks. | `with_transport(…)` replaces it, **under** the middlewares. |

**`concord::Http`** is Concord's own HTTP client, and the transport of a client that was given none. It waits at most 30 seconds for a connection, follows no redirect (a `3xx` is an answer), and keeps no cookies. `Http::with(|builder| …)` adds a setting of yours (a certificate, a proxy). After the closure Concord sets one thing: no redirect. "No cookies" is a default: a caller whose build of `reqwest` has cookies may turn them on there.

```rust
let http = concord::Http::with(|builder| builder.add_root_certificate(cert))?;
let vault = Vault::new(key).with_transport(http);          // still signed: the middleware is above
```

A `reqwest::Client` that you bring is a transport too, and keeps its own settings. One that follows redirects sends the second request itself, below the middlewares: a signature that covers the path is no longer right. What `Http` sets is in [the engine page](engine.md), section 6.

## 6.3 Retry

**Off until you set it.**

```rust
let riot = Riot::new(key).with_retry(Retry::standard());     // for every call of this client
let game = asia.get(id).with_retry(Retry::none()).await?;         // for this call only
```

**Three questions decide whether a request goes out again.** All three must say yes ([rules](rules.md), rule 18; the exact conditions are C4.1 of the [contracts](contracts.md)).

| The question | The answer |
|---|---|
| **Can it be sent again?** | Yes, unless its body is a stream. |
| **May it be repeated?** | Yes, in three ways. Its method says that repeating it changes nothing more, as HTTP defines it (`GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`). Or its author wrote `idempotent;`. Or the API refused it with a status that the policy lists with `not_applied_on`. A `POST` that is not marked is not sent again after a connection that failed: its caller gets the error (`e.class()` says `Connect`), and decides. |
| **Is the failure worth another try?** | Yes for the failures of the network and the statuses that the standard table names (C4.2), and for what a rule of yours adds. |

`idempotent` is a fact of the API, so its author writes it. What follows the word is a Rust expression that gives a `bool`, with the inputs as variables. With nothing after the word, it means yes.

```rust
POST create(new: NewCustomer, idempotency_key: String) -> Customer {
    body form(new);
    headers { "idempotency-key" = idempotency_key; }
    idempotent;                    // this API promises: the same key, the same result
}

POST charge(new: NewCharge, key?: String) -> Charge {
    body form(new);
    headers { "idempotency-key" = key; }
    idempotent key.is_some();      // safe to repeat only when the caller gave a key
}
```

The rare `PUT` or `DELETE` that must not be sent twice says `idempotent false;`. An API where every `POST` takes a key writes the three lines once, as a part: `part keyed(idempotency_key: String) { headers { "idempotency-key" = idempotency_key; } idempotent; }`, and then `POST create(customer: NewCustomer, ..keyed)`. And it is the method of the request as it was built that counts, after the edits and before any middleware: if an `.edit_request(…)` turns a `GET` into a `POST`, that request is a `POST`.

**A status proves nothing about what the server did.** A `429` usually means "not now, and nothing was done", but only the API can promise it. When it does, the policy is told, by the author who knows:

```rust
// in the crate of the Slack client, beside the macro
pub fn retry() -> concord::Retry<SlackProblem> {
    concord::Retry::standard().not_applied_on(429)      // Slack turns a request away before it does anything
}

// in the application
let slack = Slack::new(token).with_retry(slack::retry());
```

A `POST` that is not idempotent then goes out again after a `429`, and only then. `idempotent;` is not the word for this: it would also send the message again after a lost connection, and post it twice.

**`Retry::standard()`** is a few tries, with a random wait that grows from one try to the next. (The random part matters: many clients that wait the same time come back together.) When the answer has `Retry-After`, the wait is never less than that. It is seconds or a date, and Concord reads both: a date in the past is a wait of zero, and one that cannot be read is as no `Retry-After`. A `Retry-After`, or a wait that a rule of yours asks for, over 60 seconds means no other try: the error is the result, and `e.retry_after()` says how long. It gives the longer of the two waits: the `Retry-After` of the answer, and what a rule of `when` asked for the try that did not happen. The waits are fixed, and so is the table of what is worth another try: the numbers are C4.2 and C4.3 of the [contracts](contracts.md). Each wait is logged, as one event ([chapter 7](7-promises.md), section 7.3).

**Your own.** `Retry::standard().tries(5)`, `not_applied_on(status)`, and rules of yours for what the standard table does not know: `when` and `unless`. Each gets the status, the headers and the API's own error, typed. A rule starts from the client's own `Retry` (`github::Retry`, `riot::Retry`, beside `Error` and `Result` in the module of the client), so that Rust knows the type of the problem.

```rust
// GitHub answers `403` with `retry-after` when a client goes too fast
let rule = github::Retry::standard().when(|tried| {
    let too_fast = tried.status().is_some_and(|s| s == 403);
    too_fast.then(|| tried.headers()?.retry_after()).flatten()
});

// a `503` that says "maintenance" is not worth waiting for
let rule = riot::Retry::standard().unless(|tried| tried.problem().is_some_and(|p| p.status.message == "maintenance"));
```

| | |
|---|---|
| `unless` | Asked first, every one of them. One `true` ends the tries, whatever the table and `when` say. |
| `when` | `Some(wait)` means: try again, and wait at least this long. `None` leaves the decision to the next rule, and then to the standard table. Its "yes" answers the third question only. |
| Rules add up | Inside one policy, the `when` rules are asked in the order they were added, and the first that gives a wait wins: the rules after it are not asked. A rule added later does not erase one that the author shipped. |
| `with_retry` on a call | It replaces the whole policy of the client for that call, `tries` and rules: that is the caller's own word. An author's policy is kept by passing it on: `with_retry(github::retry().when(mine))`. |
| When they are asked | Only when another try is still possible: tries are left, and the two other questions said yes ([engine](engine.md), section 3.2). |

**An API that says "later" in its own way.** Telegram writes the wait in the body. That is a fact of the API, and the place for it is a value that the author ships beside the description:

```rust
// in the crate of the Telegram client, beside the macro
pub fn retry() -> concord::Retry<TgProblem> {
    concord::Retry::standard().when(|tried| {
        let seconds = tried.problem()?.parameters.as_ref()?.retry_after?;
        Some(Duration::from_secs(seconds))
    })
}

// in the application
let bot = Telegram::new(token).with_retry(telegram::retry());
```

**What never changes:**

| | |
|---|---|
| A try is the whole exchange | The request out, the head in, and the body in when it is read whole. |
| The request is built once | The same bytes are handed to the transport again, as a copy for each try. |
| A stream that was handed over is outside | With `read Stream;` the try ends at the head. |
| When the tries run out | The last outcome is the result: a Refused error with status 429 and its headers, for example. `e.tries()` says how many tries were made: a try is counted when the request is handed to the middlewares. |
| A body that does not decode | Never tried again: it would be the same body. |
| What is sent again without Concord | `reqwest`'s own resend (a stream that the server refused before processing it) is left as `reqwest` has it. It is below what Concord counts: `tries()` does not see it. |
| `send_raw()` | The try ends at the head. A head below `400` is final, like a value. A head of `400` or above is looked at by the policy, as a refusal is; a rule of yours gets no `problem()`: nothing of the body is read. |

**The line with rate limiting.** *Reacting* to one answer is retry. *Counting ahead* is rate limiting: slowing down before the server complains, sharing a quota between tasks, a budget of retries for a whole program. That is outside, and its door is a middleware (section 6.6).

## 6.4 Pages

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

**A call to an endpoint with pages cannot be awaited directly.** `.send().await` gives one page, and says so; `.pages()` gives them all. A bare `.await` there does not compile, so nobody takes the first page for the whole list. On every other endpoint `.await` is unchanged.

**The next page is the same call with an input changed.** That is what a caller would write by hand, and it is what a pager is: a function from the call and its answer to the next call.

```rust
GET list(limit?: u32, starting_after?: String) -> List<Customer> {
    query { limit; starting_after; }
    paginate move |call, page| page.value().next_cursor().map(|cursor| call.starting_after(cursor));
}
```

`call` is the call that got this page. `page` is its answer: `page.value()` is the value after the arrow, and `page.headers()` is beside it. The function gives `Some(next call)`, or `None` at the end. A pager written in place is a `move` closure: what `paginate` gets must own what it holds, and without the word `rustc` says to add it.

Six functions are shipped for the shapes that most APIs have. They are ordinary items of `concord`, and the macro knows none of them. Their names start with `by_`, because APIs name their inputs `offset`, `cursor` and `page`, and an input hides an item of its name in the lines of its node. (That is Rust's own rule. The item is then written with its path: `body concord::form(form);`.) **In `paginate`, an input is the input itself, as a place**, not its value: the function reads it on the call that was answered and sets it on the next one.

| Written | The next call | Ends when | Like |
|---|---|---|---|
| `paginate by_offset(start);` | `start` grows by the number of items that came. | A page is empty. | — |
| `paginate by_offset_limit(start, count);` | The same. | A page is empty, or shorter than the `count` that the caller gave. Write this only when the API promises it. | Riot |
| `paginate by_page(page, 1);` | `page + 1`. With no `page` given, the page that was answered was number 1: the second argument. | A page is empty. | Riot |
| `paginate by_cursor(starting_after, List::next_cursor);` | The input takes what your function finds in the page. | The function gives `None`. | Stripe, Slack, Notion |
| `paginate by_link();` | The same call, sent to the URL of `Link: <…>; rel="next"`. | There is no such link. A `Link` field that cannot be read is a Mismatch, not an end. Only the shape of a field is checked: its target is read as `reqwest` reads a URL. | GitHub, GitLab |
| `paginate by_next_url(Page::next);` | The same, with a URL that your function reads in the page. | The function gives `None`. | Twilio, OData |

**An API pages all its lists in one way, so it is written once: a part.** A part is a named group of inputs with the statements that place them. A node names it at the end of its inputs, and then has those inputs and those statements as if it had written them ([grammar](grammar.md)).

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

`Slot` is the place of one input: `get` reads it on a call, `set` gives the call with it changed. `Call` adds one method for an API that hands out addresses: `call.at_url(url)`, the same call with the path and the query of that URL. It never gives an origin: a URL at another origin than the one that the call builds is a Build error, in pages and outside them. A pager that cannot fail gives an `Option<C>`. A pager that fails with the API's problem gives a Refused error, with the status of the page that came; one that fails with `concord::mismatch(…)` gives a Mismatch. The fields of a call are not public: a pager reads an input through its place, or sets it with its method. The shipped pagers are in [`concord-extras/src/page.rs`](../api/concord-extras/src/page.rs): they use these, and `concord::page::fragmentless` (a URL without its fragment), which is public too.

| | |
|---|---|
| A page that arrived is always given | What the pager says about a page comes with the following `try_next()`: the next page, the end, or an error. An error about the next page never takes away a page that arrived. |
| One page for each `try_next()` | Each is one call, with the retry policy and the two clocks of the call. Its request is built when the page is asked for: a `Secret` that was replaced between two pages is used, and `timeout` covers the building. Nothing is fetched ahead, and nothing is sent before the first one. The time that you spend between two pages is not counted, and nothing bounds the whole loop. |
| The end is the author's statement | A cursor never looks at the length of a page: Slack, DynamoDB and Google all send short or empty pages in the middle of a list. |
| Never the same request twice in a row | If the next call would be the request that was just sent, the pages end with a Mismatch. "The same" is the method, the URL without its fragment, the bytes of the body, and the headers, each value in its order. What a secret wrote in the URL or in a header is left out: a piece of a path, a pair of the query, a value of a header. A body is compared as its bytes. So an API that pages by a header moves, and a key that was replaced does not look like a move, also when it stands in the path. What marks a header value is the flag `sensitive`, which Concord sets for a secret: a value that your `edit_request` marks is left out too (C8.4). A longer cycle, or a list that never ends, is not seen ([chapter 8](8-scope.md), section 8.2). |
| A next URL | Its path and its query are sent as the server gave them, without the fragment. A relative one is resolved against the URL of the answer that gave it; with no answer (your own `call.at_url("../next")`), against the URL that the call builds. A query pair of the description is not added again, except one whose value is a `Secret`: every pair of that key that the URL already holds is taken out, and the secret is written once. |
| Another origin | Every page of one `.pages()` goes to the origin of its first request, whatever the pager did. Another origin is a Build error: the call carries your credentials. |
| After an error | `try_next()` gives the error, and the pages are over: the next `try_next()` gives `None`. To go on later, call the endpoint again from the last page that you hold. |
| `try_next()` that is dropped | Nothing is lost, and nothing is sent twice: the next `try_next()` goes on with the request in flight. |
| `.pages().items()` | The same pages, one item at a time. It asks one thing of the type after the arrow: `concord::page::IntoItems` (below). `Option` and `Result` do not have it, so no error of the domain is lost in an iterator. |
| A body that is a stream | A paged call whose body is one `Body` that can be read once (an input that is a stream) fails at its second page, with a Build error: the first page took the stream, and nothing of the second is sent. A stream inside a body made of bodies (a `multipart`) is the same. An expression that makes a new stream each time it runs gives each page its own. |
| Not there | "Collect everything", pages in parallel, a total count, the head of each page. A loop is three lines. |

**What a page is: `concord::page::IntoItems`.** `.pages().items()` and the counting pagers (`by_offset`, `by_offset_limit`, `by_page`) ask it of the type after the arrow. It is written for `Vec<T>`. For a list type of your own, you write it:

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

How `Pages` works is in [the engine page](engine.md), section 7. The exact lines are C8.4 to C8.7 of the [contracts](contracts.md).

## 6.5 One call: its steps and its two clocks

| Step | What happens | If it fails |
|---|---|---|
| 1 | The expressions of the description run: `idle_timeout`, then `idempotent`, then the request is built from the inputs, by kind (the base, the paths, the query, the headers, the body). Then the caller's `.edit_request(…)` changes it. This happens once for a send, and each page is one send. | Build error. Nothing was sent. |
| 2 | For each try, a copy goes through the middlewares: the caller's, then the description's in the order written (one writes a token, the last one signs). Then it is handed to the transport, which sends it. | No answer: the try ended. A middleware or a transport that will not send: no more tries, and the result is that blocked error. |
| 3 | The head arrives. The status picks an arm (section 5.2), the reader reads the body if the arm binds it, and the arm gives the result. | A Refused error (also when an arm on a status that is not a 2xx binds a `body` that does not read), a Mismatch, or the try ended if the body broke. |
| 4 | Retry looks at how the try ended. | Another try from step 2, or the result. |

The other expressions have their own moment: `read` runs for each answer, the expression of `paginate` once, at `.pages()`, and a `middleware` expression once, in `new`. Inside a kind, the order is the order of the tree ([rules](rules.md), rule 10). How the engine does this is in [the engine page](engine.md), sections 2 and 3. Step 3 alone is `call.read_response(response)`: the arms, the reader and the problem type of the endpoint, on an answer that you got yourself with `call.request()?`. It has no retry, no clock and no page, and it takes `&self`: a call can read several answers.

**Two clocks.** `timeout` is the whole call: no limit until you give one, and no request is handed to the transport after it has ended. `idle_timeout` is the clock for silence: 30 seconds, until something says otherwise. Both are set with `with_…`, on the client or on a call, and neither changes the other.

**The clock for silence watches two things, and nothing else.** The transport, from the moment it gets the request until the head of the answer: each piece of a stream body on its way out starts the clock again. And a body while it is read: each piece starts it again. A middleware's time, before or after the transport, and a reader's own waits are never silence: only `timeout` bounds them. *Silent:* a body held in memory goes out in one piece, so a large one on a slow line needs a longer clock.

**An endpoint that is silent on purpose says so.** A long poll, or a stream that is quiet for minutes, is a fact of an endpoint, and its callers should not have to know it. One statement gives the clock for silence of the calls below it:

```rust
GET get_updates(offset?: i64, timeout?: u32) -> Vec<Update> {
    path "getUpdates";
    query { offset; timeout; }
    idle_timeout Duration::from_secs(u64::from(timeout.unwrap_or(0)) + 10);
}
```

`idle_timeout <expression>;` stands on an endpoint, a scope or the client (a part may hold it too), and the nearest one is used. The expression is Rust with the inputs as variables, and gives a `std::time::Duration`. The order: `with_idle_timeout` on the call, then this statement, then `with_idle_timeout` on the client, then 30 seconds. (Here `timeout` is Telegram's own parameter.)

**A refusal is not lost to a clock.** When a clock ends a try whose status already said no, the result is that Refused error, and `kept()` says that its body was interrupted. How the engine holds the clocks is in [the engine page](engine.md), section 4. Setting them is [chapter 4](4-usage.md), section 4.6.

**A drop.** A call that is dropped stops, and Concord starts no other try. What the HTTP library does with the connection is its own: the request may have reached the server.

## 6.6 What stays outside, and its door

| Outside | Why | The door |
|---|---|---|
| A signer; a token that is renewed; the flows that get a token (OAuth2 and the like) | A process on each request as it leaves, with state of its own; and requests to another API. Concord has the place for it, and ships none. | A middleware (section 6.2): the author's, in the description. It sees every try and every answer. |
| Rate limiting, a quota shared by all calls, a budget of retries | State shared by every call, ahead of the answers. | A middleware: the author's for a quota of the API, a caller's (`with_middleware`) for a budget of their own. It sees the endpoint and the number of each try, it can wait (its wait is not silence), and it can refuse to send. |
| A cache of answers (`ETag` and the rest) | It needs storage. | `send_if_none_match(etag)` on any `GET` or `HEAD`: it gives an `Answer<Option<T>>`, `None` on a `304`, with the headers in both cases (section 5.5). |
