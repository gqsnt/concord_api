# The engine: how one call runs

*[Concord](../../README.md) · [docs](../README.md) · internals · next: [the macro](macro.md)*

For someone who wants to change `concord-core`, or to know what a call really does: the values the engine holds, the path of one call, the two clocks, what a drop does, and how pages keep a request in flight, with the names of the code.

[The contracts](../reference/contracts.md) say **what** the engine must do. This page says **how** the engine in [`concord-core`](../../concord-core/src) does it. A name that is not public is one of the hidden module [`__private`](../../concord-core/src/__private/mod.rs).

The engine needs two things from outside: `reqwest`, and Tokio's timer (`tokio::time`). **Sending a call needs a Tokio 1.x runtime with its time and IO drivers** (the timer is Concord's; the IO is `reqwest`'s): polled elsewhere, the send panics with Tokio's own message. Building a client and a call needs no runtime. Outside the helpers for tests, it is the one panic that the library documents, under "Panics" on the front page of `concord` and on each send. **Concord starts no task and holds no global state.** (The HTTP library has tasks of its own: `hyper` drives each connection in one. So bytes can still move while nobody polls a call.)

## The values

| Value | Holds | Lives |
|---|---|---|
| `Core<P>` | The transport and the middlewares of the client (each behind an `Arc<dyn …>`), the retry policy, the two clocks, a base that replaces the description's. | In the client. A clone is cheap, and shares the transport and the middlewares. |
| A handle (`riot::Matches`) | A `Core<P>` and the inputs of every level above. | Made by each call of a scope. |
| A call (`riot::matches::Ids`) | The same, its own inputs, and an `Options<P>`. It is `Clone`. | Made by the endpoint's method. Sent once. |
| `Options<P>` | What this call changes: retry, clocks, a URL that replaces the built one (`at_url`), the edits of `edit_request`. | In the call. |
| `Run` | **The clocks of one call:** the instant where `timeout` ends, and the length of `idle_timeout` for this call. Also the name of the endpoint, and a place for an outcome that is already known ([the two clocks](#the-two-clocks)). | Made at step 1 of a send. Every part that waits holds a clone: the tries, the readers, a `ByteStream`. |
| `TryInfo` | The name of the endpoint (`riot.matches.ids`: a promise, the same text as in a printed error), the number of the try, and (hidden) the `Run`. | Made for each try. Given to each middleware, to the transport, and to the generated code of the try. |
| `Sending<T, P>` | One boxed future: the whole exchange. | From `send()` until it gives its result. |

`P` is the problem type of the client (`()` when it has none). `T` is the type after the arrow.

**The default transport** is `concord::Http` ([Concord's own HTTP client](#concords-own-http-client)). A client that was given none makes it at its first send, once, in a cell that all clones share. If it cannot be made, that send is a Build error.

**The transport is erased.** `Core` holds `Arc<dyn Erased>`, where `Erased::send` gives a boxed future.

## The path of one call

`call.await` is `call.send().await`. Nothing happens before the first poll.

| Step | What happens | It can end the call with |
|---|---|---|
| 1 | **`Run::start(core, options, endpoint)`**: the clock of the whole call starts here, if a `timeout` is set (the call's, or else the client's). Then the two expressions of the description that are not part of the request run, in this order: `idle_timeout`, then `idempotent`. (`idle_timeout` runs for each send, also when the call set its own with `with_idle_timeout`: the call's length then wins, and the expression has still run.) | |
| 2 | **The request is built**, by generated code, on a `Draft`, in a fixed order (the base, the paths, the query, the headers, the body; inside each, from the client down, and the lines of a part before the node's own): the nearest `base` (or the client's `with_base`, which replaces the whole `base` line: scheme, host, port and path prefix), each `path` from the root down, the entries of `query` and `headers` in the order of the tree, the body. | Build |
| 3 | **`Draft::finish_for_send`** checks the request (the rules of [C2](../reference/contracts.md)), writes `Content-Type` from the body unless a header of the description gave one (an entry whose value is `None` gave none), puts the URL of `at_url` in place if there is one (a URL that does not parse, `the next URL is not a URL`, and another origin than the one that the call builds are Build errors here), and runs the edits of the call in the order they were added. It gives a `reqwest::Request`. | Build |
| 4 | **`exchange(run, …)`** takes the request and runs the tries ([the tries](#the-tries)). | Transport, Refused, Mismatch |
| 5 | The result of the last try is the result of the call. `send()` keeps the value; `send_full()` keeps the `Answer<T>`. | |

Steps 1 to 3 run once for a call, and once for each page of a paged call. Every try sends a copy of the same request ([C3.1, C3.2](../reference/contracts.md)).

**A send is two halves, and both are hidden.** `prepare` is steps 1 to 3: it gives a `Prepared`, which holds the `Run`, the finished request, what a secret wrote in it (for pages), the mark of `idempotent`, and what reads the answer. `Prepared::send` is steps 4 and 5. `send_full()` is one after the other, and the hidden trait of every call (`__private::Call`) has both (a `GET` or a `HEAD` has a second first half for a conditional send: see [reading an answer that you got yourself](#reading-an-answer-that-you-got-yourself)), so that the engine can look at a request between the two: that is what [pages](#pages) do. [The hidden interface](#the-hidden-interface-method-by-method) is in one table below.

`request()` is steps 2 and 3 with `Draft::finish`: the same request, and the call is still whole. It refuses a body that can be read only once, without touching it. `into_request()` is steps 2 and 3 as for a send.

### How a value is written

Every value that the description places goes to the `Draft` with a name, as `Piece::Val("region", &value)`: the name is what a Build error prints (`input region is not one label of a host`). For a hole and for a `name;` entry the name is the input; for an entry written `"key" = value;` it is the key, also when the value is a bare input. A `None` in a path hole is refused: "a `None` is not sent" is for a query and a header.

A value is written by one writer of the core, a `serde` serializer that takes nothing, one text, or a list of texts. A float is written as `serde_json` writes a number (`1.0`).

**`Secret` is not serializable**, so `json(x)` of a type that holds a secret does not compile. Generated code passes every value through one adapter of the hidden module before it reaches the writer. The adapter knows `Secret` and `Option<Secret>` (and references to them) first, and asks for `Serialize` for everything else. A value that came through the secret side is marked: in a header, the header is sensitive; in a query, the pair follows a next URL; in a path, the piece is known to the `Draft` (it is left out when two requests of a page are compared: see [pages](#pages)); in a host it is known too, but a host that changed is another origin, and that check comes first. A template with a secret in one hole is secret as a whole.

**A body made of entries** is one value, `concord::body::Entries`: a list of keys and values that `serde` writes as a map, in the order written. A value that is a `None` is left out, as in a query: `Entries` asks `serde` whether the value is a `None` (a small serializer that answers only that), so no type is known to the macro. What is inside one `Option` is written as the format writes it. A `Secret` comes through the same adapter and is written as its text; nothing marks it, because a body is never printed and never follows a next URL. `Entries` is a plain map for every `serde` writer: the engine knows that it writes the value of an entry, and the name of that entry, from `Entries` itself, never from a name that `serde` hands on, so the name that a Build error prints is always the entry's ([C6.2](../reference/contracts.md)).

**A body made of bodies** is `Body::concat(parts)`: bytes when every part is in memory, else a stream. Its one-shot streams are taken together, when the whole is taken for a send; one that was already taken makes that a Build error, before a byte goes out. `Body::as_bytes()` and `Body::into_stream()` are public, for a writer of the author's.

**Two orders.** The expressions of a request run by kind first (the base, the paths, the query, the headers, the body), and inside a kind in the order of the tree: the client first, a node's parts before its own lines. The entries go on the wire in the order in which each key was first written. The two differ after an `override` (it runs at the turn of the node that wrote it, and is written at the place of the key), so the values of a kind are kept until all of that kind have run.

A path hole encodes every byte outside `A-Z a-z 0-9 - . _ ~` as `%XX`, with upper-case hex digits, `%` too. A header value is never percent-encoded. A `Base` is taken only when the URL library gives back what was written, piece by piece: the scheme, case aside; the host as the library reads a host on its own (upper case, a longer spelling of an IPv6 address, an international name are the same host); the port, or none when the default one was written; the path once both are decoded (an empty path is the root: a bare host, `https://api.example.com`, is a legal base). Any other difference is refused. So whatever the library would change with no word is never sent. A host label is compared without case; a hole may be a piece of a label (`api-{region}`), and the finished label is checked (63 characters at most; ASCII letters, digits, `-`). Every hole of a template is checked before the entry is left out for a `None`. The URL ends with `/` when the last piece on the way down does: the last `path`, or the `base` when no `path` follows.

In a `{x}` hole, an empty value is refused. A finished segment that is `.` or `..` is refused. In a `{*x}` hole, a piece may be empty, the whole value may not, and a piece that is `.` or `..` is refused.

### The hidden interface, method by method

For every public method of a call, this table says what generated code calls. **The macro writes five things in each endpoint:**

- `__ENDPOINT`, the name of the endpoint (`riot.matches.ids`);
- `__build_draft()`: step 2, the request on a `Draft`;
- `__build_request(take)`: `__build_draft()`, then `Draft::finish` (`take` is false) or `Draft::finish_for_send` (`take` is true);
- `__read_answer(response, info)`: the reader, the arms, the problem;
- `__prepare_inner()`: step 1 (the `Run`, `idle_timeout`, `idempotent`), then `__build_draft()`, then `Draft::prepare`, which finishes the request for a send and gives a `Prepared` that holds `__read_answer`.

**Everything else is the same for every call** and comes from one hidden macro of the core, `call_methods!`, which the macro writes once for each endpoint. It is the one copy of these methods: the macro writes the call to it, and no method of its own among them. It writes the public methods of the table, the hidden trait `__private::Call` (its `prepare` is `__prepare_inner`; `answered_from`, below), `page::Call` (`at_url`), `IntoFuture` for a call with no `paginate`, and for a `GET` or a `HEAD` the second first half `__prepare_if_none_match(etag)` (see [a conditional send](#reading-an-answer-that-you-got-yourself)).

| Public method of a call | What it is built on |
|---|---|
| `send()`, and `.await` (a call to an endpoint with `paginate` has `send()` only) | `prepare()`, then `Prepared::send`, then the value of the `Answer`. |
| `send_full()` | `prepare()`, then `Prepared::send`. |
| `send_raw()` | `prepare()`, then `Prepared::send_raw`: the tries end at the head, and nothing is read. |
| `send_if_none_match(etag)` (a `GET` or a `HEAD`) | `__prepare_if_none_match(etag)`, then `Prepared::send` (see [a conditional send](#reading-an-answer-that-you-got-yourself)). |
| `request()` | `__build_request(false)`: steps 2 and 3 with `Draft::finish`; a one-shot body is refused, not taken. No `Run`, no `idle_timeout`, no `idempotent`. |
| `into_request()` | `__build_request(true)`: steps 2 and 3 with `Draft::finish_for_send`, and a one-shot body is taken. No `Run`. |
| `read_response(response)` | `__read_answer(response, info)` alone (see [reading an answer that you got yourself](#reading-an-answer-that-you-got-yourself)). |
| `with_retry`, `with_timeout`, `with_idle_timeout`, `edit_request(f)`, `at_url(url)` | A field of `Options`, read by `prepare` and by `finish_for_send`. |
| `pages()` (written by the macro, inside the line of `call_methods!`) | `__private::pages(call, pager)`: the machine of [pages](#pages). For each page: `prepare()`, the two checks on the `Prepared`, `Prepared::send`. On the call that the pager gives: `answered_from(url)`, below. |
| `pages().items()` | `IntoItems` of each page. |

**How generated code gives values to the `Draft`.**

- A template is always given with a literal piece first, an empty one when the template is exactly one hole (`"{ids}"`): so a list in it is a Build error, as in every template. One `Val` alone is a whole value.
- A segment of a `path` whose text ends with `/` ends with the literal piece `"/"`: a last literal piece that ends with `/` tells the `Draft` that the segment ends with `/`, and so the URL when it is the last one.
- `Draft::segment` given only an empty literal does nothing.
- `Draft::header` is given the name as written; it compares and stores it without case.

**The other pieces of `__private`.** `Core` and `Options` (what a client and a call hold: [the values](#the-values)); `Draft`, a request being built, which takes each value with its name ([how a value is written](#how-a-value-is-written)); one adapter that every placed value goes through, which knows `Secret` and `Option<Secret>` before it asks for `Serialize`; `InheritAcrossBase` and `InheritPathAcrossBase`, two traits that the secret side of the adapter does not have, so that a secret, in an entry or in a piece of a `path`, stays with the `base` where it was written ([C8.9](../reference/contracts.md#c8-secrets-and-pages)); `Run`, `exchange`, `exchange_raw` and `again` (the clocks and the tries of one send, in one loop, and the decision to try again: [the tries](#the-tries)); `no_arm` (the Mismatch for a 2xx that a `HEAD` takes in no arm, with its evidence); and functions such as `reader`, whose bound checks an expression of the author, and which carries the type that is read. The names of the helpers carry no promise. How the macro uses them: [the macro](macro.md).

**A relative next URL needs the URL of the answer that gave it**, and a pager gives only text to `at_url`. So the machine of pages tells the next call where its answer came from: the hidden trait of every call has `answered_from(url)`, which keeps that URL in `Options`. `finish_for_send` resolves a relative `at_url` against it; a call that no machine touched (a caller's own `call.at_url("../next")`) has none, and resolves against the URL that it builds. The resolution comes before the check of the origin and before the edits.

## The tries

One loop runs the tries, for every kind of send. `__private::exchange` (a send) and `__private::exchange_raw` (`send_raw()`) both call it; they differ only in how a try reads its answer.

```text
exchange_with(run, core, options, mark, request, read):
    policy = options.retry, else core.retry, else "one try"      // a call's policy replaces the client's, whole
    copy   = request.try_clone()                     // none for a body that is a stream: one try only
    n = 0
    loop:
        this = the request the first time, else a clone of `copy`
        n = n + 1                                    // a try is counted when it is handed to the chain
        info = TryInfo for (run, n)
        outcome = one try, under the two clocks
        if outcome is a value: return it
        (wait, asked) = again(...)                   // the decision to try again
        until = now + wait                           // one deadline, taken once, after the rules ran
        if no wait, or `until` is not before the end of `timeout`: return outcome, with tries = n
        log the event `retry`
        sleep until `until`                          // only `timeout` bounds it
        if `timeout` cut the wait: return outcome, unchanged     // the last outcome, not a timeout
        drop everything of this try                  // C4.5: the answer is gone before the next send
```

A try that a middleware or the transport blocked (`TransportError::blocked()`) ends the tries: the blocked error is the result. A slow log or any other delay near the end of `timeout` cannot lose the last outcome: the deadline is taken once, and the outcome is kept until the wait is over.

### One try

A try is the whole exchange for one copy of the request ([C3.5](../reference/contracts.md)):

1. The request goes out through the chain ([the way out](#the-way-out-the-middlewares-then-the-transport)): the middlewares, then the transport. A `TransportError` from any of them is the outcome: a Transport error with its class.
2. With the `reqwest::Response`, the engine calls `one_try(response, info)`. An answer with no URL of its own (one that a transport of the caller, a `fake` or a `tower` service made from an `http::Response`: `reqwest` gives it a placeholder) is first given the URL of its request, so a relative next URL is resolved against the request that was sent. The docs of `Transport` and `fake` say that an answer should carry its URL. This is the generated part. It looks at the status, picks the arm (an exact status first, then a class, then the simple rule), reads the body with the reader if the arm binds it or the simple rule needs it, and runs the arm.
3. The outcome is one of: a value; a Refused error (a status that no arm took, an arm that ended with the API's problem, or an arm on a status that is not a 2xx whose `body` did not read); a Mismatch (a 2xx whose body did not read, or an arm said so with `mismatch(…)` or `require(…)`); a Transport error (the body broke while it was read, or a clock ended).

With `read Stream;`, a success ends the try at the head: nothing is read. A refusal is read like any refusal (64 KiB, the problem type). For `send_raw()` the try always ends at the head; a head below `400` is final, like a value, and a head of `400` or above is looked at by the policy, as a refusal is.

The simple rule for an arrow that is an `Option<T>` or a `Result<T, E>` reads a `T` and wraps it (`Some`, `Ok`): the generated code does that, the engine sees one value.

The helpers that generated code calls inside `one_try` are `read_body` (a reader, or a Mismatch; on a status that is not a 2xx, a Refused error with the status, the headers and the bytes kept, and no problem), `refused` and `refused_plain` (the error for a status that no arm takes, with at most 64 KiB of the body and the problem type tried on it), `no_arm` (a 2xx that no arm takes on a `HEAD` with an arrow), and `arm` (what an arm gave, as the result of the try). Each gets the `TryInfo`, and so the clocks.

### The decision to try again

One function with no I/O, so that it can be tested alone. The time left, the wall clock (for a `Retry-After` that is a date) and the random number are given to it. It gives the wait before the next try, or nothing; and with either, what was asked (the `Retry-After` of the answer and the wait of the rule of `when` that gave one), so that `e.retry_after()` can give the longer of the two and no rule is asked twice.

```text
again(policy, outcome, n, method, mark, can_copy, time_left, now, random) -> (Option<Duration>, asked):
    1. outcome is a Mismatch, a Build error, or blocked  → None      (C3.6)
    2. n >= policy.tries                                 → None      (C4.2)
    3. not can_copy                                      → None      (C3.2)
    4. may_repeat =  mark is Yes
                  or (mark is ByMethod and method is GET, HEAD, OPTIONS, PUT or DELETE)
                  or outcome is a refusal whose status the policy lists with not_applied_on
       not may_repeat                                    → None      (C4.1)
    5. any rule of `unless` says true                    → None
    6. asked = the first rule of `when`, in the order added, that gives a wait
       if none did: the standard table (C4.2) must name the outcome, else → None
    7. after = Retry-After of the outcome, if it has one and it can be read   // seconds, or a date in one of the three forms of RFC 9110, 5.6.7,
                                                                                // read by Concord against the wall clock that was given; a date in the past is zero
       asked or after is over 60 seconds                 → None      (C4.3)
       wait = max(asked, after, random × min(60 s, 1 s × 2^(n-1)))
       wait >= time_left                                 → None      (C4.3)
    8. Some(wait)
```

The numbers on the right are those of [the contracts](../reference/contracts.md).

**`random` is drawn again for each wait.** Two waits of one call are not tied to each other: the loop gives a new draw each time it asks.

Step 4 opens the gate for a refusal only: a body that broke, or a clock that ended, after a head with a listed status does not open it. `time_left` may be given to the function; the loop of [the tries](#the-tries) gives none, and compares its one deadline with the end of `timeout` itself, after the rules ran (a rule may take time).

`method` is the method of the request as it was built, after the edits of `edit_request` and before any middleware. `mark` is what the description says: nothing (ByMethod), or `idempotent <bool>;`, which is Yes or No for this call (`idempotent;` alone is Yes).

A policy holds every rule of `when` and every rule of `unless` that was added to it. A rule is asked only at its step, so only when another try is still possible.

The function is in [`concord-core/src/retry.rs`](../../concord-core/src/retry.rs).

### The way out: the middlewares, then the transport

A client has one **transport**, which really sends (`concord::Http` unless `with_transport` gave another), and a list of **middlewares**, which every request goes through on the way: first the ones that a caller added with `with_middleware`, then the ones of the description (`middleware m;`), in the order written. **The author's are the last before the wire**, and among them the author writes the one that signs last.

- A middleware of the description is made once, in `new`, from the required inputs of the client. Every handle and every call shares it, so it may hold what must be shared: a quota, a token.
- For each try the engine makes a `Next` over the whole list and calls `next.send(request, info)`. A middleware gets the request, the `TryInfo` and the `Next` for the rest of the way. It may change the request, wait, send on once, send on again (a token that was renewed), or not send (`TransportError::blocked()`).
- **The last `Next` is the door to the transport.** There the engine checks `timeout` (no request is handed to the transport after it has ended), starts the clock for silence, and calls the `send` of the transport.
- `with_transport` replaces the transport and leaves the list as it is. So a fake in a test gets the request after every middleware: a signed API is signed in its tests too.
- What Concord counts is one try. What a middleware sends on its own inside that try is its own affair. So is what it changes: the checks of retry (the method) and of pages (the origin, "the same request") look at the request as it was built, before any middleware.
- A middleware gets the request as it was built, with the edits of `edit_request` already made (step 3 of [the path of one call](#the-path-of-one-call)).

## The two clocks

What each clock bounds, its default, and the order in which its length is chosen are [rule 19](../reference/rules.md#behaviour) and [C7.6](../reference/contracts.md#c7-concords-own-client-and-the-clocks). This part says how the engine runs them. `timeout` starts at step 1 of [the path of one call](#the-path-of-one-call), before the request is built (for a page: when that page is asked for). `idle_timeout` starts each time the transport gets a request, and each time a wait for a piece of a body begins. Its length is read once for a send, from the `Options` of the call, the statement of the description (evaluated with the inputs), and the `Core`.

When one ends, the call ends with a Transport error of class Timeout, whose note names the clock, what it waited for (an answer, or a body after its status), and where its length is set (the lines of a request that is built or answered, in [the messages](../reference/messages.md)).

**A clock ends a wait, and nothing else.** Both are Tokio timers beside a wait. Work that can finish without waiting finishes, also after the instant has passed: the work is polled first, and the timer second. Code that does not yield is not interrupted. One check is added, because a send is an effect: **no request is handed to the transport after `timeout` has ended** ([the way out](#the-way-out-the-middlewares-then-the-transport)).

**Two clocks that end at the same instant: the one that covers more is reported.** The call's `timeout` wins over the connect timeout of the transport and over the clock for silence. So when `timeout` and the silence of a body end together, the error is the Timeout of the call. A wait between two tries is another case: it is not started when it would end after `timeout`, and when `timeout` still cuts one that was started, the caller gets the last outcome, unchanged, not a timeout ([C4.3](../reference/contracts.md)).

**The clock for silence watches two things, and nothing else.**

| What it watches | From | Until | What starts it again |
|---|---|---|---|
| The transport, while it has the request | The hand-over (the last `Next`: [the way out](#the-way-out-the-middlewares-then-the-transport)) | The head of the answer (the return of the transport's `send`) | Each piece of the request body that is taken, when the body is a stream: the engine wraps the stream. |
| A body, while it is read (`Incoming::bytes`, `Incoming::stream`, a `ByteStream` in the caller's hands) | Each wait for a piece | That piece | — |

So the engine needs no shared instant for a whole try: one timer around the `send` of the transport, and one around each wait for a piece. A middleware's time, before or after the transport, is never silence. Nor is a reader's own wait for something that is not the body. Only `timeout` bounds those. The clock of a `ByteStream` runs while a `try_next()` waits, not between two of them: dropping that future resets it. Through `impl Stream for ByteStream` it runs from the first poll that found nothing until the next piece, also between two polls, because a dropped `next()` cannot be seen. A `ByteStream` may read ahead of what the caller asked for. A failure while it is read is a `concord::read::StreamError` (its kind is Transport), which `?` turns into any `concord::Error<P>` or a `ReadError`. After `send_raw()` neither clock follows the answer that the caller holds.

**An outcome that is already known is never lost to a clock** ([C5.7](../reference/contracts.md)). There are two: a refusal, and the Mismatch of `no_arm`. Silence while its body is read: `refused` (or `no_arm`) gets the timeout from the read, and gives the outcome that it knows, with what it has of the body and `kept()` saying `Interrupted`. The end of `timeout`: the engine, which waits beside the whole try, drops it; so `refused` and `no_arm` write into the `Run` what they know as it comes (the kind, the status, the headers, then each piece up to 64 KiB), and the engine looks there before it gives the Timeout error.

An `Incoming` that the engine makes for a try has the `Run` (the length of the silence, and the end of `timeout`). One that a test makes with `Incoming::new(response)` has no clock.

Two limits, said plainly. A piece of a stream body that the library pulled is not a piece that the server received: the library pulls while its buffers have room, so a line that has stalled is seen one buffer later. And a body that is held in memory is given to the transport as bytes, in one piece, so that a transport or a middleware can read it (a signer hashes it): the clock then sees nothing until the head, and a large upload from memory on a slow line needs a longer `idle_timeout` (on the call, or on the endpoint in the description).

## A drop

Everything of a call lives in its `Sending`, and Concord moves it only when it is polled.

| What is dropped | What Concord promises |
|---|---|
| A call that was not sent | Nothing happened. No request was built. |
| `Sending`, before the first poll | Nothing happened. |
| `Sending`, at any later moment | Concord stops, and starts no other try. **The request may have reached the server.** What the HTTP library does with the connection (close it, drain it, use it again) is its own affair. |
| A `ByteStream` | Concord reads no more of it. |

`Sending` is not cancel safe, like any request: a `select!` that drops it has lost the call, and cannot know whether the server saw it. `Pages` is different (see [pages](#pages)).

**What Concord logs:** one event, `retry`, through `tracing` with the target `concord`, at the level debug, each time it decides to try again. Its fields: `endpoint` (the name of the endpoint), `try` (the number of the try that failed), `status` (when the failed try had an answer) or `class` (when it had none), and `wait_ms` (the wait before the next try, in whole milliseconds) ([C10.6](../reference/contracts.md#c10-the-generated-code-and-the-log)). Nothing else: a middleware sees every try and every answer.

## Concord's own HTTP client

`concord::Http` is a `reqwest::Client` that Concord made, in a type of its own. Its settings, and what `Http::with(f)` lets `f` change, are in [the crates](../reference/crates.md#concords-own-http-client). `Http::with` sets the defaults on `reqwest`'s builder, runs `f`, and then sets `redirect::Policy::none()`, so that no `f` can turn redirects on. `Http::new()` is `Http::with(|b| b)`. The two functions are in [`transport.rs`](../../concord-core/src/transport.rs).

**The class of a failure.** `Connect` means that a connection failed before a head was received, also after it was made, with any client: an `io::Error` of kind `ConnectionRefused`, `ConnectionReset`, `ConnectionAborted`, `BrokenPipe`, `NotConnected` or `UnexpectedEof` in the chain, or any `reqwest` error of the kind "request" with no head that is not a timeout. A failure while the body of an answer is read is class `Body`, whatever its chain. `Other` is what is none of these, of a timeout and of blocked. An error of `reqwest` that is about a status or about decoding keeps no source. Connect says nothing about what was sent, and Concord draws nothing from it: a request that may not be repeated is not sent again after it, because a failed connection does not prove that nothing was sent. What `reqwest` itself sends again below this level (a stream that the server refused before processing it) is left as `reqwest` has it, and is not counted as a try ([C4.4](../reference/contracts.md)).

## Pages

`Pages<T, E>` is one boxed state machine. The call type and the pager are erased inside it. `try_next()` and the `Stream` implementation poll the same function on the same state.

```text
state:
    Ready   { call }                      // nothing is in flight; `call` is the next one to send
    Flying  { call, sent, exchange }      // `exchange` is the Sending of `call`; `sent` is what its request was (see "the same request")
    Failed  ( error )                     // the pager refused the page that came
    Over
kept beside the state:  origin (of the first request, as it was finished: after the edits of `edit_request`), last (what the request before was)

poll_page:
    Ready { call }:
        prepared = call.clone().prepare()                        → on an error: state = Over; give Err
                   // the first half of a send (the path of one call): the clocks of this page start, `idle_timeout`, `idempotent`, the request built and finished
        sent     = (method, URL, headers, bytes of the body), without the fragment and without what a secret wrote in the URL or in a header
        first page: origin = the origin of the request
        later page: the origin of the request is not `origin`    → state = Over; give a Build error
        later page: `sent` is equal to `last`                    → state = Over; give a Mismatch      // the same request again
        exchange = prepared.send()                               // the second half: the tries
        state    = Flying { call, sent, exchange }; go on
    Flying:
        poll `exchange`
            Pending          → Pending                       // the state stays where it is
            Ready(Err(e))    → state = Over; give Err(e)
            Ready(Ok(answer)):
                last = sent
                next = pager(call.clone(), &answer)          // it cannot be interrupted
                    None        → state = Over
                    Err(fail)   → state = Failed(the error of `fail`: Refused for the API's problem, Mismatch for `mismatch(…)`)
                    Some(next)  → state = Ready { call: next }
                give Ok(Some(answer.into_value()))
    Failed(error) → state = Over; give Err(error)            // given once, with the `try_next()` after the page
    Over          → give Ok(None)
```

- **The request of a page is built when that page is asked for**, once, by the first half of a send, and that same request is then sent by the second half: nothing is built twice. It is built from a clone of the call, so that the call stays whole for the pager. One `Body` that is a one-shot stream (an input) is shared by the clones and taken by the first page; the building of a second page is then a Build error (`the body is a stream that was already sent`), and nothing is sent. An expression that makes a new stream each time it runs gives each page its own. The same holds for a body made of bodies (`Body::concat`, which `multipart` is built on): its one-shot streams are taken with the whole, when the request is built for a send (before the middlewares), so a stream that was already sent is found before anything goes out. So a `Secret` that was replaced between two pages is used, the edits of `edit_request` run once for each page, and `timeout` covers the building of every page.
- **Every page of one `.pages()` goes to the origin of its first request,** whatever the pager did: a pager that writes an input which stands in the host cannot carry a key to another place.
- **"The same request"** is: the same method, the same URL, the same bytes of the body and the same headers, **where what a secret wrote in the URL or in a header is left out**: a piece of a path, a pair of the query, a value of a header (the builder knows which pieces these are; for a header it is the flag `sensitive`, so a value that `edit_request` marks is left out too). The fragment of a URL is left out as well. A body is compared as its bytes: a secret that is an entry of a body, replaced between two pages, looks like a move, once. A body that is a stream has no bytes to compare: a request with one is never "the same request". Two limits of the marks: a secret in a host that was replaced is another origin, and that check comes first, so the pages end with its Build error; and after an `edit_request` that changed the URL, the pieces that a secret wrote are no longer known, so the URL is compared as it is. So an API that pages by a header moves, and a key that was replaced between two pages does not look like a move, in a path, a query or a header.
- **`at_url` gives a path and a query, never an origin.** A URL at another origin than the one that the call builds (its `base`, or `with_base`) is a Build error, in pages and outside them. It is checked when the request is built, in step 3, before the edits of `edit_request`. So the two checks are apart: `at_url` cannot leave the origin that the description gives, and the pages cannot leave the origin of their first request. An `edit_request` that changes the host cannot be combined with a pager that uses `at_url`: its origin check uses the call before the edits. Use `with_base` to move the client. A relative URL is resolved against the URL of the answer that gave it; with no answer (a caller's own `call.at_url("../next")`), against the URL that the call builds.
- A next URL that already has a pair with the key of a secret pair: every pair of that key is taken out, and the secret is written once.
- A relative next URL is resolved against the URL of the answer that gave it; an `Answer` keeps that URL, hidden.
- **A `try_next()` that is dropped loses nothing** ([C8.5](../reference/contracts.md)). The exchange in flight is in the state, not in the future that was dropped.
- An exchange that nobody polls is not moved by Concord. Its clocks are instants, so they still run. A wait that is still open at the next poll, after its clock has ended, ends with a timeout. An answer that is already whole is given.
- Each page is one call, with the options of the call. The time that the caller spends between two pages is not counted. Nothing bounds the whole loop; that is a timer of the caller around it.
- `pages.items()` gives each item of each page. It asks one thing of the page type, `concord::page::IntoItems`, which a `Vec<T>` has and an author writes for a list type of their own. An `Option` and a `Result` do not have it.
- A paged call has no `.await` of its own: `.send()` gives one page, `.pages()` gives this machine.

## Reading an answer that you got yourself

`call.read_response(response)` is the reading step of [a try](#one-try), alone: the arms, the reader and the problem type of the endpoint, on a `reqwest::Response` that came from anywhere.

```rust
let call = riot.matches(Europe).get(&id);
let response = my_client.execute(call.request()?).await?;
let game = call.read_response(response).await?;      // riot::Result<Match>
```

No retry, no clock and no page: those belong to a send, and Concord sent nothing. On an error from `read_response`, `tries()` is 0.

**A conditional send.** `call.send_if_none_match(etag)`, on a call whose method is `GET` or `HEAD`, writes `If-None-Match` when the request is built (in place of one that the description wrote, and before the edits of `edit_request`), and sends. A `304` that no arm takes gives an `Answer` whose value is `None`; anything else gives `Some(value)`. The headers are there in both cases. **It has its own first half** (see [the path of one call](#the-path-of-one-call)): for a `GET` or a `HEAD`, `call_methods!` writes `__prepare_if_none_match(etag)` beside `prepare`. It is `prepare` with two differences: the header is written after the headers of the description and before `finish_for_send` runs the edits (so an edit of the caller that removes `if-none-match` removes this one too), and what reads the answer gives `None` for a `304` that no arm takes. It gives a `Prepared` whose value is `Option<T>`; `send_if_none_match` is that half, then `Prepared::send`.

## What a call costs

Measured with `just bench` (`criterion`, outside the gate), on the `Script` transport of the test kit (no socket), with the clients of [`tests/tour`](../../tests/tour/src), against the same request written by hand on `reqwest` alone with the same stand-in. A counting allocator counts each call to `alloc` and `realloc`; making the client is not counted. Time in microseconds, then allocations.

| Operation | Concord | `reqwest` by hand |
|---|---:|---:|
| Build a `GET` (two path inputs, three query pairs added by `edit_request`) | 1.656 µs · 33 | 0.626 µs · 6 |
| Send that `GET` | 3.763 µs · 86 | 1.312 µs · 25 |
| A `POST` with a JSON body of entries | 3.613 µs · 93 | 1.248 µs · 35 |
| A call tried three times | 6.082 µs · 109 | 1.642 µs · 46 |
| A loop of ten pages | 34.892 µs · 807 | 7.104 µs · 141 |
| 1 MiB read as `Json` | 376.096 µs · 18 | 247.581 µs · 13 |
| 1 MiB read as `Text` | 296.223 µs · 17 | 192.269 µs · 13 |
| 1 MiB read as `Binary` | 250.229 µs · 16 | 0.584 µs · 12 |
| 1 MiB read as `Stream` | 0.662 µs · 15 | 0.545 µs · 11 |

**The budget:** Concord adds less than 2 µs and less than 25 allocations to building a plain `GET`, over building the same `reqwest::Request` by hand.

**The measure against the budget** goes through a client that `concord::api!` writes in the bench itself ([`concord-api/benches/macro_get.rs`](../../concord-api/benches/macro_get.rs)), on the endpoint that the budget names: **two path inputs and three `query` inputs**, with a secret header from the client. It measures the time and the allocations that Concord adds to building the request, over the same `reqwest::Request` built by hand. Concord adds **1.07 µs and 23 allocations** (1.695 µs and 29 allocations, against 0.621 µs and 6). The same measure on `Shapes::guarded` of `tests/tour-gen` (one path input and two `query` inputs, with secrets among the values) adds **0.78 µs and 23 allocations**. Both are inside the budget of 2 µs and 25 allocations. The table above is measured on another path: the hand-written client of `tests/tour`, with the three pairs added by `edit_request`, so the `query` of a description does not run there.

What Concord adds pays for owning the inputs, the checks of every value, the marks of secrets, the clocks, retry, pages, the limits and the evidence of an error.

Not measured: how late the wrapped stream of [the two clocks](#the-two-clocks) sees a line that stalled.
