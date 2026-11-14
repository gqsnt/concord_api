# Retries

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Credentials and middlewares](credentials.md) · next: [Pages](pages.md)*

For the author and the user of a client: when Concord sends a request again, and how to choose a policy. Retry is off until you set it.

The split: the description says the fact of the API (this `POST` is safe to send twice: `idempotent;`), a `Retry` value says whether to try again and how hard, and the application chooses that value where it builds the client: `with_retry(…)`. The description never holds a policy: how often to try again is different for a command-line tool and for a night job.

## Off until you set it

```rust
let riot = Riot::new(key).with_retry(Retry::standard());     // for every call of this client
let game = asia.get(id).with_retry(Retry::none()).await?;         // for this call only
```

## Three questions

**Three questions decide whether a request goes out again.** All three must say yes ([rules](../reference/rules.md), rule 18; the exact conditions are C4.1 of the [contracts](../reference/contracts.md)).

| The question | The answer |
|---|---|
| **Can it be sent again?** | Yes, unless its body is a stream. |
| **May it be repeated?** | Yes, in three ways. Its method says that repeating it changes nothing more, as HTTP defines it (`GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`). Or its author wrote `idempotent;`. Or the API refused it with a status that the policy lists with `not_applied_on`. A `POST` that is not marked is not sent again after a connection that failed: its caller gets the error (`e.class()` says `Connect`), and decides. |
| **Is the failure worth another try?** | Yes for the failures of the network and the statuses that the standard table names ([C4.2](../reference/contracts.md)), and for what a rule of yours adds. |

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

## A status proves nothing

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

## The standard policy

**`Retry::standard()`** is a few tries, with a random wait that grows from one try to the next. (The random part matters: many clients that wait the same time come back together.) When the answer has `Retry-After`, the wait is never less than that. It is seconds or a date, and Concord reads both: a date in the past is a wait of zero, and one that cannot be read is as no `Retry-After`. A `Retry-After`, or a wait that a rule of yours asks for, over 60 seconds means no other try: the error is the result, and `e.retry_after()` says how long. It gives the longer of the two waits: the `Retry-After` of the answer, and what a rule of `when` asked for the try that did not happen. The waits are fixed, and so is the table of what is worth another try: the numbers are C4.2 and C4.3 of the [contracts](../reference/contracts.md). Each wait is logged, as one event ([what Concord logs](promises.md#what-concord-logs)).

## Rules of your own

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
| When they are asked | Only when another try is still possible: tries are left, and the two other questions said yes ([the engine](../internals/engine.md)). |

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

A policy of yours, played in a test on virtual time with the time of each try: [retry on virtual time](testing.md#retry-on-virtual-time).

## What never changes

| | |
|---|---|
| A try is the whole exchange | The request out, the head in, and the body in when it is read whole. |
| The request is built once | The same bytes are handed to the transport again, as a copy for each try. |
| A stream that was handed over is outside | With `read Stream;` the try ends at the head. |
| When the tries run out | The last outcome is the result: a Refused error with status 429 and its headers, for example. `e.tries()` says how many tries were made: a try is counted when the request is handed to the middlewares. |
| A body that does not decode | Never tried again: it would be the same body. |
| What is sent again without Concord | `reqwest`'s own resend (a stream that the server refused before processing it) is left as `reqwest` has it. It is below what Concord counts: `tries()` does not see it. |
| `send_raw()` | The try ends at the head. A head below `400` is final, like a value. A head of `400` or above is looked at by the policy, as a refusal is; a rule of yours gets no `problem()`: nothing of the body is read. |
| The clocks | `timeout`, when you set one, covers every try and every wait between tries ([time](calling.md#time)). |

## What stays outside: rate limiting

*Reacting* to one answer is retry. *Counting ahead* is rate limiting: slowing down before the server complains, sharing a quota between tasks, a budget of retries for a whole program. That is outside Concord, and its door is a middleware.

| Outside | Why | The door |
|---|---|---|
| Rate limiting, a quota shared by all calls, a budget of retries | State shared by every call, ahead of the answers. | A middleware: the author's for a quota of the API, a caller's (`with_middleware`) for a budget of their own ([a middleware](credentials.md#a-middleware)). It sees the endpoint and the number of each try, it can wait (its wait is not silence), and it can refuse to send. |

Why it is outside: [scope and limits](../design/scope.md).
