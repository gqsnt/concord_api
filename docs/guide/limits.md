# Rate limits

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Retries](retries.md) · next: [Pages](pages.md)*

*Reacting* to one response is retrying. *Planning ahead* is rate limiting: slowing down before the server complains, and sharing a quota between the calls that spend it ([who shares a limit](#who-shares-a-limit)). That is state that lives across calls, ahead of any response, so it is not a retry policy but a middleware layer: `concord::limit::Limit` (feature `limit` of `concord-api`). A limit only decides when a request may leave. It never sends a request twice and never changes a response: what happens after a response stays the retry policy's job.

The feature is off by default:

```toml
[dependencies]
concord-api = { git = "https://github.com/gqsnt/concord_api", features = ["limit"] }
```

## The smallest limit

The smallest limit waits after a `429` that has a `Retry-After`:

```rust
use concord::limit::Limit;

let limit = Limit::new();                                   // after a 429 with Retry-After, the next calls wait
let riot = Riot::new(key).with_middleware(limit.clone());   // clones share one state
```

The call that received the `429` gets it as usual: a Refused error, or another attempt if its retry policy allows one. Every call through the limit after it waits until the `Retry-After` has passed, and so does that other attempt. `Retry-After` is read with `Headers::retry_after`, as the retry policy reads it, as seconds or a date; zero or a date in the past holds nothing. That is the header alone: a wait that a `when` rule of the retry policy requests holds nothing in the limit. A `429` without a `Retry-After` that can be read holds nothing either, unless you choose a wait for it: with `Limit::new().without_retry_after(Duration::from_secs(1))`, such a `429` holds what a `Retry-After` would hold, for one second.

The blocks of this page are fragments: the rustdoc of `concord::limit` has a whole example, with its description, and so does the [Riot Games](../examples/riot.md) page.

## A cap on the calls in flight

```rust
let limit = Limit::new().max_in_flight(4);                  // at most 4 requests at once
```

A request keeps its place until the body of its response has been read to the end, has failed, or has been dropped: with `read Stream;`, until you are done with the stream. A cap of zero is an error at the first call.

A cap can also belong to each key of a bucket. Xero allows five calls in progress at once for each tenant; a client of many tenants keeps a cap for all of them as well:

```rust
let per_tenant = Bucket::new(|call| Some(call.request().headers().get("xero-tenant-id")?.to_str().ok()?.to_owned()))
    .max_in_flight(5);                                     // at most 5 at once for each tenant
let limit = Limit::new().max_in_flight(20).bucket(per_tenant);   // and at most 20 in all
```

The two caps apply together, with the windows, all at once: a request that waits for its tenant's place holds nothing elsewhere, and one that finds no place before `max_hold` ends fails with `LimitError::MaxHold`, as any other wait. A bucket that has a cap and no `learn` lets up to its cap through at once, from the first call. A cap counts what this process holds, and it does not protect against three things:

| | |
|---|---|
| A call that is dropped | A call that its caller drops, or that its `timeout` ends, frees its place at once, even while the server still works on it: the server can count more calls than the cap for that time. |
| A body that is kept unread | A response whose body is kept and never read to its end holds its place for as long as it is kept. |
| Many keys at once | A key with a call in flight is never forgotten, so that its count is never lost. When every key that `max_keys` allows has a call in flight, a call with a new key fails at once with `LimitError::TooManyKeys`. |

## A bucket

A bucket is one budget of the API (per key, per host, per endpoint), described in one place: which bucket a request is charged to, what is known of its windows before any response, what each response says about them, what a request costs, and which `429` is about it. A rate that is known in advance needs no response:

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

A full window of Riot can last 120 seconds, longer than a call waits by default (`max_hold`, below). The [Riot Games](../examples/riot.md) page sets `max_hold` to that window, and a wait for a `429` that comes without `Retry-After`. A retry policy has a ceiling of its own, 60 seconds by default, for a `Retry-After`; raise it with `max_retry_after` to retry after such a `429` ([a longer `Retry-After`](retries.md#the-standard-policy)).

### A bucket that learns, before it knows anything

A bucket that learns and knows no window yet lets one call go at a time, until a response teaches it a window, and for good if the responses teach nothing. Two clients show what that costs. Anthropic's client learns its three budgets from the headers of each response; the head of a message comes only once the whole answer is written, so the first calls of a program wait for each other until one of them returns, and after that they go together. S3's client holds a host after a `503` (SlowDown), with a bucket that learns only from that status, and caps the calls in flight for the whole limit:

```rust
let limit = Limit::new().max_in_flight(8).bucket(
    Bucket::new(|call| call.request().url().host_str().map(str::to_owned)).learn(move |seen| {
        if seen.status() == 503 { vec![Window::fixed(1, pause).remaining(0)] } else { Vec::new() }
    }),
);
```

Its other responses teach nothing, so the requests to one host go one at a time, each until the head of the one before it, whatever `max_in_flight` allows. Both clients accept that: before a response, nothing says how large the budget is.

### A reset written as a date

`remaining_rfc3339` reads a reset such as `2026-11-17T12:00:30Z`, as Anthropic writes it in `anthropic-ratelimit-requests-reset`:

```rust
use concord::limit::remaining_rfc3339;

let requests = Bucket::new(|_| Some("requests".into())).learn(remaining_rfc3339(
    "anthropic-ratelimit-requests-limit",
    "anthropic-ratelimit-requests-remaining",
    "anthropic-ratelimit-requests-reset",
));
```

The time until the reset is counted from the response's own `Date` header, when it has one valid date, so a wrong clock on your machine does not move it; without one, from your clock. Then a clock that is fast by a few minutes makes each reset that much nearer, so the window refills that much early and the server answers with a `429`, and a reset nearer than the error looks past and teaches nothing; a clock that is slow makes calls wait longer than they need. A fraction of a second is rounded up, never down. The date is read strictly: `T`, `t` or a space between the date and the time, `Z`, `z` or an offset such as `+01:00`, and a leap second only at the end of a month; anything else teaches nothing, as does a capacity written with a sign (`+50`) or a header over 4,096 bytes. A response that reports less left, for the same reset or a later one, lowers what is left at once, and the later of the two deadlines is kept. A response that arrives late and names a reset whose refill was already given teaches nothing: it cannot give that refill twice. `Seen::until_rfc3339(text)` reads such a date in a `learn` function of your own, on the same clock.

| | |
|---|---|
| `Bucket::new(key)` | The key names the budget that a request is charged to. `call.request()` is the request as built, and `call.info()` gives its endpoint and attempt number. `None` leaves the request out of this bucket. The same key returned by two buckets still names two budgets. |
| `.window(…)` | What is known before any response: `Window::rolling(capacity, period)` allows at most `capacity` in any `period`, and `Window::fixed(capacity, resets_in)` is a budget that the server refills once, at a moment that it states. A bucket has at most eight windows. |
| `.learn(…)` | Reads the windows from each response. Three readers are shipped: `counts(limits, counts)` for pairs such as `20:1,100:120` with their counts, `remaining(limit, remaining, reset)` for a capacity, what is left and a reset in Unix seconds, and `remaining_rfc3339(limit, remaining, reset)` for the same with a reset written as a date, `2026-11-17T12:00:30Z` ([a reset written as a date](#a-reset-written-as-a-date)). Your own function receives a `Seen` (`status()`, `header(name)`, `until_unix(seconds)`) and returns the windows. A header that is missing or cannot be read teaches nothing. The list describes every window of the bucket, in order, and it is checked whole before any of it is used: when one of its windows is invalid (for example, a `remaining` above its capacity, a capacity of zero or a rolling period of zero), when it has more than eight windows, or when it does not match the windows already known for the key (another number of windows, or a window whose kind or rolling period differs from the known window at the same position; a learned fixed window that has become unknown again accepts either kind), nothing of it is learned from that response, and what was known stays. In a list that passes, a fixed window whose reset is older than the one already known, or already past, is skipped on its own, and the other windows are still learned. `counts` reads a count above the capacity as nothing left; `remaining` reads a remaining value above the capacity as nothing learned. So a function of your own clamps the `remaining` that it returns to the capacity, or the bucket learns nothing from that response. |
| `.cost(…)` | What a request costs in this bucket: 1 by default. A cost of 0 leaves the request out of the bucket. Two buckets may count one request differently, in points and in requests. |
| `.hit_when(…)` | Which `429` is about this bucket. A `429` with `Retry-After` holds every bucket of the call that has no `hit_when`, and every bucket whose `hit_when` returns `true`. When it holds none, every call waits. |
| `.max_in_flight(n)` | At most `n` requests of each key at once, each until the end of its body ([a cap on the calls in flight](#a-cap-on-the-calls-in-flight)). |
| A bucket with no known window | It lets one request through at a time, until the head of its response has been read: before that, nothing says how large the budget is. When the responses teach nothing, that lasts for good. A bucket that has `max_in_flight` and no `learn` is the exception: it learns nothing, and its cap is its limit. |

## Who shares a limit

A `Limit` is a handle to one state: what it has counted, the waits after a `429`, the requests in flight. Every clone of it shares that state, and two values that are not clones of each other know nothing of each other.

| | |
|---|---|
| The calls of one client | A client creates its middleware once, so every handle and every call of the client goes through the same `Limit`, and so does every clone of the client. To use the client from several tasks, give each task a clone of the client. |
| Several clients | Give each client a clone of the same `Limit`, with `with_middleware(limit.clone())`. Clients that spend one budget of the API then count their calls together. |
| A limit in the description | `middleware Limit::new();` runs once in each `new`, so every client made by `new` has a state of its own, shared only with its clones: two clients made by `new` do not wait for each other. When the clients of a program must share one budget, the caller adds the limit with `with_middleware`, or the description takes it as a required input of the client and names a clone of it (`middleware limit.clone();`). |
| Configure it fully, then clone it | `max_in_flight`, `max_hold`, `max_keys`, `without_retry_after` and `bucket` change the state that the clones share, so a setting made on one clone applies to every clone. Configure the limit before you hand out clones ([what a clone shares](middleware.md#what-a-clone-shares)). |
| One process | The state lives in the memory of one process ([what a limit does not do](#what-a-limit-does-not-do)). |

## What holds for every limit

| | |
|---|---|
| All or nothing | A request is charged to all its buckets and windows at once, or it waits. A cost is never given back once the request has left, even when the call then fails or is dropped. |
| A response never reopens a budget | What a response says can lower what is left of a window, never raise it above what the limit has already counted. |
| A reported capacity replaces a declared one | The windows given with `window` are where the budget of each key starts. When `learn` reports the same window, its capacity replaces the declared one from that response on, also upward: Riot gives a development key 20 requests a second and a production key far more, and the same code learns which one it has. What is left still only goes down. |
| After a response | The limit never fails the call: the response is returned as it is, whatever its headers say. |
| How long a call waits | At most `max_hold`, 60 seconds by default. A wait that is known to be longer fails at once, and a wait of unknown length fails when `max_hold` ends. The call is then a Transport error of class blocked, nothing is sent, and its `source()` is a `concord::limit::LimitError::MaxHold`, with the wait that was still required (when it is known), the `Retry-After` that caused it (when one did), and the time already waited. `LimitError::find(&e)` returns it from the error of the call. A limit is a wait in a middleware: the idle timeout does not count it, and the call timeout, `timeout`, does ([a layer that waits](middleware.md#a-layer-that-waits)). |
| Retries | An attempt that the engine makes again waits behind the same limits, like any other request. |
| Memory | At most `max_keys` keys, 1024 by default, counted across all the buckets: the same text returned by two buckets is two keys. When room is needed, a key that holds nothing is forgotten: no cost spent in a window that has not ended, no wait after a `429`, no request in flight until the end of its body, and no first response still awaited. Forgetting it forgets nothing that was spent: the windows given with `window` still apply, and the learned ones are learned again. When every key holds something, one more is `LimitError::TooManyKeys`, before anything is sent. A key is at most 512 bytes, and a rolling window keeps at most 4,096 entries: when it is full, its oldest entries are merged on the safe side, and no call fails for it. |
| Your own functions | A panic in a key, a cost, `learn` or `hit_when` never leaves another call waiting. No key, header or request is printed. |

## What a limit does not do

| Not provided | Why | The escape hatch |
|---|---|---|
| A limit shared between processes | The state of a `Limit` lives in one process. | A middleware layer of your own, with a store that the processes share. |
| A budget that is only in the body of a response | A limit reads the status and the headers, never the body. | A `when` rule, for a wait in the body (Telegram, [your own retry rules](retries.md#your-own-retry-rules)), or a middleware layer of your own. |
| A retry budget for a whole program | It is a policy of the program across all its clients, about attempts rather than the budget of an API. | A caller's middleware layer (`with_middleware`), which sees the number of each attempt ([how a middleware layer works](middleware.md#how-a-middleware-layer-works)). |

A middleware layer of your own sees the endpoint and the number of each attempt, it can wait (its wait does not count as silence: [a layer that waits](middleware.md#a-layer-that-waits)), and it can refuse to send. See [scope and limits](../design/scope.md) for what else is outside Concord.
