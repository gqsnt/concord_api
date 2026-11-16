# Retries

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Credentials and middleware](credentials.md) · next: [Pages](pages.md)*

This page explains when Concord retries a request, and how to choose a retry policy.

The work is split three ways. The description states the fact about the API (this `POST` may be repeated: `idempotent;`). A `Retry` value decides when to retry and how often. The application chooses that value where it builds the client: `with_retry(…)`. The description never holds a policy, because how often to retry is different for a command-line tool and for a nightly job.

## Retries are off until you turn them on

```rust
let riot = Riot::new(key).with_retry(Retry::standard());     // for every call of this client
let game = asia.get(id).with_retry(Retry::none()).await?;         // for this call only
```

## Three questions decide a retry

A request is retried only if the answer to all three questions is yes ([rules](../reference/rules.md), rule 18; see C4.1 of the [contracts](../reference/contracts.md) for the exact conditions).

| The question | The answer |
|---|---|
| **Can it be sent again?** | Yes, unless its body is a stream ([a body that is a stream](#a-body-that-is-a-stream)). |
| **May it be repeated?** | Yes, in three cases. Its method is idempotent as HTTP defines it, so repeating it has no further effect (`GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`). Or its author wrote `idempotent;`. Or the API rejected it with a status that the policy lists with `not_applied_on`. A `POST` that is not marked is not retried after a failed connection: the caller gets the error (`e.class()` says `Connect`) and decides what to do. |
| **Is the failure worth another attempt?** | Yes for network failures and for the statuses listed in the standard table ([C4.2](../reference/contracts.md)), and for whatever your own rules add. |

`idempotent` is a fact about the API, so its author writes it. What follows the keyword is a Rust expression that evaluates to a `bool`, with the inputs as variables. With no expression, `idempotent;` means `true`.

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

The rare `PUT` or `DELETE` that must not be sent twice declares `idempotent false;`. An API where every `POST` takes an idempotency key writes those statements once, in a part: `part keyed(idempotency_key: String) { headers { "idempotency-key" = idempotency_key; } idempotent; }`, and then `POST create(customer: NewCustomer, ..keyed)`. The method that counts is the method of the request as it was built, after any edits and before any middleware: if an `.edit_request(…)` turns a `GET` into a `POST`, that request is a `POST`.

## A key made for one write

When the provider accepts a new UUID for each write, the description can make it in the header expression. Add `uuid = { version = "1", features = ["v4"] }` to the application's dependencies:

```rust
concord::api! {
    pub client Payments {
        base "https://payments.example.com";
        POST create(amount: u64) {
            headers { "idempotency-key" = uuid::Uuid::new_v4().to_string(); }
            body concord::form { amount; }
            idempotent;
        }
    }
}
```

```rust
let payments = Payments::new().with_retry(concord::Retry::standard());
payments.create(100).await?;
payments.create(100).await?; // a second write, with another UUID
```

The key is made when the request is built, before its first attempt. The engine builds the request once and copies it for every attempt, so all attempts of that call carry the same key. A second call builds another request and makes another UUID. Which calls carry a key is the author's choice: put the header on the endpoints where the provider promises this behavior. `idempotent;` records that promise; the application still chooses whether to retry.

Constructing an unsent call makes no key. `request()` and `into_request()` each build a request, so a key observed through inspection is not the key of a later send. A separately sent clone also builds its own request. To repeat the same business operation across separate calls or process restarts, keep a caller-supplied key as an input, as in the Stripe example, rather than generating a fresh one in the description.

## A status proves nothing, unless the API promises it

A `429` usually means "not now, and nothing was done", but a status alone does not prove what the server did: only the API can guarantee it. When the API does, the author, who knows the API, tells the policy:

```rust
// in the crate of the Slack client, beside the macro
pub fn retry() -> concord::Retry<SlackProblem> {
    concord::Retry::standard().not_applied_on(429)      // Slack rejects a request before it does anything
}

// in the application
let slack = Slack::new(token).with_retry(slack::retry());
```

A `POST` that is not idempotent is then retried after a `429`, and only then. `idempotent;` is the wrong tool for this: it would also resend the message after a lost connection, and post it twice.

## The standard policy

**`Retry::standard()`** makes a few attempts, with a random wait that grows from one attempt to the next. (The randomness matters: many clients that wait the same time would all retry at once.) When the response has a `Retry-After`, the wait is never shorter than that. `Retry-After` is either a number of seconds or a date, and Concord reads both: a date in the past means a wait of zero, and a value that cannot be parsed counts as no `Retry-After`. When a `Retry-After`, or a wait that one of your rules requests, is longer than 60 seconds, no further attempt is made: the error is the result, and `e.retry_after()` says how long to wait. It returns the longer of two waits: the `Retry-After` of the response, and the wait that a `when` rule requested for the attempt that was not made. The waits are fixed, and so is the table of failures worth another attempt; see C4.2 and C4.3 of the [contracts](../reference/contracts.md) for the numbers. Each wait is logged as one event ([what Concord logs](promises.md#what-concord-logs)).

## Your own retry rules

**Your own policy** combines `Retry::standard().tries(5)`, `not_applied_on(status)`, and your own rules, `when` and `unless`, for what the standard table does not cover. Each rule receives the status, the headers and the API's own error, as your problem type. A rule starts from the client's own `Retry` (`github::Retry`, `riot::Retry`, next to `Error` and `Result` in the client's module), so that Rust knows the problem type.

```rust
// GitHub responds with `403` and `retry-after` when a client goes too fast
let rule = github::Retry::standard().when(|tried| {
    let too_fast = tried.status().is_some_and(|s| s == 403);
    too_fast.then(|| tried.headers()?.retry_after()).flatten()
});

// a `503` that says "maintenance" is not worth waiting for
let rule = riot::Retry::standard().unless(|tried| tried.problem().is_some_and(|p| p.status.message == "maintenance"));
```

| | |
|---|---|
| `unless` | All `unless` rules are checked first. A single `true` stops the retries, whatever the table and `when` say. |
| `when` | `Some(wait)` means: retry, and wait at least this long. `None` leaves the decision to the next rule, and then to the standard table. A `when` rule can answer yes only to the third question. |
| Rules add up | Within one policy, the `when` rules are checked in the order they were added, and the first that returns a wait wins: the rules after it are not checked. A rule added later does not erase one that the author shipped. |
| `with_retry` on a call | It replaces the client's whole policy for that call, `tries` and rules alike: that is the caller's explicit choice. To keep an author's policy, build on it: `with_retry(github::retry().when(mine))`. |
| When rules are checked | Only when another attempt is still possible: attempts remain, and the other two questions were answered yes ([the engine](../internals/engine.md)). |
| A rule that ships | `concord::auth::after_401` (feature `auth`) is a `when` rule: one more attempt after a `401` on the first attempt, which a `Renewed` sends with a new token (`Retry::standard().when(after_401)`, [a token that is renewed](credentials.md#a-token-that-is-renewed)). It does not make an unmarked `POST` repeatable. |

**An API that says "later" in its own way.** Telegram puts the wait in the body. That is a fact about the API, so the author ships a policy for it beside the description:

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

To test your own policy on virtual time and check when each attempt happens, see [retry on virtual time](testing.md#retry-on-virtual-time).

## What holds for every retry

| | |
|---|---|
| An attempt is the whole exchange | Sending the request, receiving the head, and receiving the body when it is read whole. |
| The request is built once | The same bytes are handed to the transport again, as a copy for each attempt. |
| A stream handed to you is outside the attempt | With `read Stream;`, the attempt ends at the head. |
| When the attempts run out | The last outcome is the result: for example, a Refused error with status 429 and its headers. `e.tries()` says how many attempts were made: an attempt counts once the request is handed to the middleware. |
| A body that does not decode | Never retried: it would be the same body. |
| What is resent without Concord | `reqwest`'s own resend of an HTTP/2 request whose stream the server refused before processing it (`REFUSED_STREAM`) is left as `reqwest` does it. It happens below the level where Concord counts attempts: `tries()` does not see it. |
| `send_raw()` | The attempt ends at the head. A head with a status below `400` is final, like a value. A head of `400` or above is checked by the policy, as an error response is, but your rules get no `problem()`, because nothing of the body is read. |
| The timeouts | The call timeout, `timeout`, when you set one, covers every attempt and every wait between attempts ([timeouts](calling.md#timeouts)). |

## A body that is a stream

A call whose body is a stream is sent once, whatever its retry policy says: it gets one attempt, and `e.tries()` returns 1. The engine builds the request once and gives each attempt a copy of it, and a stream cannot be copied: its bytes are read as they are sent ([C3.2](../reference/contracts.md)). The policy's rules are not called for such a call, but `e.retry_after()` still returns the `Retry-After` of the response.

**To know beforehand.** `body.as_bytes()` returns the bytes of a body held in memory, and `None` when the body is a stream or contains one (a `multipart` with a part made by `Body::stream`). It does not read the stream or use it up. A body from `Body::new`, `json` or `form`, and a `multipart` whose parts are all in memory, holds bytes, and is retried like any other.

**To send a large upload again**, build the call again, with a new stream: the file, opened again. Your own loop then makes the attempts, a bounded number of times, after the failures that you judge safe to repeat. Here it sends a file with an `upload` endpoint like the one of [the tour](tour.md#an-endpoint-that-differs-says-so-in-its-own-block), a `PUT` whose body is its input `file`, on a client named `Files` (`PUT upload(name: String, file: concord::Body) { path "files/{name}"; body file; }`), and returns its errors as `BoxError`, written `type BoxError = Box<dyn std::error::Error + Send + Sync>;`:

```rust
use concord::error::Class;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio_util::io::ReaderStream;           // crate `tokio-util`, feature `io`; `tokio::fs` needs tokio's `fs`

/// Sends the file at `path` as `name`: at most three calls, within ten minutes.
async fn upload(files: &Files, name: &str, path: &Path) -> Result<(), BoxError> {
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut calls = 0;
    loop {
        let file = tokio::fs::File::open(path).await?;             // the file, opened again
        let body = concord::Body::stream(ReaderStream::new(file));  // a new stream for each call
        let left = deadline.saturating_duration_since(Instant::now());
        let error = match files.upload(name, body).with_timeout(left).await {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        calls += 1;
        // A `PUT` of the same file to the same name may be repeated; these failures are worth it.
        let again = error.class() == Some(Class::Connect) || error.is_status(503);
        let wait = error.retry_after().unwrap_or(Duration::from_secs(1));
        if !again || calls == 3 || wait >= deadline.saturating_duration_since(Instant::now()) {
            return Err(error.into());
        }
        tokio::time::sleep(wait).await;
    }
}
```

The exact block is kept in [upload_stream.rs](../../tests/real-apis/fixtures/recipes/upload_stream.rs) and run by [stream_recipes.rs](../../tests/real-apis/tests/stream_recipes.rs): the test reads both uploads in full, fails the first call, and checks the second call's bytes and the two request counts.

Each call sends the whole file. The loop now decides what the engine's retry would have checked:

| The engine checks | Your loop decides |
|---|---|
| May it be repeated? (The method, or `idempotent`.) | Whether this write may be sent twice. A `PUT` of the same file to the same name may; a `POST` that creates a file may create two, unless the API takes an idempotency key. |
| Is the failure worth another attempt? (The standard table, and your rules.) | Which errors to repeat: here a failed connection (`e.class()` says `Connect`) and a `503`. After a failed connection, part of the file may have reached the server. |
| How many attempts, and how long to wait. (A growing random wait, never shorter than `Retry-After`; no attempt after a wait over 60 seconds.) | A bound on the number of calls, and each wait: `e.retry_after()` returns the `Retry-After` of the response. |
| The whole time. (`timeout` covers every attempt and every wait.) | A deadline of your own: `timeout` on one call covers that call only, so the loop gives each call the time that is left. |

## Limits shared by the calls of a client

*Reacting* to one response is retrying. *Planning ahead* is rate limiting: slowing down before the server complains, and sharing a quota between the calls and the tasks that use it. That is state shared by every call, ahead of any response, so it is not a retry policy but a middleware layer: `concord::limit::Limit` (feature `limit` of `concord-api`). A limit only decides when a request may leave. It never sends a request twice and never changes a response: what happens after a response stays the retry policy's job.

The feature is off by default:

```toml
[dependencies]
concord-api = { git = "https://github.com/gqsnt/concord_api", features = ["limit"] }
```

**The smallest limit** waits after a `429` that has a `Retry-After`:

```rust
use concord::limit::Limit;

let limit = Limit::new();                                   // after a 429 with Retry-After, the next calls wait
let riot = Riot::new(key).with_middleware(limit.clone());   // clones share one state
```

The call that received the `429` gets it as usual: a Refused error, or another attempt if its retry policy allows one. Every call through the limit after it waits until the `Retry-After` has passed, and so does that other attempt. `Retry-After` is read as the retry policy reads it, as seconds or a date; zero or a date in the past holds nothing. A `429` without a `Retry-After` that can be read holds nothing either, unless you choose a wait for it: with `Limit::new().without_retry_after(Duration::from_secs(1))`, such a `429` holds what a `Retry-After` would hold, for one second.

The blocks of this section are fragments: the rustdoc of `concord::limit` has a whole example, with its description, and so does the [Riot Games](../examples/riot.md) page.

**A cap on the calls in flight:**

```rust
let limit = Limit::new().max_in_flight(4);                  // at most 4 requests at once
```

A request keeps its place until the body of its response has been read to the end, has failed, or has been dropped: with `read Stream;`, until you are done with the stream. A cap of zero is an error at the first call.

**A bucket** is one budget of the API (per key, per host, per endpoint), described in one place: which bucket a request is charged to, what is known of its windows before any response, what each response says about them, what a request costs, and which `429` is about it. A rate that is known in advance needs no response:

```rust
use concord::limit::{Bucket, Limit, Window};
use std::time::Duration;

let limit = Limit::new().bucket(
    Bucket::new(|_| Some("requests".into()))                   // every request is charged to this bucket
        .window(Window::rolling(10, Duration::from_secs(1))),  // at most 10 in any second
);
```

An API that reports its limits in the headers of each response is read with `learn`. Riot has a budget for the application on each host, and one for each method on each host, and tells which one a `429` is about:

```rust
use concord::limit::{counts, Bucket, Limit};

let limit = Limit::new()
    .bucket(Bucket::new(|call| Some(format!("{} application", call.request().url().host_str()?)))
        .learn(counts("x-app-rate-limit", "x-app-rate-limit-count"))
        .hit_when(|seen| seen.header("x-rate-limit-type") == Some("application")))
    .bucket(Bucket::new(|call| Some(format!("{} {}", call.request().url().host_str()?, call.info().endpoint())))
        .learn(counts("x-method-rate-limit", "x-method-rate-limit-count"))
        .hit_when(|seen| seen.header("x-rate-limit-type") != Some("application")));
```

A full window of Riot can last 120 seconds, longer than a call waits by default (`max_hold`, below). The [Riot Games](../examples/riot.md) page sets `max_hold` to that window, and a wait for a `429` that comes without `Retry-After`.

| | |
|---|---|
| `Bucket::new(key)` | The key names the budget that a request is charged to. `call.request()` is the request as built, and `call.info()` gives its endpoint and attempt number. `None` leaves the request out of this bucket. The same key returned by two buckets still names two budgets. |
| `.window(…)` | What is known before any response: `Window::rolling(capacity, period)` allows at most `capacity` in any `period`, and `Window::fixed(capacity, resets_in)` is a budget that the server refills once, at a moment that it states. A bucket has at most eight windows. |
| `.learn(…)` | Reads the windows from each response. Two readers are shipped: `counts(limits, counts)` for pairs such as `20:1,100:120` with their counts, and `remaining(limit, remaining, reset)` for a capacity, what is left and a reset in Unix seconds. Your own function receives a `Seen` (`status()`, `header(name)`, `until_unix(seconds)`) and returns the windows. A header that is missing or cannot be read teaches nothing. |
| `.cost(…)` | What a request costs in this bucket: 1 by default. A cost of 0 leaves the request out of the bucket. Two buckets may count one request differently, in points and in requests. |
| `.hit_when(…)` | Which `429` is about this bucket. A `429` with `Retry-After` holds every bucket of the call that has no `hit_when`, and every bucket whose `hit_when` returns `true`. When it holds none, every call waits. |
| A bucket with no known window | It lets one request through at a time, until the head of its response has been read: before that, nothing says how large the budget is. |

**What holds for every limit:**

| | |
|---|---|
| All or nothing | A request is charged to all its buckets and windows at once, or it waits. A cost is never given back once the request has left, even when the call then fails or is dropped. |
| A response never reopens a budget | What a response says can lower what is left of a window, never raise it above what the limit has already counted. |
| After a response | The limit never fails the call: the response is returned as it is, whatever its headers say. |
| How long a call waits | At most `max_hold`, 60 seconds by default. A wait that is known to be longer fails at once, and a wait of unknown length fails when `max_hold` ends. The call is then a Transport error of class blocked, nothing is sent, and its `source()` is a `concord::limit::LimitError::MaxHold`, with the wait that was still required (when it is known), the `Retry-After` that caused it (when one did), and the time already waited. `LimitError::find(&e)` returns it from the error of the call. A limit is a wait in a middleware: the idle timeout does not count it, and the call timeout, `timeout`, does. |
| Retries | An attempt that the engine makes again waits behind the same limits, like any other request. |
| Memory | At most `max_keys` keys, 1024 by default, counted across all the buckets: the same text returned by two buckets is two keys. When room is needed, a key that holds nothing is forgotten: no cost spent in a window that has not ended, no wait after a `429`, no request in flight until the end of its body, and no first response still awaited. Forgetting it forgets nothing that was spent: the windows given with `window` still apply, and the learned ones are learned again. When every key holds something, one more is `LimitError::TooManyKeys`, before anything is sent. A key is at most 512 bytes, and a rolling window keeps at most 4,096 entries: when it is full, its oldest entries are merged on the safe side, and no call fails for it. |
| Sharing | Clones of a `Limit` share one state: configure it fully, then give a clone to each client that uses the same allowance. Two values that are not clones of each other know nothing of each other. A `middleware Limit::new();` in the description gives each client made by `new` a state of its own. |
| Your own functions | A panic in a key, a cost, `learn` or `hit_when` never leaves another call waiting. No key, header or request is printed. |

**What a limit does not do:**

| Not provided | Why | The escape hatch |
|---|---|---|
| A limit shared between processes | The state of a `Limit` lives in one process. | A middleware layer of your own, with a store that the processes share. |
| A budget that is only in the body of a response | A limit reads the status and the headers, never the body. | A `when` rule, for a wait in the body (Telegram, [above](#your-own-retry-rules)), or a middleware layer of your own. |
| A retry budget for a whole program | It is a policy of the program across all its clients, about attempts rather than the budget of an API. | A caller's middleware layer (`with_middleware`), which sees the number of each attempt ([a middleware](credentials.md#a-middleware)). |

A middleware layer of your own sees the endpoint and the number of each attempt, it can wait (its wait does not count as silence), and it can refuse to send. See [scope and limits](../design/scope.md) for what else is outside Concord.
