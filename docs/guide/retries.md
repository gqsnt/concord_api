# Retries

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Middleware](middleware.md) · next: [Rate limits](limits.md)*

This page explains when Concord retries a request, and how to choose a retry policy.

The work is split three ways. The description states the fact about the API (this `POST` may be repeated: `idempotent;`). A `Retry` value decides when to retry and how often. The application chooses that value where it builds the client: `with_retry(…)`. The description never holds a policy, because how often to retry is different for a command-line tool and for a nightly job.

## Retries are off until you turn them on

This is Concord's own engine. With the default transport, `concord::Http`, and the middleware layers that Concord ships, each attempt writes the request at most once; a middleware layer or a `reqwest::Client` of your own can send it again ([what can send a request twice](#what-can-send-a-request-twice)).

```rust
let riot = Riot::new(key).with_retry(Retry::standard());     // for every call of this client
let game = asia.get(id).with_retry(Retry::none()).await?;         // for this call only
```

## Three questions decide a retry

A request is retried only if the answer to all three questions is yes ([rules](../reference/rules.md), rule 18; see C4.1 of the [contracts](../reference/contracts.md) for the exact conditions).

| The question | The answer |
|---|---|
| **Can it be sent again?** | Yes, unless its body is a one-shot stream: one made by `Body::stream`, or by `Body::map_stream` over bytes in memory or over a one-shot stream ([a body that is a stream](bodies.md#a-body-that-is-a-stream)). A body of bytes is sent again as it is, and a `Body::repeatable` is opened again ([a body that is opened again](bodies.md#a-body-that-is-opened-again-for-each-attempt)). |
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

Tested: [`write_key.rs`](../../tests/real-apis/fixtures/recipes/write_key.rs), run by [`writer_recipes.rs`](../../tests/real-apis/tests/writer_recipes.rs), which compares this block with the file.

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

**`Retry::standard()`** makes a few attempts, with a random wait that grows from one attempt to the next. (The randomness matters: many clients that wait the same time would all retry at once.) When the response has a `Retry-After`, the wait is never shorter than that. `Retry-After` is either a number of seconds or a date, and Concord reads both: a date in the past means a wait of zero, and a value that cannot be parsed counts as no `Retry-After`. When a `Retry-After`, or a wait that one of your rules requests, is longer than 60 seconds, no further attempt is made: the error is the result, and `e.retry_after()` says how long to wait. It returns the longer of two waits: the `Retry-After` of the response, and the wait that a `when` rule requested for the attempt that was not made. `headers.retry_after()`, on the headers of a response, reads the header alone. The waits are fixed, and so is the table of failures worth another attempt; see C4.2 and C4.3 of the [contracts](../reference/contracts.md) for the numbers. Each wait is logged as one event ([what Concord logs](promises.md#what-concord-logs)).

## Your own retry rules

**Your own policy** combines `Retry::standard().tries(5)`, `not_applied_on(status)`, and your own rules, `when` and `unless`, for what the standard table does not cover. Each rule receives the status, the headers and the API's own error, as your problem type. A rule starts from the client's own `Retry` (`github::Retry`, `riot::Retry`, next to `Error` and `Result` in the client's module), so that Rust knows the problem type. Each setter changes only the policy it returns, never a clone ([what a clone shares](middleware.md#what-a-clone-shares)).

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
| The request is built once | The same bytes are handed to the transport again, as a copy for each attempt. A `Body::repeatable` is not built again either: each attempt opens it again, from its first byte. |
| What is sent again | The request as it was built: its bytes and its headers, an idempotency key included ([a key made for one write](#a-key-made-for-one-write)). Each attempt passes through every middleware layer again, so a signer signs it again and a `Renewed` places its current token. A `Body::stream` is never sent again: a call that has one gets one attempt ([a body that is a stream](bodies.md#a-body-that-is-a-stream)). No middleware layer that Concord ships sends a request a second time on its own: after a `401`, `after_401` asks the engine for the next attempt, under these same rules. |
| A stream handed to you is outside the attempt | With `read Stream;`, the attempt ends at the head. |
| When the attempts run out | The last outcome is the result: for example, a Refused error with status 429 and its headers. `e.tries()` says how many attempts were made: an attempt counts once the request is handed to the middleware. |
| A body that does not decode | Never retried: it would be the same body. |
| What is sent again below the engine | With `concord::Http` and the middleware layers that Concord ships, nothing that can put a request on the wire twice ([what can send a request twice](#what-can-send-a-request-twice)). |
| `send_raw()` | The attempt ends at the head. A head with a status below `400` is final, like a value. A head of `400` or above is checked by the policy, as an error response is, but your rules get no `problem()`, because nothing of the body is read. |
| The timeouts | The call timeout, `timeout`, when you set one, covers every attempt and every wait between attempts ([timeouts](calling.md#timeouts)). |

### What can send a request twice

**`concord::Http` resends nothing on its own.** It builds its `reqwest` client with `reqwest::retry::never()` and `reqwest::redirect::Policy::none()`, both applied after `Http::with(f)`, so a retry or redirect policy that `f` sets is replaced ([the client](calling.md#the-client)). Within one call that uses `Http` and only the middleware layers that Concord ships, each attempt writes the request at most once, and every repetition is an attempt of the engine: `e.tries()` counts it, the retry policy decides it, and it happens only for a request that may be repeated ([three questions decide a retry](#three-questions-decide-a-retry)). With no retry policy, a call makes one attempt, or none when its request cannot be built. A page of a paginated call is a request of its own ([pages](pages.md)).

- **The connection pool** under `reqwest` sends a request again, on another connection, when a reused connection proved unusable and handed the request back before writing any of it, whatever its body and as often as that happens. That never puts a request on the wire twice, and it is not an attempt.
- **After an HTTP/2 refusal** (a `REFUSED_STREAM` reset, or a `GOAWAY` that leaves its stream out), a `GET` with no retry policy can fail with `Connect`. `Retry::standard()` permits another counted attempt when the request may be repeated, within its number of attempts and the call's `timeout`.
- **A `reqwest::Client` of your own**, passed to `with_transport`, resends and follows redirects as its own policies say, below the engine, where `e.tries()` does not count them and the policy does not decide them. By default, with HTTP/2, it sends a request whose body is in memory or empty again, up to twice and whatever its method, after a remote `REFUSED_STREAM` reset or a `GOAWAY` with `NO_ERROR`, deciding on the error alone; and it follows up to ten redirects, sending the method and a body that can be copied again after a `307` or a `308`. Build it with `.retry(reqwest::retry::never())` and `.redirect(reqwest::redirect::Policy::none())` to prevent both.
- **A middleware layer or a transport of your own** can send a request again: a second `next.send` inside one attempt is not an attempt ([C9.3](../reference/contracts.md#c9-the-transport-and-tests)).
- **A proxy or a gateway** between the client and the server can forward a request again; what it does is outside what the client can state.

**A failure that states that nothing was sent.** `e.request_not_sent()` is `true` when Concord states that no byte of the call's request was written and none can still be: after a `blocked()` failure, a connector failure that `Http` recognizes, or a `TransportError::not_sent()` of your own transport or middleware. Which failures count, and when the call's error reports it: [an exchange that was not sent](credentials.md#an-exchange-that-was-not-sent) and [C9.5](../reference/contracts.md#c9-the-transport-and-tests). It lets a caller reuse a single-use credential that the request carried, as `RenewalError::from_call` does. `false` means that the request may have been written, or that Concord cannot tell. The retry policy does not read it, and the class of an error alone never decides whether a request may be sent again.
