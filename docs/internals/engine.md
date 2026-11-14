# The engine: how one call runs

*[Concord](../../README.md) · [docs](../README.md) · internals · next: [the macro](macro.md)*

This page is for a developer who wants to change `concord-core`, or to know what a call really does. It covers the values that the engine holds, the path of one call, the two timeouts, what a drop does, and how pages keep a request in flight, with the names used in the code.

[The contracts](../reference/contracts.md) say **what** the engine must do. This page says **how** the engine in [`concord-core`](../../concord-core/src) does it. A name that is not public belongs to the hidden module [`__private`](../../concord-core/src/__private/mod.rs).

The engine needs two things from outside: `reqwest`, and Tokio's timer (`tokio::time`). **Sending a call needs a Tokio 1.x runtime with its time and IO drivers** (the timer is Concord's; the IO is `reqwest`'s): polled elsewhere, the send panics with Tokio's own message. Building a client and a call needs no runtime. Outside the test helpers, this is the one panic that the library documents, under "Panics" on the front page of `concord` and on each send method. **Concord starts no task and holds no global state.** (The HTTP library has tasks of its own: `hyper` drives each connection in one, so bytes can still move while nobody polls a call.)

## The values

| Value | Holds | Lives |
|---|---|---|
| `Core<P>` | The transport and the middleware of the client (each behind an `Arc<dyn …>`), the retry policy, the two timeouts, and a base that replaces the description's. | In the client. A clone is cheap, and shares the transport and the middleware. |
| A handle (`riot::Matches`) | A `Core<P>` and the inputs of every level above. | Made by each call to a scope's method. |
| A call (`riot::matches::Ids`) | The same, plus its own inputs and an `Options<P>`. It is `Clone`. | Made by the endpoint's method. Sent once. |
| `Options<P>` | What this call changes: retry, timeouts, a URL that replaces the built one (`at_url`), the edits of `edit_request`. | In the call. |
| `Run` | **The timeouts of one call:** the instant when `timeout` expires, and the length of `idle_timeout` for this call. Also the name of the endpoint, and a slot for an outcome that is already known ([the two timeouts](#the-two-timeouts)). | Made at step 1 of a send. Every part that waits holds a clone: the attempts, the readers, a `ByteStream`. |
| `TryInfo` | The name of the endpoint (`riot.matches.ids`: a promise, the same text as in a printed error), the number of the attempt, and (hidden) the `Run`. | Made for each attempt. Given to each middleware, to the transport, and to the generated code of the attempt. |
| `Sending<T, P>` | One boxed future: the whole exchange. | From `send()` until it yields its result. |

`P` is the client's problem type (`()` when it has none). `T` is the type after the arrow.

**The default transport** is `concord::Http` ([Concord's own HTTP client](#concords-own-http-client)). A client that was given none creates it at its first send, once, in a cell that all clones share. If it cannot be created, that send fails with a Build error.

**The transport is erased.** `Core` holds `Arc<dyn Erased>`, where `Erased::send` returns a boxed future.

## The path of one call

`call.await` is `call.send().await`. Nothing happens before the first poll.

| Step | What happens | It can end the call with |
|---|---|---|
| 1 | **`Run::start(core, options, endpoint)`**: the call timeout starts here, if a `timeout` is set (the call's, or else the client's). Then the two expressions of the description that are not part of the request run, in this order: `idle_timeout`, then `idempotent`. (`idle_timeout` runs for each send, even when the call set its own with `with_idle_timeout`: the call's length then wins, but the expression has still run.) | |
| 2 | **The request is built** by generated code on a `Draft`, in a fixed order (the base, the paths, the query, the headers, the body; inside each, from the client down, with the lines of a part before the node's own): the nearest `base` (or the client's `with_base`, which replaces the whole `base` line: scheme, host, port and path prefix), each `path` from the root down, the entries of `query` and `headers` in tree order, then the body. | Build |
| 3 | **`Draft::finish_for_send`** checks the request (the rules of [C2](../reference/contracts.md)), writes `Content-Type` from the body unless a header of the description set one (an entry whose value is `None` sets none), puts the URL of `at_url` in place if there is one (a URL that does not parse, `the next URL is not a URL`, and an origin other than the one that the call builds are Build errors here), and runs the call's edits in the order they were added. It returns a `reqwest::Request`. | Build |
| 4 | **`exchange(run, …)`** takes the request and runs the attempts ([the attempts](#the-attempts)). | Transport, Refused, Mismatch |
| 5 | The result of the last attempt is the result of the call. `send()` keeps the value; `send_full()` keeps the `Answer<T>`. | |

Steps 1 to 3 run once for a call, and once for each page of a paginated call. Every attempt sends a copy of the same request ([C3.1, C3.2](../reference/contracts.md)).

**A send has two halves, and both are hidden.** `prepare` is steps 1 to 3: it returns a `Prepared`, which holds the `Run`, the finished request, what a secret wrote in it (for pages), the `idempotent` mark, and what reads the response. `Prepared::send` is steps 4 and 5. `send_full()` runs one after the other. The hidden trait of every call (`__private::Call`) has both halves (a `GET` or a `HEAD` has a second first half for a conditional send: see [reading a response that you obtained yourself](#reading-a-response-that-you-obtained-yourself)), so that the engine can inspect a request between the two: that is what [pages](#pages) do. [The hidden interface](#the-hidden-interface-method-by-method) is summarized in one table below.

`request()` is steps 2 and 3 with `Draft::finish`: the same request, and the call stays whole. It rejects a body that can be read only once, without consuming it. `into_request()` is steps 2 and 3 as for a send.

### How a value is written

Every value that the description places goes to the `Draft` with a name, as `Piece::Val("region", &value)`: the name is what a Build error prints (`input region is not one label of a host`). For a placeholder and for a `name;` entry, the name is the input; for an entry written `"key" = value;`, it is the key, even when the value is a bare input. A `None` in a path placeholder is rejected: "a `None` is not sent" applies to a query and a header.

A value is written by one writer in the core, a `serde` serializer that produces nothing, one string, or a list of strings. A float is written as `serde_json` writes a number (`1.0`).

**`Secret` is not serializable**, so `json(x)` of a type that holds a secret does not compile. Generated code passes every value through one adapter in the hidden module before it reaches the writer. The adapter recognizes `Secret` and `Option<Secret>` (and references to them) first, and requires `Serialize` for everything else. A value that came through the secret side is marked: in a header, the header is sensitive; in a query, the pair follows a next URL; in a path, the `Draft` knows the segment (it is left out when the requests of two pages are compared: see [pages](#pages)); in a host, the `Draft` knows it too, but a host that changed is another origin, and that check comes first. A template with a secret in one placeholder is secret as a whole.

**A body made of entries** is one value, `concord::body::Entries`: a list of keys and values that `serde` serializes as a map, in the order written. A value that is a `None` is left out, as in a query: `Entries` asks `serde` whether the value is a `None` (with a small serializer that answers only that question), so the macro needs to know no type. What is inside one `Option` is written as the format writes it. A `Secret` comes through the same adapter and is written as its text; nothing marks it, because a body is never printed and never follows a next URL. `Entries` is a plain map for every `serde` serializer: the engine learns that it is writing the value of an entry, and the name of that entry, from `Entries` itself, never from a name that `serde` passes on, so the name that a Build error prints is always the entry's ([C6.2](../reference/contracts.md)).

**A body made of bodies** is `Body::concat(parts)`: bytes when every part is in memory, otherwise a stream. Its one-shot streams are taken together, when the whole is taken for a send; if one of them was already taken, that is a Build error, before a byte goes out. `Body::as_bytes()` and `Body::into_stream()` are public, for a custom writer.

**Two orders.** The expressions of a request run by kind first (the base, the paths, the query, the headers, the body), and within a kind in tree order: the client first, and a node's parts before its own lines. The entries go on the wire in the order in which each key was first written. The two orders differ after an `override` (it runs at the turn of the node that wrote it, and is written at the position of the key), so the values of a kind are kept until all the expressions of that kind have run.

A path placeholder encodes every byte outside `A-Z a-z 0-9 - . _ ~` as `%XX`, with upper-case hex digits, `%` included. A header value is never percent-encoded. A `Base` is accepted only when the URL library returns what was written, part by part: the scheme, ignoring case; the host as the library parses a host on its own (upper case, a longer spelling of an IPv6 address, or an international name is the same host); the port, or none when the default one was written; the path once both are decoded (an empty path is the root: a bare host, `https://api.example.com`, is a legal base). Any other difference is rejected, so nothing that the library would silently change is ever sent. A host label is compared without case; a placeholder may be part of a label (`api-{region}`), and the finished label is checked (63 characters at most; ASCII letters, digits, `-`). Every placeholder of a template is checked before the entry is left out for a `None`. The URL ends with `/` when the last part on the way down does: the last `path`, or the `base` when no `path` follows.

In a `{x}` placeholder, an empty value is rejected, and so is a finished segment that is `.` or `..`. In a `{*x}` placeholder, a segment may be empty but the whole value may not, and a segment that is `.` or `..` is rejected.

### The hidden interface, method by method

For every public method of a call, this table says what generated code calls. **The macro generates five things in each endpoint:**

- `__ENDPOINT`, the name of the endpoint (`riot.matches.ids`);
- `__build_draft()`: step 2, the request on a `Draft`;
- `__build_request(take)`: `__build_draft()`, then `Draft::finish` (`take` is false) or `Draft::finish_for_send` (`take` is true);
- `__read_answer(response, info)`: the reader, the arms, the problem;
- `__prepare_inner()`: step 1 (the `Run`, `idle_timeout`, `idempotent`), then `__build_draft()`, then `Draft::prepare`, which finishes the request for a send and returns a `Prepared` that holds `__read_answer`.

**Everything else is the same for every call** and comes from one hidden macro of the core, `call_methods!`, which the macro invokes once for each endpoint. It is the only copy of these methods: the macro generates the invocation, and none of these methods itself. `call_methods!` generates the public methods of the table, the hidden trait `__private::Call` (its `prepare` is `__prepare_inner`; `answered_from`, below), `page::Call` (`at_url`), `IntoFuture` for a call with no `paginate`, and, for a `GET` or a `HEAD`, the second first half `__prepare_if_none_match(etag)` (see [a conditional send](#reading-a-response-that-you-obtained-yourself)).

| Public method of a call | What it is built on |
|---|---|
| `send()`, and `.await` (a call to an endpoint with `paginate` has only `send()`) | `prepare()`, then `Prepared::send`, then the value of the `Answer`. |
| `send_full()` | `prepare()`, then `Prepared::send`. |
| `send_raw()` | `prepare()`, then `Prepared::send_raw`: the attempts end at the head, and nothing is read. |
| `send_if_none_match(etag)` (a `GET` or a `HEAD`) | `__prepare_if_none_match(etag)`, then `Prepared::send` (see [a conditional send](#reading-a-response-that-you-obtained-yourself)). |
| `request()` | `__build_request(false)`: steps 2 and 3 with `Draft::finish`; a one-shot body is rejected, not taken. No `Run`, no `idle_timeout`, no `idempotent`. |
| `into_request()` | `__build_request(true)`: steps 2 and 3 with `Draft::finish_for_send`, and a one-shot body is taken. No `Run`. |
| `read_response(response)` | `__read_answer(response, info)` alone (see [reading a response that you obtained yourself](#reading-a-response-that-you-obtained-yourself)). |
| `with_retry`, `with_timeout`, `with_idle_timeout`, `edit_request(f)`, `at_url(url)` | A field of `Options`, read by `prepare` and by `finish_for_send`. |
| `pages()` (generated by the macro, inside the `call_methods!` invocation) | `__private::pages(call, pager)`: the state machine of [pages](#pages). For each page: `prepare()`, the two checks on the `Prepared`, then `Prepared::send`. On the call that the pager returns: `answered_from(url)`, below. |
| `pages().items()` | `IntoItems` of each page. |

**How generated code passes values to the `Draft`.**

- A template is always passed with a literal piece first, an empty one when the template is exactly one placeholder (`"{ids}"`): so a list in it is a Build error, as in every template. A single `Val` on its own is a whole value.
- A segment of a `path` whose text ends with `/` ends with the literal piece `"/"`: a last literal piece that ends with `/` tells the `Draft` that the segment ends with `/`, and so does the URL when this segment is the last one.
- `Draft::segment` given only an empty literal does nothing.
- `Draft::header` receives the name as written; it compares and stores it without case.

**The other pieces of `__private`.** `Core` and `Options` (what a client and a call hold: [the values](#the-values)); `Draft`, a request being built, which takes each value with its name ([how a value is written](#how-a-value-is-written)); one adapter that every placed value goes through, which recognizes `Secret` and `Option<Secret>` before it requires `Serialize`; `InheritAcrossBase` and `InheritPathAcrossBase`, two traits that the secret side of the adapter does not implement, so that a secret, in an entry or in a segment of a `path`, stays with the `base` where it was written ([C8.9](../reference/contracts.md#c8-secrets-and-pages)); `Run`, `exchange`, `exchange_raw` and `again` (the timeouts and the attempts of one send, in one loop, and the retry decision: [the attempts](#the-attempts)); `no_arm` (the Mismatch for a 2xx that no arm of a `HEAD` matches, with its evidence); and functions such as `reader`, whose bound checks an expression of the author's and which carries the type that is read. The names of the helpers carry no promise. How the macro uses them: [the macro](macro.md).

**A relative next URL needs the URL of the response that contained it**, and a pager passes only text to `at_url`. So the pages state machine tells the next call where its response came from: the hidden trait of every call has `answered_from(url)`, which stores that URL in `Options`. `finish_for_send` resolves a relative `at_url` against it; a call that no state machine touched (a caller's own `call.at_url("../next")`) has none, and resolves against the URL that it builds. The resolution comes before the origin check and before the edits.

## The attempts

One loop runs the attempts, for every kind of send. `__private::exchange` (a send) and `__private::exchange_raw` (`send_raw()`) both call it; they differ only in how an attempt reads its response.

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

An attempt that a middleware or the transport blocked (`TransportError::blocked()`) ends the attempts: the blocked error is the result. A slow log or any other delay near the end of `timeout` cannot lose the last outcome: the deadline is taken once, and the outcome is kept until the wait is over.

### One attempt

An attempt is the whole exchange for one copy of the request ([C3.5](../reference/contracts.md)):

1. The request goes out through the chain ([the way out](#the-way-out-middleware-then-the-transport)): the middleware, then the transport. A `TransportError` from any of them is the outcome: a Transport error with its class.
2. With the `reqwest::Response`, the engine calls `one_try(response, info)`. A response with no URL of its own (one that a caller's transport, a `fake` or a `tower` service built from an `http::Response`: `reqwest` assigns it a placeholder URL) first receives the URL of its request, so that a relative next URL is resolved against the request that was sent. The docs of `Transport` and `fake` say that a response should carry its URL. `one_try` is the generated part. It looks at the status, picks the arm (an exact status first, then a class, then the simple rule), reads the body with the reader if the arm binds it or the simple rule needs it, and runs the arm.
3. The outcome is one of: a value; a Refused error (a status that no arm matched, an arm that ended with the API's problem, or an arm on a status that is not a 2xx whose `body` could not be read); a Mismatch (a 2xx whose body could not be read, or an arm that said so with `mismatch(…)` or `require(…)`); a Transport error (the body broke while it was read, or a timeout expired).

With `read Stream;`, a success ends the attempt at the head: nothing is read. An error response is read like any other (64 KiB, the problem type). For `send_raw()` the attempt always ends at the head; a head below `400` is final, like a value, and the policy looks at a head of `400` or above, as it does at an error response.

For an arrow that is an `Option<T>` or a `Result<T, E>`, the simple rule reads a `T` and wraps it (`Some`, `Ok`): the generated code does that, and the engine sees one value.

The helpers that generated code calls inside `one_try` are `read_body` (a reader, or a Mismatch; on a status that is not a 2xx, a Refused error with the status, the headers and the bytes kept, and no problem), `refused` and `refused_plain` (the error for a status that no arm matches, with at most 64 KiB of the body and the problem type tried on it), `no_arm` (a 2xx that no arm matches on a `HEAD` with an arrow), and `arm` (what an arm produced, as the result of the attempt). Each receives the `TryInfo`, and so the timeouts.

### The decision to retry

This is one function with no I/O, so that it can be tested on its own. The time left, the wall clock (for a `Retry-After` that is a date) and the random number are passed to it. It returns the wait before the next attempt, or nothing; and in both cases, what was requested (the response's `Retry-After`, and the wait from the `when` rule that returned one), so that `e.retry_after()` can return the longer of the two and no rule is consulted twice.

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

**`random` is drawn again for each wait.** Two waits of one call are independent: the loop passes a new draw each time it calls the function.

Step 4 opens the gate for an error response only: a body that broke, or a timeout that expired, after a head with a listed status does not open it. `time_left` may be passed to the function; the loop of [the attempts](#the-attempts) passes none, and compares its one deadline with the end of `timeout` itself, after the rules ran (a rule may take time).

`method` is the method of the request as it was built, after the edits of `edit_request` and before any middleware. `mark` is what the description says: nothing (ByMethod), or `idempotent <bool>;`, which is Yes or No for this call (`idempotent;` alone is Yes).

A policy holds every `when` rule and every `unless` rule that was added to it. A rule is consulted only at its step, so only when another attempt is still possible.

The function is in [`concord-core/src/retry.rs`](../../concord-core/src/retry.rs).

### The way out: middleware, then the transport

A client has one **transport**, which actually sends (`concord::Http` unless `with_transport` set another), and a list of **middleware layers**, which every request passes through on its way out: first the ones that a caller added with `with_middleware`, then the description's (`middleware m;`), in the order written. **The author's are the last before the wire**, and among them the author writes the signing one last.

- A middleware of the description is made once, in `new`, from the client's required inputs. Every handle and every call shares it, so it may hold what must be shared: a quota, a token.
- For each attempt the engine builds a `Next` over the whole list and calls `next.send(request, info)`. A middleware receives the request, the `TryInfo` and the `Next` for the rest of the way. It may change the request, wait, send it on once, send it on again (with a renewed token), or not send it (`TransportError::blocked()`).
- **The last `Next` leads to the transport.** There the engine checks `timeout` (no request is handed to the transport after it has expired), starts the idle timeout, and calls the transport's `send`.
- `with_transport` replaces the transport and leaves the list as it is. So a fake in a test receives the request after every middleware: a signed API is signed in its tests too.
- What Concord counts is one attempt. What a middleware sends on its own inside that attempt is its own business, and so is what it changes: the retry check (the method) and the page checks (the origin, "the same request") look at the request as it was built, before any middleware.
- A middleware receives the request as it was built, with the edits of `edit_request` already applied (step 3 of [the path of one call](#the-path-of-one-call)).

## The two timeouts

What each timeout bounds, its default, and the order in which its length is chosen are stated in [rule 19](../reference/rules.md#behaviour) and [C7.6](../reference/contracts.md#c7-concords-own-client-and-the-timeouts). This section says how the engine runs them. `timeout` starts at step 1 of [the path of one call](#the-path-of-one-call), before the request is built (for a page: when that page is requested). `idle_timeout` starts each time the transport receives a request, and each time a wait for a chunk of a body begins. Its length is read once per send, from the call's `Options`, the description's statement (evaluated with the inputs), and the `Core`.

When one of them expires, the call ends with a Transport error of class Timeout, whose note names the timeout, what it waited for (a response, or a body after its status), and where its length is set (the lines for a request that is built or answered, in [the messages](../reference/messages.md)).

**A timeout ends a wait, and nothing else.** Both are Tokio timers beside a wait. Work that can finish without waiting does finish, even after the instant has passed: the work is polled first, and the timer second. Code that does not yield is not interrupted. One check is added, because a send is an effect: **no request is handed to the transport after `timeout` has expired** ([the way out](#the-way-out-middleware-then-the-transport)).

**When two timeouts expire at the same instant, the one that covers more is reported.** The call's `timeout` wins over the transport's connect timeout and over the idle timeout. So when `timeout` and the idle timeout of a body expire together, the error is the call's Timeout. A wait between two attempts is a different case: it is not started when it would end after `timeout`, and when `timeout` still cuts short one that was started, the caller gets the last outcome, unchanged, not a timeout ([C4.3](../reference/contracts.md)).

**The idle timeout watches two things, and nothing else.**

| What it watches | From | Until | What restarts it |
|---|---|---|---|
| The transport, while it has the request | The hand-over (the last `Next`: [the way out](#the-way-out-middleware-then-the-transport)) | The head of the response (the return of the transport's `send`) | Each chunk of the request body that is taken, when the body is a stream: the engine wraps the stream. |
| A body, while it is read (`Incoming::bytes`, `Incoming::stream`, a `ByteStream` in the caller's hands) | Each wait for a chunk | That chunk | — |

So the engine needs no shared instant for a whole attempt: one timer around the transport's `send`, and one around each wait for a chunk. A middleware's time, before or after the transport, never counts as silence, and neither does a reader's own wait for something other than the body. Only `timeout` bounds those. The idle timeout of a `ByteStream` runs while a `try_next()` waits, not between two of them: dropping that future resets it. Through `impl Stream for ByteStream`, it runs from the first poll that found nothing until the next chunk, even between two polls, because a dropped `next()` cannot be detected. A `ByteStream` may read ahead of what the caller asked for. A failure while it is read is a `concord::read::StreamError` (its kind is Transport), which `?` converts into any `concord::Error<P>` or a `ReadError`. After `send_raw()`, neither timeout follows the response that the caller holds.

**An outcome that is already known is never lost to a timeout** ([C5.7](../reference/contracts.md)). There are two such outcomes: an error response, and the Mismatch of `no_arm`. If the idle timeout expires while its body is read, `refused` (or `no_arm`) gets the timeout from the read, and returns the outcome that it knows, with what it has of the body and `kept()` saying `Interrupted`. If `timeout` expires, the engine, which waits beside the whole attempt, drops the attempt; so `refused` and `no_arm` write what they know into the `Run` as it arrives (the kind, the status, the headers, then each chunk up to 64 KiB), and the engine looks there before it returns the Timeout error.

An `Incoming` that the engine creates for an attempt has the `Run` (the length of the idle timeout, and the end of `timeout`). One that a test creates with `Incoming::new(response)` has no timeout.

Two limits, stated plainly. A chunk of a stream body that the library pulled is not a chunk that the server received: the library pulls while its buffers have room, so a stalled connection is detected one buffer later. And a body held in memory is given to the transport as bytes, in one piece, so that a transport or a middleware can read it (a signer hashes it): the idle timeout then sees nothing until the head, and a large upload from memory on a slow connection needs a longer `idle_timeout` (on the call, or on the endpoint in the description).

## A drop

Everything of a call lives in its `Sending`, and Concord makes progress on it only when it is polled.

| What is dropped | What Concord promises |
|---|---|
| A call that was not sent | Nothing happened. No request was built. |
| `Sending`, before the first poll | Nothing happened. |
| `Sending`, at any later moment | Concord stops, and starts no further attempt. **The request may have reached the server.** What the HTTP library does with the connection (close it, drain it, reuse it) is up to the library. |
| A `ByteStream` | Concord reads no more of it. |

`Sending` is not cancel safe, like any request: a `select!` that drops it has lost the call, and cannot know whether the server saw it. `Pages` is different (see [pages](#pages)).

**What Concord logs:** one event, `retry`, through `tracing` with the target `concord`, at the debug level, each time it decides to retry. Its fields: `endpoint` (the name of the endpoint), `try` (the number of the attempt that failed), `status` (when the failed attempt had a response) or `class` (when it had none), and `wait_ms` (the wait before the next attempt, in whole milliseconds) ([C10.6](../reference/contracts.md#c10-the-generated-code-and-the-log)). Nothing else: a middleware sees every attempt and every response.

## Concord's own HTTP client

`concord::Http` is a `reqwest::Client` that Concord builds, wrapped in a type of its own. See [the crates](../reference/crates.md#concords-own-http-client) for its settings, and for what `Http::with(f)` lets `f` change. `Http::with` sets the defaults on `reqwest`'s builder, runs `f`, and then sets `redirect::Policy::none()`, so that no `f` can turn redirects on. `Http::new()` is `Http::with(|b| b)`. Both functions are in [`transport.rs`](../../concord-core/src/transport.rs).

**The class of a failure.** `Connect` means that a connection failed before a head was received, even after the connection was established, with any client: an `io::Error` of kind `ConnectionRefused`, `ConnectionReset`, `ConnectionAborted`, `BrokenPipe`, `NotConnected` or `UnexpectedEof` in the chain, or any `reqwest` error of the kind "request" with no head that is not a timeout. A failure while the body of a response is read is class `Body`, whatever its chain. `Other` is whatever is none of these, nor a timeout, nor blocked. A `reqwest` error about a status or about decoding keeps no source. Connect says nothing about what was sent, and Concord infers nothing from it: a request that may not be repeated is not retried after it, because a failed connection does not prove that nothing was sent. What `reqwest` itself retries below this level (a stream that the server rejected before processing it) is left as `reqwest` has it, and is not counted as an attempt ([C4.4](../reference/contracts.md)).

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

- **The request of a page is built when that page is requested**, once, by the first half of a send, and the second half then sends that same request: nothing is built twice. It is built from a clone of the call, so that the call stays whole for the pager. A `Body` that is a one-shot stream (an input) is shared by the clones and taken by the first page; building a second page is then a Build error (`the body is a stream that was already sent`), and nothing is sent. An expression that makes a new stream each time it runs gives each page its own. The same holds for a body made of bodies (`Body::concat`, on which `multipart` is built): its one-shot streams are taken with the whole when the request is built for a send (before any middleware), so a stream that was already sent is detected before anything goes out. As a result, a `Secret` that was replaced between two pages is used, the edits of `edit_request` run once for each page, and `timeout` covers the building of every page.
- **Every page of one `.pages()` goes to the origin of its first request,** whatever the pager did: a pager that sets an input which appears in the host cannot carry a key to another place.
- **"The same request"** means the same method, the same URL, the same body bytes and the same headers, **where what a secret wrote in the URL or in a header is left out**: a segment of a path, a pair of the query, a value of a header (the builder knows which parts these are; for a header it is the `sensitive` flag, so a value that `edit_request` marks is left out too). The fragment of a URL is left out as well. A body is compared by its bytes: a secret that is an entry of a body, replaced between two pages, looks like progress, once. A body that is a stream has no bytes to compare, so a request with one is never "the same request". The marks have two limits: a secret in a host that was replaced makes another origin, and that check comes first, so the pages end with its Build error; and after an `edit_request` that changed the URL, the parts that a secret wrote are no longer known, so the URL is compared as it is. So an API that paginates by a header makes progress, and a key that was replaced between two pages does not look like progress, whether it is in a path, a query or a header.
- **`at_url` sets a path and a query, never an origin.** A URL at an origin other than the one that the call builds (its `base`, or `with_base`) is a Build error, in pages and outside them. It is checked when the request is built, in step 3, before the edits of `edit_request`. So the two checks are separate: `at_url` cannot leave the origin that the description sets, and the pages cannot leave the origin of their first request. An `edit_request` that changes the host cannot be combined with a pager that uses `at_url`: its origin check uses the call before the edits. Use `with_base` to move the client. A relative URL is resolved against the URL of the response that contained it; with no response (a caller's own `call.at_url("../next")`), against the URL that the call builds.
- If a next URL already has a pair with the key of a secret pair, every pair with that key is removed, and the secret is written once.
- A relative next URL is resolved against the URL of the response that contained it; an `Answer` keeps that URL, hidden.
- **A `try_next()` that is dropped loses nothing** ([C8.5](../reference/contracts.md)). The exchange in flight is in the state, not in the future that was dropped.
- Concord does not advance an exchange that nobody polls. Its timeouts are instants, so they keep running. A wait that is still open at the next poll, after its timeout has expired, ends with a timeout. A response that is already complete is returned.
- Each page is one call, with the options of the call. The time that the caller spends between two pages is not counted. Nothing bounds the whole loop; that takes a timer around it, in the caller.
- `pages.items()` yields each item of each page. It asks one thing of the page type, `concord::page::IntoItems`, which a `Vec<T>` implements and which an author implements for their own list type. An `Option` and a `Result` do not implement it.
- A paginated call has no `.await` of its own: `.send()` returns one page, and `.pages()` returns this state machine.

## Reading a response that you obtained yourself

`call.read_response(response)` is the reading step of [an attempt](#one-attempt), on its own: the arms, the reader and the problem type of the endpoint, applied to a `reqwest::Response` that came from anywhere.

```rust
let call = riot.matches(Europe).get(&id);
let response = my_client.execute(call.request()?).await?;
let game = call.read_response(response).await?;      // riot::Result<Match>
```

No retry, no timeout and no pages: those belong to a send, and Concord sent nothing. On an error from `read_response`, `tries()` is 0.

**A conditional send.** `call.send_if_none_match(etag)`, on a call whose method is `GET` or `HEAD`, writes `If-None-Match` when the request is built (replacing one that the description wrote, and before the edits of `edit_request`), and sends. A `304` that no arm matches produces an `Answer` whose value is `None`; anything else produces `Some(value)`. The headers are there in both cases. **It has its own first half** (see [the path of one call](#the-path-of-one-call)): for a `GET` or a `HEAD`, `call_methods!` generates `__prepare_if_none_match(etag)` beside `prepare`. It is `prepare` with two differences: the header is written after the description's headers and before `finish_for_send` runs the edits (so a caller's edit that removes `if-none-match` removes this one too), and what reads the response returns `None` for a `304` that no arm matches. It returns a `Prepared` whose value is `Option<T>`; `send_if_none_match` is that half, then `Prepared::send`.

## What a call costs

Measured with `just bench` (`criterion`, outside the gate), on the `Script` transport of the test kit (no socket), with the clients of [`tests/tour`](../../tests/tour/src), against the same request written by hand on `reqwest` alone with the same stand-in transport. A counting allocator counts each call to `alloc` and `realloc`; creating the client is not counted. Time is in microseconds, followed by allocations.

| Operation | Concord | `reqwest` by hand |
|---|---:|---:|
| Build a `GET` (two path inputs, three query pairs added by `edit_request`) | 1.656 µs · 33 | 0.626 µs · 6 |
| Send that `GET` | 3.763 µs · 86 | 1.312 µs · 25 |
| A `POST` with a JSON body of entries | 3.613 µs · 93 | 1.248 µs · 35 |
| A call attempted three times | 6.082 µs · 109 | 1.642 µs · 46 |
| A loop of ten pages | 34.892 µs · 807 | 7.104 µs · 141 |
| 1 MiB read as `Json` | 376.096 µs · 18 | 247.581 µs · 13 |
| 1 MiB read as `Text` | 296.223 µs · 17 | 192.269 µs · 13 |
| 1 MiB read as `Binary` | 250.229 µs · 16 | 0.584 µs · 12 |
| 1 MiB read as `Stream` | 0.662 µs · 15 | 0.545 µs · 11 |

**The budget:** Concord adds less than 2 µs and less than 25 allocations to building a plain `GET`, over building the same `reqwest::Request` by hand.

**The measurement against the budget** goes through a client that `concord::api!` generates in the bench itself ([`concord-api/benches/macro_get.rs`](../../concord-api/benches/macro_get.rs)), on the endpoint that the budget names: **two path inputs and three `query` inputs**, with a secret header from the client. It measures the time and the allocations that Concord adds to building the request, over the same `reqwest::Request` built by hand. Concord adds **1.07 µs and 23 allocations** (1.695 µs and 29 allocations, against 0.621 µs and 6). The same measurement on `Shapes::guarded` of `tests/tour-gen` (one path input and two `query` inputs, with secrets among the values) adds **0.78 µs and 23 allocations**. Both are inside the budget of 2 µs and 25 allocations. The table above is measured on a different path: the hand-written client of `tests/tour`, with the three pairs added by `edit_request`, so the `query` of a description does not run there.

What Concord adds pays for owning the inputs, the checks of every value, the marks of secrets, the timeouts, retry, pages, the limits and the evidence kept in an error.

Not measured: how late the wrapped stream of [the two timeouts](#the-two-timeouts) detects a stalled connection.
